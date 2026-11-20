use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use rustyline::{DefaultEditor, error::ReadlineError};
use dashu::integer::{IBig, UBig};
use dashu::base::{BitTest, DivRem, UnsignedAbs};

// --- Value ---

#[derive(Clone, Debug)]
enum Value {
    Float(f64),
    Int(IBig),
}

// Digits above which we don't inline-print but show summary instead.
const INLINE_LIMIT: usize = 10_000;

impl Value {
    fn to_float(&self) -> f64 {
        match self {
            Value::Float(f) => *f,
            Value::Int(n)   => n.to_f64().value(),
        }
    }


    fn to_integer(&self) -> Option<IBig> {
        match self {
            Value::Int(n) => Some(n.clone()),
            Value::Float(f) if f.fract() == 0.0 && f.is_finite() && f.abs() < 9e18 => {
                Some(IBig::from(*f as i64))
            }
            _ => None,
        }
    }

    fn is_zero(&self) -> bool {
        match self {
            Value::Float(f) => *f == 0.0,
            Value::Int(n)   => n == &IBig::ZERO,
        }
    }

    // Approximate digit count without full decimal conversion.
    // Uses bit length * log10(2). May be off by 1, good enough for display decision.
    fn approx_digits(&self) -> usize {
        match self {
            Value::Float(_) => 20,
            Value::Int(n) => {
                if n == &IBig::ZERO { return 1; }
                let bits = n.bit_len() as f64;
                (bits / std::f64::consts::LOG2_10).ceil() as usize
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
                    // GMP decimal conversion of millions of digits is still slow.
                    // Show summary instead. Use :full or :save to get the full number.
                    let s = n.to_string();
                    let actual = s.len();
                    format!(
                        "[{actual}-digit number: {}...{}]  (use :full or :save <file>)",
                        &s[..12],
                        &s[actual - 8..]
                    )
                }
            }
        }
    }

    fn full_string(&self) -> String {
        match self {
            Value::Float(f) => format!("{f}"),
            Value::Int(n)   => n.to_string(),
        }
    }
}

// --- Token ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Plus, Minus, Star, Slash, Percent, Caret, Bang,
    LParen, RParen, Eof,
    Root(u32),   // √ ∛ ∜ — prefix n-th root
    Pi,          // π
    Sup(String), // superscript exponent, e.g. "²" → "2", "⁻¹" → "-1"
}

