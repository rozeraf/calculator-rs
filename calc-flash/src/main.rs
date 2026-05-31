use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;
use rustyline::{DefaultEditor, error::ReadlineError};
use rug::{Integer, ops::Pow};

// --- Value ---

static FACT_CACHE: OnceLock<Vec<Integer>> = OnceLock::new();

fn fact_cache() -> &'static [Integer] {
    FACT_CACHE.get_or_init(|| {
        let mut v = Vec::with_capacity(1001);
        v.push(Integer::from(1u32)); // 0!
        for i in 1u64..=1000 {
            let next = v.last().unwrap().clone() * Integer::from(i);
            v.push(next);
        }
        v
    })
}


#[derive(Clone, Debug)]
enum Value {
    Float(f64),
    Int(Integer),
}

// Digits above which we don't inline-print but show summary instead.
const INLINE_LIMIT: usize = 10_000;

impl Value {
    fn to_float(&self) -> f64 {
        match self {
            Value::Float(f) => *f,
            Value::Int(n)   => n.to_f64(),
        }
    }

    fn to_integer(&self) -> Option<Integer> {
        match self {
            Value::Int(n) => Some(n.clone()),
            Value::Float(f) if f.fract() == 0.0 && f.is_finite() && f.abs() < 9e18 => {
                Some(Integer::from(*f as i64))
            }
            _ => None,
        }
    }

    fn is_zero(&self) -> bool {
        match self {
            Value::Float(f) => *f == 0.0,
            Value::Int(n)   => *n == 0,
        }
    }

    // Approximate digit count without full decimal conversion.
    // Uses bit length * log10(2). May be off by 1, good enough for display decision.
    fn approx_digits(&self) -> usize {
        match self {
            Value::Float(_) => 20,
            Value::Int(n) => {
                if *n == 0 { return 1; }
                let bits = n.significant_bits() as u64;
                // integer approximation of bits * log10(2), avoids float rounding
                (bits * 1233).div_ceil(4096) as usize
            }
        }
    }

    fn display_string(&self) -> String {
        match self {
            Value::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 { format!("{}", *f as i64) }
                else { format!("{f}") }
            }
            Value::Int(n) => {
                let approx = self.approx_digits();
                if approx <= INLINE_LIMIT {
                    n.to_string()
                } else {
                    // Never call to_string_radix(10) here — O(n log n), costs seconds
                    // for numbers with millions of digits. approx_digits() is O(1).
                    format!("[~{approx}-digit integer]  (use :full or :save <file>)")
                }
            }
        }
    }

    // NOTE: to_string_radix(10) belongs here and in tests only.
    // Never call it from display_string() or approx_digits().
    fn full_string(&self) -> String {
        match self {
            Value::Float(f) => format!("{f}"),
            Value::Int(n)   => n.to_string_radix(10),
        }
    }
}

// --- Token ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Integer(String),
    Float(String),
    Plus, Minus, Star, Slash, Percent, Caret, Bang,
    LParen, RParen, Eof,
}

// --- Lexer ---

struct Lexer { input: Vec<char>, pos: usize }

