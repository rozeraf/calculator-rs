# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Cargo workspace with two independent binaries implementing the same arbitrary-precision integer calculator (REPL + single-expression CLI):

- **`calc-flash`** — bignum engine is `rug` (GMP), plus `rayon`. Linux only; needs system `gmp`, `mpfr`, `mpc` dev libraries (`libgmp-dev libmpfr-dev libmpc-dev` on Debian/Ubuntu). This is where active performance work happens.
- **`calc-cross`** — bignum engine is `dashu`, pure Rust, builds on Linux/macOS/Windows with no system deps.

Each crate is a single file, `src/main.rs`, with no shared library crate. Nothing is shared between them: a fix to the lexer, parser, REPL or value semantics in one crate usually has to be ported by hand to the other (unless it depends on the engine).

## Commands

```bash
cargo build --release -p calc-cross        # or -p calc-flash
cargo test -p calc-cross                   # what CI runs (ubuntu/windows/macos)
cargo test -p calc-flash                   # not in CI; requires GMP locally
cargo test -p calc-flash swing_matches_legendre   # run a single test by name filter
cargo run --release -p calc-flash -- "5! + 10"    # single-expression mode
cargo run --release -p calc-flash -- "100000!" -o out.txt   # -o appends expr + full result to a file
```

Tests are inline `#[cfg(test)] mod tests` at the bottom of each `main.rs`. They go through `eval(&str)`. Some tests (e.g. `big_fact`, `(10!)!`) are slow in debug builds, especially in calc-cross. Use `--release` if they take too long.

## Architecture (same shape in both crates)

`main.rs` is laid out top to bottom:

1. **`Value`**: `enum { Float(f64), Int(BigInt) }`. Arithmetic (`v_add`, `v_div`, `v_pow`, …) stays exact when both operands convert via `to_integer()` and falls back to `f64` otherwise. `/` returns `Int` only if the remainder is zero, otherwise `Float`. There is no arbitrary-precision float type.
2. **`Lexer` → `Token`**, then a **recursive-descent `Parser`** that evaluates while it parses (no AST). Precedence, lowest to highest: `expr (+ -)` → `term (* / %, plus implicit * before √ ∛ ∜ π ()` → `unary (-)` → `power (^, right-assoc, RHS parsed as unary)` → `prefix (√ ∛ ∜)` → `postfix (! and superscript exponents)` → `primary (number, π/pi, sqrt(…)/cbrt(…), parens)`. Prefix `√` takes its operand with postfixes (`√4! = √(4!)`), while `sqrt(…)` is a primary and takes only its parenthesised argument (`sqrt(4)! = 2`). The grammar is spelled out in a comment above `struct Parser`. Unicode aliases (`× ÷ −` etc.) are mapped to the ASCII tokens in the lexer, so the parser only sees `Token::Star` and friends.
3. **Factorial**: sieve primes, then build a product tree.
4. **`CalcError`**: a single error enum with a `Display` impl.
5. **`main`**: `parse_args` (the first non-flag argument is the expression; `-o/--output <file>`), otherwise a `rustyline` REPL that keeps `last: Option<Value>` for the `:full`, `:dump`, `:digits` and `:save` commands. Code point escapes (`u221a`, `U+221A`, `√`) are decoded twice: `decode_escapes()` in `Lexer::new` for any input, and live in the REPL, where hex-digit keys are bound to `EscapeHandler`, which triggers `Cmd::Complete` so that `ReplHelper`'s `Completer` swaps the escape for the symbol. `Cmd::Replace` can't be used for this: rustyline resets its count to 1 for custom bindings and leaves the cursor before the inserted text.

### Display invariant

Numbers above `INLINE_LIMIT` (10 000 digits) print as a summary. In calc-flash, `display_string()`/`approx_digits()` **must never** do a decimal conversion. They estimate digits from `significant_bits()`, because base-10 conversion of multi-million-digit numbers takes seconds. Only `full_string()` (used by `:full`, `:dump`, `-o`) and the tests may call `to_string_radix(10)`. There is a test for this (`display_large_no_full_conversion`).

### Where the crates differ

| | calc-flash | calc-cross |
|---|---|---|
| Integer literals | lexed as `Token::Integer(String)` and parsed losslessly by `Integer::parse` | lexed through `f64`, so they are lossy above 2^53 |
| Factorial | n ≤ 1000 comes from the `OnceLock` `FACT_CACHE`. Larger n uses Luschny **Prime Swing** (`prime_swing_factorial` → `odd_factorial` → `odd_swing`, then `<< (n − popcount(n))`). `product_tree` uses `rayon::join` above 128 factors. The older Legendre implementation is kept as `#[cfg(test)] prime_factorial_legendre` and serves as the reference oracle in tests | sieve + Legendre exponents + sequential product tree |
| `^` limit | exponent > `u32::MAX` → `ExponentTooLarge` | exponent > `MAX_INT_EXP` → `ExponentTooLarge(u64)` |
| Chain mode | a REPL line that starts with a binary operator (`is_continuation`) reuses `last` as the LHS | not implemented |
| Large-number summary | `[~N-digit integer]` (approximate, no conversion) | `[N-digit number: first...last]` (exact, converts) |

## Build/release notes

- The release profile (fat LTO, `codegen-units = 1`, `panic = "abort"`, strip) lives in the root `Cargo.toml` and applies to both crates. Cargo ignores `[profile.*]` sections in member crates.
- Pushing a `v*` tag triggers `.github/workflows/release.yml`. It builds calc-flash for `x86_64-unknown-linux-gnu` and calc-cross for musl x64/aarch64 (aarch64 via `cross`), Windows MSVC, and macOS x64/arm64/universal (lipo), then publishes a GitHub release. The tag version is the project version (e.g. v3.1.0), while each crate has its own `Cargo.toml` version (calc-flash 0.3.x, calc-cross 3.x).
- Commit style is Conventional Commits with the crate as scope, e.g. `perf(calc-flash): …`, `fix(calc-flash): …`, `chore: prepare for vX.Y.Z release`.