// Maps superscript digits (⁰¹²³⁴⁵⁶⁷⁸⁹) to ASCII digits.
fn superscript_digit(c: char) -> Option<char> {
    match c {
        '⁰' => Some('0'), '¹' => Some('1'), '²' => Some('2'), '³' => Some('3'),
        '⁴' => Some('4'), '⁵' => Some('5'), '⁶' => Some('6'), '⁷' => Some('7'),
        '⁸' => Some('8'), '⁹' => Some('9'),
        _ => None,
    }
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
        // is_whitespace also covers NBSP and other Unicode spaces from copy-paste
        while self.peek().is_some_and(char::is_whitespace) { self.advance(); }
    }
    // Reads a superscript exponent: optional '⁻' followed by superscript digits.
    fn read_superscript(&mut self) -> Result<Token, CalcError> {
        let mut s = String::new();
        if self.peek() == Some('⁻') {
            self.advance();
            s.push('-');
        }
        while let Some(d) = self.peek().and_then(superscript_digit) {
            self.advance();
            s.push(d);
        }
        if s.is_empty() || s == "-" {
            return Err(CalcError::UnexpectedChar('⁻'));
        }
        Ok(Token::Sup(s))
    }
    fn read_number(&mut self) -> Result<f64, CalcError> {
        let start = self.pos;
        while matches!(self.peek(), Some('0'..='9') | Some('.') | Some('e') | Some('E')) {
            let c = self.advance().unwrap();
            if (c == 'e' || c == 'E') && matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
        }
        let s: String = self.input[start..self.pos].iter().collect();
        s.parse::<f64>().map_err(|_| CalcError::InvalidNumber(s))
    }
    fn tokenize(&mut self) -> Result<Vec<Token>, CalcError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None    => { tokens.push(Token::Eof); break; }
                Some(c) => match c {
                    '0'..='9' | '.' => tokens.push(Token::Number(self.read_number()?)),
                    '+' => { self.advance(); tokens.push(Token::Plus); }
                    // − U+2212 minus sign
                    '-' | '−' => { self.advance(); tokens.push(Token::Minus); }
                    // × multiplication, · middle dot, ⋅ dot operator, ∙ bullet operator, ∗ asterisk operator
                    '*' | '×' | '·' | '⋅' | '∙' | '∗' => { self.advance(); tokens.push(Token::Star); }
                    // ÷ division sign, ∕ division slash
                    '/' | '÷' | '∕' => { self.advance(); tokens.push(Token::Slash); }
                    '√' => { self.advance(); tokens.push(Token::Root(2)); }
                    '∛' => { self.advance(); tokens.push(Token::Root(3)); }
                    '∜' => { self.advance(); tokens.push(Token::Root(4)); }
                    'π' => { self.advance(); tokens.push(Token::Pi); }
                    c if c == '⁻' || superscript_digit(c).is_some() => tokens.push(self.read_superscript()?),
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
// expr     = term   ((+ | -)  term)*
// term     = unary  ((* | / | %) unary | <implicit *> unary)*
//            implicit multiplication only before √ ∛ ∜ π ( — e.g. 2√3, 3π, 2(1+1)
// unary    = - unary | power
// power    = prefix (^ unary)?   right-associative
// prefix   = (√ | ∛ | ∜) radicand | postfix
// radicand = - radicand | prefix    so that ∛-8 works
// postfix  = primary (! | superscript)*
// primary  = number | π | ( expr )

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
                Token::Root(_) | Token::Pi | Token::LParen => { l = v_mul(l, self.parse_unary()?)?; }
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
        let base = self.parse_prefix()?;
        if self.peek() == &Token::Caret {
            self.advance();
            return v_pow(base, self.parse_unary()?);
        }
        Ok(base)
    }
    fn parse_prefix(&mut self) -> Result<Value, CalcError> {
        if let Token::Root(n) = *self.peek() {
            self.advance();
            return v_root(self.parse_radicand()?, n);
        }
        self.parse_postfix()
    }
    fn parse_radicand(&mut self) -> Result<Value, CalcError> {
        if self.peek() == &Token::Minus {
            self.advance();
            return Ok(v_neg(self.parse_radicand()?));
        }
        self.parse_prefix()
    }
    fn parse_postfix(&mut self) -> Result<Value, CalcError> {
        let mut v = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Token::Bang => { self.advance(); v = factorial(v)?; }
                Token::Sup(s) => {
                    self.advance();
                    let e: IBig = s.parse().map_err(|_| CalcError::InvalidNumber(s.clone()))?;
                    v = v_pow(v, Value::Int(e))?;
                }
                _ => break,
            }
        }
        Ok(v)
    }
    fn parse_primary(&mut self) -> Result<Value, CalcError> {
        match self.peek().clone() {
            Token::Number(n) => { self.advance(); Ok(Value::Float(n)) }
            Token::Pi => { self.advance(); Ok(Value::Float(std::f64::consts::PI)) }
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

fn int_or_float2(a: Value, b: Value, fi: impl Fn(IBig, IBig) -> Value, ff: impl Fn(f64, f64) -> f64) -> Value {
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
            let (q, r) = IBig::div_rem(x, y);
            if r == IBig::ZERO { Value::Int(q) }
            else { Value::Float(a.to_float() / b.to_float()) }
        }
        _ => Value::Float(a.to_float() / b.to_float()),
    })
}
fn v_rem(a: Value, b: Value) -> Result<Value, CalcError> {
    if b.is_zero() { return Err(CalcError::DivisionByZero); }
    Ok(int_or_float2(a, b, |x, y| Value::Int(x % y), |x, y| x % y))
}

// Максимальный показатель для целочисленного возведения в степень.
// 2^MAX_INT_EXP ~ 10^(MAX_INT_EXP * 0.301) цифр.
// 4_000_000 -> ~1.2M цифр, разумный верхний предел.
const MAX_INT_EXP: u64 = 4_000_000;