impl Lexer {
    fn new(input: &str) -> Self { Self { input: input.chars().collect(), pos: 0 } }
    fn peek(&self) -> Option<char> { self.input.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        self.pos += 1; c
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t')) { self.advance(); }
    }
    fn read_number(&mut self) -> Result<Token, CalcError> {
        let start = self.pos;
        let mut is_float = false;
        while let Some(c) = self.peek() {
            match c {
                '0'..='9' => { self.advance(); }
                '.' => { is_float = true; self.advance(); }
                'e' | 'E' => {
                    is_float = true;
                    self.advance();
                    // consume optional sign after exponent marker
                    if matches!(self.peek(), Some('+') | Some('-')) {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
        let s: String = self.input[start..self.pos].iter().collect();
        if is_float {
            // validate eagerly so the error message names the bad literal
            s.parse::<f64>().map_err(|_| CalcError::InvalidNumber(s.clone()))?;
            Ok(Token::Float(s))
        } else {
            Ok(Token::Integer(s))
        }
    }
    fn tokenize(&mut self) -> Result<Vec<Token>, CalcError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None    => { tokens.push(Token::Eof); break; }
                Some(c) => match c {
                    '0'..='9' | '.' => tokens.push(self.read_number()?),
                    '+' => { self.advance(); tokens.push(Token::Plus); }
                    '-' => { self.advance(); tokens.push(Token::Minus); }
                    '*' => { self.advance(); tokens.push(Token::Star); }
                    '/' => { self.advance(); tokens.push(Token::Slash); }
                    '%' => { self.advance(); tokens.push(Token::Percent); }
                    '^' => { self.advance(); tokens.push(Token::Caret); }
                    '!' => { self.advance(); tokens.push(Token::Bang); }
                    '(' => { self.advance(); tokens.push(Token::LParen); }
                    ')' => { self.advance(); tokens.push(Token::RParen); }
                    other => return Err(CalcError::UnexpectedChar(other)),
                },
            }
        }
        Ok(tokens)
    }
}

// --- Parser ---
//
// expr    = term   ((+ | -)  term)*
// term    = unary  ((* | / | %) unary)*
// unary   = - unary | power
// power   = postfix (^ unary)?   right-associative
// postfix = primary !*
// primary = number | ( expr )

struct Parser { tokens: Vec<Token>, pos: usize }

impl Parser {
    fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0 } }
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }
    fn parse(&mut self) -> Result<Value, CalcError> {
        let v = self.parse_expr()?;
        if self.peek() != &Token::Eof {
            return Err(CalcError::TrailingChars(format!("{:?}", self.peek())));
        }
        Ok(v)
    }
    fn parse_expr(&mut self) -> Result<Value, CalcError> {
        let mut l = self.parse_term()?;
        loop {
            match self.peek() {
                Token::Plus  => { self.advance(); l = v_add(l, self.parse_term()?)?; }
                Token::Minus => { self.advance(); l = v_sub(l, self.parse_term()?)?; }
                _ => break,
            }
        }
        Ok(l)
    }
    fn parse_term(&mut self) -> Result<Value, CalcError> {
        let mut l = self.parse_unary()?;
        loop {
            match self.peek() {
                Token::Star    => { self.advance(); l = v_mul(l, self.parse_unary()?)?; }
                Token::Slash   => { self.advance(); l = v_div(l, self.parse_unary()?)?; }
                Token::Percent => { self.advance(); l = v_rem(l, self.parse_unary()?)?; }
                _ => break,
            }
        }
        Ok(l)
    }
    fn parse_unary(&mut self) -> Result<Value, CalcError> {
        if self.peek() == &Token::Minus {
            self.advance();
            return Ok(v_neg(self.parse_unary()?));
        }
        self.parse_power()
    }
    fn parse_power(&mut self) -> Result<Value, CalcError> {
        let base = self.parse_postfix()?;
        if self.peek() == &Token::Caret {
            self.advance();
            return v_pow(base, self.parse_unary()?);
        }
        Ok(base)
    }
    fn parse_postfix(&mut self) -> Result<Value, CalcError> {
        let mut v = self.parse_primary()?;
        while self.peek() == &Token::Bang {
            self.advance();
            v = factorial(v)?;
        }
        Ok(v)
    }
    fn parse_primary(&mut self) -> Result<Value, CalcError> {
        match self.peek().clone() {
            Token::Integer(s) => {
                self.advance();
                // Integer::parse is lossless for any number of digits
                match Integer::parse(&s) {
                    Ok(incomplete) => Ok(Value::Int(Integer::from(incomplete))),
                    Err(_) => Err(CalcError::InvalidNumber(s)),
                }
            }
            Token::Float(s) => {
                self.advance();
                match s.parse::<f64>() {
                    Ok(f) => Ok(Value::Float(f)),
                    Err(_) => Err(CalcError::InvalidNumber(s)),
                }
            }
            Token::LParen => {
                self.advance();
                let v = self.parse_expr()?;
                if self.peek() != &Token::RParen {
                    return Err(CalcError::Expected("')'".into(), format!("{:?}", self.peek())));
                }
                self.advance();
                Ok(v)
            }
            other => Err(CalcError::UnexpectedToken(format!("{:?}", other))),
        }
    }
}

// --- Arithmetic ---

fn int_or_float2(a: Value, b: Value, fi: impl Fn(Integer, Integer) -> Value, ff: impl Fn(f64, f64) -> f64) -> Value {
    match (a.to_integer(), b.to_integer()) {
        (Some(x), Some(y)) => fi(x, y),
        _ => Value::Float(ff(a.to_float(), b.to_float())),
    }
}

fn v_add(a: Value, b: Value) -> Result<Value, CalcError> {
    Ok(int_or_float2(a, b, |x, y| Value::Int(x + y), |x, y| x + y))
}
fn v_sub(a: Value, b: Value) -> Result<Value, CalcError> {
    Ok(int_or_float2(a, b, |x, y| Value::Int(x - y), |x, y| x - y))
}
fn v_mul(a: Value, b: Value) -> Result<Value, CalcError> {
    Ok(int_or_float2(a, b, |x, y| Value::Int(x * y), |x, y| x * y))
}
fn v_div(a: Value, b: Value) -> Result<Value, CalcError> {
    if b.is_zero() { return Err(CalcError::DivisionByZero); }
    Ok(match (a.to_integer(), b.to_integer()) {
        (Some(x), Some(y)) => {
            let (q, r) = x.div_rem(y);
            if r == 0 { Value::Int(q) }
            else { Value::Float(a.to_float() / b.to_float()) }
        }
        _ => Value::Float(a.to_float() / b.to_float()),
    })
}
fn v_rem(a: Value, b: Value) -> Result<Value, CalcError> {
    if b.is_zero() { return Err(CalcError::DivisionByZero); }
    Ok(int_or_float2(a, b, |x, y| Value::Int(x % y), |x, y| x % y))
}
fn v_pow(base: Value, exp: Value) -> Result<Value, CalcError> {
    if let (Some(b), Some(e)) = (base.to_integer(), exp.to_integer()) {
        if e >= 0 {
            let e_u32 = match u32::try_from(&e) {
                Ok(v) => v,
                Err(_) => return Err(CalcError::ExponentTooLarge),
            };
            return Ok(Value::Int(b.pow(e_u32)));
        }
    }
    Ok(Value::Float(base.to_float().powf(exp.to_float())))
}
fn v_neg(v: Value) -> Value {
    match v {
        Value::Int(n)   => Value::Int(-n),
        Value::Float(f) => Value::Float(-f),
    }
}

// --- Factorial (Peter Luschny's Prime Swing algorithm) ---

fn factorial(v: Value) -> Result<Value, CalcError> {
    let f = v.to_float();
    if f < 0.0 || f.fract() != 0.0 { return Err(CalcError::FactorialDomain(f)); }
    let n = f as u64;
    if n <= 1000 {
        return Ok(Value::Int(fact_cache()[n as usize].clone()));
    }
    Ok(Value::Int(prime_swing_factorial(n)))
}

// Sieve of Eratosthenes, returns primes <= n.
fn sieve(n: u64) -> Vec<u64> {
    if n < 2 { return vec![]; }
    let mut composite = vec![false; (n + 1) as usize];
    composite[0] = true; composite[1] = true;
    let mut i = 2u64;
    while i * i <= n {
        if !composite[i as usize] {
            let mut j = i * i;
            while j <= n { composite[j as usize] = true; j += i; }
        }
        i += 1;
    }
    (2..=n).filter(|&k| !composite[k as usize]).collect()
}

// Exponent of prime p in n! via Legendre's formula: sum floor(n/p^k)
#[allow(dead_code)]
fn legendre(n: u64, p: u64) -> u32 {
    let mut exp = 0u32;
    let mut pk = p;
    while pk <= n {
        exp += (n / pk) as u32;
        pk = match pk.checked_mul(p) { Some(v) => v, None => break };
    }
    exp
}

// Balanced product of a slice of integers — parallel above PAR_THRESHOLD.
#[allow(dead_code)]
fn product_tree(factors: &[Integer]) -> Integer {
    const PAR_THRESHOLD: usize = 128;
    match factors.len() {
        0 => Integer::from(1),
        1 => factors[0].clone(),
        n if n <= PAR_THRESHOLD => {
            // sequential below threshold to avoid rayon overhead on tiny slices
            let mid = n / 2;
            product_tree(&factors[..mid]) * product_tree(&factors[mid..])
        }
        n => {
            let mid = n / 2;
            let (left, right) = rayon::join(
                || product_tree(&factors[..mid]),
                || product_tree(&factors[mid..]),
            );
            left * right
        }
    }
}