fn v_pow(base: Value, exp: Value) -> Result<Value, CalcError> {
    if let (Some(b), Some(e)) = (base.to_integer(), exp.to_integer()) {
        if e >= IBig::ZERO {
            let e_u64 = u64::try_from(&e).unwrap_or(u64::MAX);
            if e_u64 > MAX_INT_EXP {
                return Err(CalcError::ExponentTooLarge(e_u64));
            }
            let e_u32 = e_u64 as u32;
            return Ok(Value::Int(b.pow(e_u32 as usize)));
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

// n-th root. Exact Int for perfect powers, otherwise f64.
// Even roots of negative numbers are a domain error; odd roots keep the sign.
fn v_root(v: Value, n: u32) -> Result<Value, CalcError> {
    if let Some(x) = v.to_integer() {
        let neg = x < IBig::ZERO;
        if neg && n % 2 == 0 { return Err(CalcError::RootDomain(n)); }
        let a: UBig = x.unsigned_abs();
        let r = a.nth_root(n as usize);
        if r.pow(n as usize) == a {
            let r = IBig::from(r);
            return Ok(Value::Int(if neg { -r } else { r }));
        }
        let f = int_root_f64(&a, n)?;
        return Ok(Value::Float(if neg { -f } else { f }));
    }
    let f = v.to_float();
    if f < 0.0 && n % 2 == 0 { return Err(CalcError::RootDomain(n)); }
    Ok(Value::Float(float_root(f, n)))
}

fn float_root(f: f64, n: u32) -> f64 {
    match n {
        2 => f.sqrt(),
        3 => f.cbrt(),
        _ => f.signum() * f.abs().powf(1.0 / f64::from(n)),
    }
}

// n-th root of a non-negative integer as f64. Huge inputs are shifted right by a
// multiple of n bits first so that to_f64() does not overflow to infinity.
fn int_root_f64(a: &UBig, n: u32) -> Result<f64, CalcError> {
    const KEEP_BITS: usize = 1000;
    let n_us = n as usize;
    let bits = a.bit_len();
    let shift = if bits > KEEP_BITS { (bits - KEEP_BITS) / n_us * n_us } else { 0 };
    let m = (a >> shift).to_f64().value();
    let scale = 2f64.powi(i32::try_from(shift / n_us).unwrap_or(i32::MAX));
    let r = float_root(m, n) * scale;
    if r.is_finite() { Ok(r) } else { Err(CalcError::FloatOverflow) }
}

// --- Factorial (Peter Luschny's algorithm) ---
//
// n! = product of (swing(k) for odd k) * 2^(n - popcount(n))
// where swing(n) = n! / (floor(n/2)!)^2
//
// In practice: split-recursive prime-counting approach.
// For each prime p <= n, compute its exact exponent in n! via Legendre's formula,
// then reconstruct using a balanced product tree. Far fewer large multiplications
// than sequential or simple divide-and-conquer.

fn factorial(v: Value) -> Result<Value, CalcError> {
    let f = v.to_float();
    if f < 0.0 || f.fract() != 0.0 { return Err(CalcError::FactorialDomain(f)); }
    let n = f as u64;
    if n <= 1 { return Ok(Value::Int(IBig::ONE)); }
    Ok(Value::Int(prime_factorial(n)))
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
fn legendre(n: u64, p: u64) -> u32 {
    let mut exp = 0u32;
    let mut pk = p;
    while pk <= n {
        exp += (n / pk) as u32;
        pk = match pk.checked_mul(p) { Some(v) => v, None => break };
    }
    exp
}

// Balanced product of a slice of integers.
fn product_tree(factors: &[UBig]) -> UBig {
    match factors.len() {
        0 => UBig::ONE,
        1 => factors[0].clone(),
        _ => {
            let mid = factors.len() / 2;
            product_tree(&factors[..mid]) * product_tree(&factors[mid..])
        }
    }
}

fn prime_factorial(n: u64) -> IBig {
    let primes = sieve(n);
    // For each prime, compute p^legendre(n,p) and collect into product tree.
    let factors: Vec<UBig> = primes
        .iter()
        .map(|&p| {
            let e = legendre(n, p);
            UBig::from(p).pow(e as usize)
        })
        .collect();
    IBig::from(product_tree(&factors))
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
    ExponentTooLarge(u64),
    RootDomain(u32),
    FloatOverflow,
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
            Self::ExponentTooLarge(n) => write!(f, "exponent {n} exceeds limit ({MAX_INT_EXP})"),
            Self::RootDomain(n)       => write!(f, "{} of a negative number is not real", root_name(*n)),
            Self::FloatOverflow       => write!(f, "result is too large for floating point"),
        }
    }
}

fn root_name(n: u32) -> String {
    match n {
        2 => "square root".into(),
        3 => "cube root".into(),
        _ => format!("{n}th root"),
    }
}

// --- Eval ---

fn eval(expr: &str) -> Result<Value, CalcError> {
    let tokens = Lexer::new(expr).tokenize()?;
    Parser::new(tokens).parse()
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
                                        println!("{} digits", n.to_string().len());
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

                match eval(trimmed) {
                    Ok(v) => {
                        println!("= {}", v.display_string());
                        if let Some(path) = &log_file {
                            append_to_file(path, trimmed, &v.full_string());
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
    fn eval_int(s: &str) -> String {
        match eval(s).unwrap() {
            Value::Int(n) => n.to_string(),
            Value::Float(f) => panic!("{s}: expected exact integer, got float {f}"),
        }
    }
    #[test] fn unicode_operators() {
        assert_eq!(evalf("6×7"), 42.0);
        assert_eq!(evalf("6·7"), 42.0);
        assert_eq!(evalf("6⋅7"), 42.0);
        assert_eq!(evalf("84÷2"), 42.0);
        assert_eq!(evalf("50−8"), 42.0);
        assert_eq!(evalf("−5+3"), -2.0);
        assert_eq!(evalf("2\u{00A0}+\u{00A0}2"), 4.0); // NBSP
    }
    #[test] fn roots_exact() {
        assert_eq!(eval_int("√16"), "4");
        assert_eq!(eval_int("∛27"), "3");
        assert_eq!(eval_int("∜81"), "3");
        assert_eq!(eval_int("∛-8"), "-2");
        assert_eq!(eval_int("√(9+16)"), "5");
        assert_eq!(eval_int("√√16"), "2");
        // literals go through f64 in calc-cross, so build the big perfect square with ^
        assert_eq!(eval_int("√(10^200)"), "1".to_string() + &"0".repeat(100));
    }
    #[test] fn roots_float() {
        assert!((evalf("√2") - std::f64::consts::SQRT_2).abs() < 1e-15);
        assert!((evalf("√2.25") - 1.5).abs() < 1e-15);
        assert!((evalf("∛-2") + 2f64.cbrt()).abs() < 1e-15);
        // huge non-perfect square must not overflow through to_f64()
        let v = evalf("√(10^400+1)");
        assert!((v / 1e200 - 1.0).abs() < 1e-12);
    }
    #[test] fn roots_domain() {
        assert!(eval("√-4").is_err());
        assert!(eval("∜-16").is_err());
        assert!(eval("√-2.5").is_err());
        assert!(eval("√").is_err());
    }
    #[test] fn root_precedence() {
        assert_eq!(evalf("-√4"), -2.0);
        assert_eq!(evalf("√4^3"), 8.0);      // (√4)^3
        assert_eq!(evalf("√4!"), 24f64.sqrt()); // √(4!)
        assert_eq!(evalf("√9+1"), 4.0);
    }
    #[test] fn implicit_multiplication() {
        assert_eq!(evalf("2√9"), 6.0);
        assert_eq!(evalf("2(3+4)"), 14.0);
        assert_eq!(evalf("(1+1)(2+3)"), 10.0);
        assert_eq!(evalf("6/2√9"), 9.0);  // same precedence as *: (6/2)·3
        assert_eq!(evalf("-2√4"), -4.0);
        assert!((evalf("2π") - 2.0 * std::f64::consts::PI).abs() < 1e-15);
        assert!(eval("2 3").is_err()); // no implicit multiplication between plain numbers
    }
    #[test] fn pi() {
        assert_eq!(evalf("π"), std::f64::consts::PI);
        assert!((evalf("π×2²") - 4.0 * std::f64::consts::PI).abs() < 1e-12);
    }
    #[test] fn superscripts() {
        assert_eq!(eval_int("3²"), "9");
        assert_eq!(eval_int("2¹⁰"), "1024");
        assert_eq!(eval_int("10⁰"), "1");
        assert_eq!(evalf("2⁻¹"), 0.5);
        assert_eq!(eval_int("-3²"), "-9");
        assert_eq!(eval_int("(-3)²"), "9");
        assert_eq!(eval_int("2^3²"), "512");  // right-assoc: 2^(3²)
        assert_eq!(eval_int("3!²"), "36");
        assert_eq!(eval_int("√(3²+4²)"), "5");
        assert!(eval("2⁻").is_err());
    }
    #[test] fn basic()    { assert_eq!(evalf("2+2"), 4.0); assert_eq!(evalf("10-3*2"), 4.0); }
    #[test] fn parens()   { assert_eq!(evalf("(1+2)*3"), 9.0); }
    #[test] fn power()    { assert_eq!(evalf("2^10"), 1024.0); assert_eq!(evalf("2^3^2"), 512.0); }
    #[test] fn fact()     { assert_eq!(evalf("5!"), 120.0); assert_eq!(evalf("(3+2)!"), 120.0); }
    #[test] fn big_fact() { assert!(eval("(10!)!").is_ok()); }
    #[test] fn unary()    { assert_eq!(evalf("-5+3"), -2.0); assert_eq!(evalf("--5"), 5.0); }
    #[test] fn divzero()  { assert!(eval("1/0").is_err()); }
    #[test] fn fact_err() { assert!(eval("(-1)!").is_err()); assert!(eval("1.5!").is_err()); }
    #[test] fn pow_guard() {
        assert!(eval(&format!("2^{}", MAX_INT_EXP + 1)).is_err());
        assert!(eval(&format!("2^{}", MAX_INT_EXP)).is_ok());
    }
}