#[cfg(test)]
fn prime_factorial_legendre(n: u64) -> Integer {
    let primes = sieve(n);
    // For each prime, compute p^legendre(n,p) and collect into product tree.
    let factors: Vec<Integer> = primes
        .iter()
        .map(|&p| {
            let e = legendre(n, p);
            Integer::from(p).pow(e)
        })
        .collect();
    product_tree(&factors)
}

// ── Balanced product of an owned Vec<Integer> ────────────────────────────
//
// Taking Vec by value avoids cloning at the leaves; use this inside
// odd_swing where we already own the factors.

fn product_tree_owned(mut factors: Vec<Integer>) -> Integer {
    if factors.is_empty() { return Integer::from(1u32); }
    while factors.len() > 1 {
        let mut next = Vec::with_capacity(factors.len().div_ceil(2));
        let mut iter = factors.into_iter();
        while let Some(a) = iter.next() {
            match iter.next() {
                Some(b) => next.push(a * b),
                None    => next.push(a),
            }
        }
        factors = next;
    }
    factors.remove(0)
}

// ── Precomputed odd swings for n < 33 ────────────────────────────────────

const SMALL_ODD_SWING: [u64; 33] = [
    1, 1, 1, 3, 3, 15, 5, 35, 35, 315, 63, 693, 231, 3003, 429, 6435, 6435,
    109395, 12155, 230945, 46189, 969969, 88179, 2028117, 676039,
    16900975, 1300075, 35102025, 5014575, 145422675, 9694845,
    300540195, 300540195
];

// ── oddSwing ─────────────────────────────────────────────────────────────

fn odd_swing(n: u64, primes: &[u64]) -> Integer {
    if n < 33 {
        return Integer::from(SMALL_ODD_SWING[n as usize]);
    }
    let sqrtn = (n as f64).sqrt() as u64;
    let mut factors: Vec<Integer> = Vec::new();

    for &p in primes {
        if p == 2 { continue; }
        if p > n { break; }

        if p > n / 2 {
            // exponent is always 1 for primes in (n/2, n]
            factors.push(Integer::from(p));
        } else if p > sqrtn {
            // exponent is 0 or 1; include p iff floor(n/p) is odd
            if (n / p) % 2 == 1 {
                factors.push(Integer::from(p));
            }
        } else {
            // small primes: exponent = number of odd quotients in n/p, n/p², …
            let mut q = n / p;
            let mut exp = 0u32;
            while q > 0 {
                if q % 2 == 1 { exp += 1; }
                q /= p;
            }
            if exp > 0 {
                factors.push(Integer::from(p).pow(exp));
            }
        }
    }

    product_tree_owned(factors)
}

// ── oddFactorial ──────────────────────────────────────────────────────────

fn odd_factorial(n: u64, primes: &[u64]) -> Integer {
    if n < 2 { return Integer::from(1u32); }
    let of_half = odd_factorial(n / 2, primes);
    let swing   = odd_swing(n, primes);
    // of_half² × swing  — clone of_half before consuming it
    of_half.clone() * of_half * swing
}

// ── prime_swing_factorial ─────────────────────────────────────────────────

fn prime_swing_factorial(n: u64) -> Integer {
    if n < 2 { return Integer::from(1u32); }
    let primes = sieve(n);
    let odd    = odd_factorial(n, &primes);
    // Multiply by 2^(n − popcount(n)).
    // rug::Integer supports left-shift: value << bits.
    // n.count_ones() is popcount for u64 in Rust std.
    let shift = n - u64::from(n.count_ones());
    // shift fits in u32 for any n that is remotely computable
    odd << (shift as u32)
}

// --- Error ---

#[derive(Debug)]
enum CalcError {
    InvalidNumber(String),
    UnexpectedChar(char),
    UnexpectedToken(String),
    Expected(String, String),
    TrailingChars(String),
    DivisionByZero,
    FactorialDomain(f64),
    ExponentTooLarge,
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNumber(s)   => write!(f, "invalid number: {s}"),
            Self::UnexpectedChar(c)  => write!(f, "unexpected character: {c:?}"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token: {t}"),
            Self::Expected(e, g)     => write!(f, "expected {e}, got {g}"),
            Self::TrailingChars(t)   => write!(f, "unexpected trailing token: {t}"),
            Self::DivisionByZero     => write!(f, "division by zero"),
            Self::FactorialDomain(x) => write!(f, "factorial requires non-negative integer, got {x}"),
            Self::ExponentTooLarge   => write!(f, "exponent too large for integer power (max 2^32−1)"),
        }
    }
}

// --- Eval ---

fn eval(expr: &str) -> Result<Value, CalcError> {
    let tokens = Lexer::new(expr).tokenize()?;
    Parser::new(tokens).parse()
}

/// Returns true when the line starts with a binary operator,
/// indicating the user wants to chain from the last result.
fn is_continuation(s: &str) -> bool {
    matches!(s.trim_start().chars().next(), Some('+' | '-' | '*' | '/' | '%' | '^'))
}

// --- Entry point ---

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (single_expr, output_file) = parse_args(&args[1..]);

    if let Some(expr) = single_expr {
        match eval(&expr) {
            Ok(v) => {
                println!("{}", v.display_string());
                if let Some(path) = &output_file { append_to_file(path, &expr, &v.full_string()); }
            }
            Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
        }
        return;
    }

    println!("calc - operators: + - * / % ^ ! ()");
    println!("commands: :save <file>  :dump <file>  :full  :digits  :quit / Ctrl-D");

    let mut rl = DefaultEditor::new().expect("failed to init readline");
    let mut log_file: Option<String> = output_file;
    let mut last: Option<Value> = None;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }
                rl.add_history_entry(trimmed).ok();

                if let Some(path) = trimmed.strip_prefix(":save ") {
                    let path = path.trim().to_string();
                    println!("logging to: {path}");
                    log_file = Some(path);
                    continue;
                }
                if let Some(path) = trimmed.strip_prefix(":dump ") {
                    let path = path.trim();
                    match &last {
                        None    => eprintln!("no result to dump"),
                        Some(v) => match std::fs::write(path, v.full_string()) {
                            Ok(_)  => println!("saved to {path}"),
                            Err(e) => eprintln!("write error: {e}"),
                        },
                    }
                    continue;
                }
                match trimmed {
                    ":quit" | ":q" => break,
                    ":full" => {
                        match &last {
                            None    => eprintln!("no result"),
                            Some(v) => {
                                let s = v.full_string();
                                println!("{s}");
                                if let Some(path) = &log_file {
                                    append_to_file(path, ":full", &s);
                                }
                            }
                        }
                        continue;
                    }
                    ":digits" => {
                        match &last {
                            None => eprintln!("no result"),
                            Some(v) => {
                                let approx = v.approx_digits();
                                if approx <= INLINE_LIMIT {
                                    if let Value::Int(n) = v {
                                        println!("{} digits", n.to_string_radix(10).len());
                                    } else {
                                        println!("~{approx} digits");
                                    }
                                } else {
                                    println!("~{approx} digits (approximate)");
                                }
                            }
                        }
                        continue;
                    }
                    _ => {}
                }

                let expr_to_eval: String = if is_continuation(trimmed) {
                    match &last {
                        None => {
                            eprintln!("error: no previous result to continue from");
                            continue;
                        }
                        Some(v) => format!("{}{}", v.display_string(), trimmed),
                    }
                } else {
                    trimmed.to_string()
                };

                match eval(&expr_to_eval) {
                    Ok(v) => {
                        println!("= {}", v.display_string());
                        if let Some(path) = &log_file {
                            append_to_file(path, &expr_to_eval, &v.full_string());
                        }
                        last = Some(v);
                    }
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(e) => { eprintln!("readline error: {e}"); break; }
        }
    }
}

fn parse_args(args: &[String]) -> (Option<String>, Option<String>) {
    let mut expr: Option<String> = None;
    let mut file: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => { i += 1; if i < args.len() { file = Some(args[i].clone()); } }
            s if !s.starts_with('-') => { expr = Some(s.to_string()); }
            _ => {}
        }
        i += 1;
    }
    (expr, file)
}

fn append_to_file(path: &str, expr: &str, result: &str) {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => { let _ = writeln!(f, "{expr} = {result}"); }
        Err(e)    => eprintln!("cannot open {path}: {e}"),
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    fn evalf(s: &str) -> f64 { eval(s).unwrap().to_float() }
    #[test] fn basic()    { assert_eq!(evalf("2+2"), 4.0); assert_eq!(evalf("10-3*2"), 4.0); }
    #[test] fn parens()   { assert_eq!(evalf("(1+2)*3"), 9.0); }
    #[test] fn power()    { assert_eq!(evalf("2^10"), 1024.0); assert_eq!(evalf("2^3^2"), 512.0); }
    #[test] fn fact()     { assert_eq!(evalf("5!"), 120.0); assert_eq!(evalf("(3+2)!"), 120.0); }
    #[test] fn big_fact() { assert!(eval("(10!)!").is_ok()); }
    #[test] fn unary()    { assert_eq!(evalf("-5+3"), -2.0); assert_eq!(evalf("--5"), 5.0); }
    #[test] fn divzero()  { assert!(eval("1/0").is_err()); }
    #[test] fn fact_err() { assert!(eval("(-1)!").is_err()); assert!(eval("1.5!").is_err()); }
    #[test] fn pow_too_large() {
        // exponent larger than u32::MAX must error, not hang
        assert!(eval("2^9999999999").is_err());
    }
    #[test] fn cached_factorial() {
        // values from cache must match prime_swing_factorial for boundary cases
        let cached = eval("1000!").unwrap().full_string();
        let direct = Value::Int(prime_swing_factorial(1000)).full_string();
        assert_eq!(cached, direct);
    }
    #[test] fn chain_continuation() {
        // simulate: user typed "56+2", got 58, then typed "+8"
        let first = eval("56+2").unwrap();
        let expanded = format!("{}{}", first.display_string(), "+8");
        assert_eq!(eval(&expanded).unwrap().to_float(), 66.0);
    }
    #[test]
    fn display_large_no_full_conversion() {
        // (10!)! has ~22M digits; display_string must return in under 2 seconds
        let v = eval("(10!)!").unwrap();
        let start = std::time::Instant::now();
        let s = v.display_string();
        assert!(
            start.elapsed().as_secs() < 2,
            "display_string took too long: {:?}", start.elapsed()
        );
        assert!(s.contains("digit"), "expected digit-count summary, got: {s}");
    }
    #[test]
    fn bigint_literal_exact() {
        // exceeds f64 precision — must round-trip without loss
        let s = "123456789012345678901234567890";
        let v = eval(s).unwrap();
        assert_eq!(v.full_string(), s);
    }

    #[test]
    fn bigint_arithmetic_exact() {
        // 10^30 + 1 must not collapse to 10^30
        let v = eval("1000000000000000000000000000000 + 1").unwrap();
        assert_eq!(v.full_string(), "1000000000000000000000000000001");
    }

    #[test]
    fn float_literal_still_works() {
        assert!((eval("3.14").unwrap().to_float() - 3.14).abs() < 1e-10);
        assert!((eval("1e10").unwrap().to_float() - 1e10).abs() < 1.0);
        assert!((eval("2.5e-1").unwrap().to_float() - 0.25).abs() < 1e-15);
    }

    #[test]
    fn swing_matches_legendre() {
        // Prime Swing must produce identical results to the Legendre method
        for n in [0u64, 1, 2, 5, 10, 20, 100, 500, 1001, 5000] {
            let swing    = prime_swing_factorial(n).to_string_radix(10);
            let legendre = prime_factorial_legendre(n).to_string_radix(10);
            assert_eq!(swing, legendre, "mismatch at n={n}");
        }
    }

    #[test]
    fn swing_digit_count() {
        // 10000! has exactly 35660 decimal digits — canonical reference value
        let s = prime_swing_factorial(10_000).to_string_radix(10);
        assert_eq!(s.len(), 35660, "10000! digit count wrong");
    }

    #[test]
    fn swing_small_table() {
        // spot-check the SMALL_ODD_SWING table entries that are easy to verify
        assert_eq!(SMALL_ODD_SWING[1],  1);
        assert_eq!(SMALL_ODD_SWING[5],  15);
        assert_eq!(SMALL_ODD_SWING[10], 63);
        assert_eq!(SMALL_ODD_SWING[20], 46189);
    }
}

