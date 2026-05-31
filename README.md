# calc

A blazingly fast arbitrary-precision calculator for integer arithmetic.

## Versions

| Version | Crate | Bignum Engine | Supported Platforms | Performance | Requirements |
|---------|-------|---------------|---------------------|-------------|--------------|
| **flash** | `calc-flash` | rug (GMP) | Linux | **Extremely Fast** | Requires system `gmp`, `mpfr`, `mpc` |
| **cross** | `calc-cross` | dashu | Linux, macOS, Windows | **Fast** | None (pure Rust, zero-dependency) |

> [!NOTE]
> **Difference between versions:**
> The primary difference is **portability vs. raw performance**, not one version being obsolete.
> - **flash** uses the C-based **GNU Multiple Precision Arithmetic Library (GMP)** via the `rug` crate. It is **technically much faster** — using state-of-the-art assembly-optimised algorithms for massive calculations like large factorials — but requires GNU dev tools to compile, especially on Windows.
> - **cross** is written in **pure Rust** (`dashu`). It has zero external dependencies and compiles out-of-the-box on Windows, macOS, and Linux with a plain `cargo build`. Thanks to `dashu`'s Karatsuba multiplication and advanced division algorithms it is significantly faster than `num-bigint` and bridges the gap with GMP on mid-to-large inputs.


## Installation


### Download Precompiled Binary

You can download the archive for your platform from the [Releases](../../releases) page:

**flash (statically linked with GMP/MPFR, Linux only — extremely fast):**
- `calc-flash-x86_64-unknown-linux-gnu.tar.gz` — Linux x64

**cross (pure Rust, zero dependency — highly portable):**
- `calc-cross-x86_64-unknown-linux-musl.tar.gz` — Linux x64 (statically linked)
- `calc-cross-aarch64-unknown-linux-musl.tar.gz` — Linux ARM64 (statically linked)
- `calc-cross-x86_64-pc-windows-msvc.zip` — Windows x64
- `calc-cross-x86_64-apple-darwin.tar.gz` — macOS Intel
- `calc-cross-aarch64-apple-darwin.tar.gz` — macOS Apple Silicon
- `calc-cross-universal-apple-darwin.tar.gz` — macOS Universal (Intel + Apple Silicon)

Extract the archive and place `calc-flash` or `calc-cross` (or `calc-cross.exe` on Windows) into a directory in your `PATH` (e.g. `~/.local/bin/` or `/usr/local/bin/`).


### Build from Source

Requirements: Rust 1.78+

**cross (recommended for most users):**
```bash
cargo build --release -p calc-cross
```

**flash (requires GMP system library):**
```bash
cargo build --release -p calc-flash
```

## Usage

The calculator operates in two modes:

1. **Interactive REPL mode** — launch without arguments.
2. **Single-expression mode** — pass the expression as a CLI argument, e.g. `./calc-cross "5! + 10"`. Supports an optional output flag `-o <file>` / `--output <file>`.

### Operators

| Operator | Description | Example |
| :--- | :--- | :--- |
| `+` | Addition | `2 + 2` |
| `-` | Subtraction / Negation | `-5 - 10` |
| `*` | Multiplication | `3 * 4` |
| `/` | Division (exact integer if remainder is zero, otherwise `f64`) | `10 / 3` |
| `%` | Modulo | `10 % 3` |
| `^` | Exponentiation (integer base & exponent ≤ 2³²−1) | `2 ^ 10` |
| `!` | Factorial (non-negative integers only) | `5!` |
| `(...)` | Grouping | `(2 + 2) * 2` |

### REPL Commands

| Command | Description |
| :--- | :--- |
| `:save <file>` | Start logging all expressions and results to a file. |
| `:dump <file>` | Write the full exact value of the last result to a file (useful for huge integers). |
| `:full` | Print the full exact value of the last result without truncation. |
| `:digits` | Show the exact digit count of the last result. |
| `:quit` / `:q` | Exit the REPL. |

### Chain-calculator mechanic (flash)

When a line starts with a binary operator the previous result is used as the left-hand side — just like a physical calculator:

```
> 56 + 2
= 58
> + 8
= 66
> * 2
= 132
> - 100
= 32
```

If there is no previous result an error is shown instead of hanging.

## What's inside calc-flash

| Feature | Detail |
| :--- | :--- |
| **Factorial algorithm** | Prime-sieve + Legendre exponent counting + balanced product tree — far fewer large multiplications than naïve approaches |
| **Parallel product tree** | `rayon::join` parallelises the tree above 128 factors, giving a real speedup on multi-core machines for inputs like `100000!` |
| **Factorial cache** | Factorials ≤ 1000 are computed once and stored in a `OnceLock<Vec<Integer>>`; subsequent calls are an O(1) clone |
| **Safe exponentiation** | Exponents larger than `u32::MAX` (4 294 967 295) return an error immediately instead of hanging |
| **Digit display** | Numbers with > 10 000 digits are summarised (`[N-digit number: first12...last8]`); use `:full` or `:dump` for the raw value |

## Limitations

- Computing `(10!)!` yields a number with over 7 million digits. In `calc-cross` (pure Rust) this may take 10–30 seconds; `calc-flash` (GMP) handles it in seconds.
- `dashu` does not support arbitrary-precision floats, so fractional results fall back to `f64` in `calc-cross`.
- Integer exponentiation in `calc-flash` is limited to exponents ≤ 2³²−1 (~4.3 billion); larger exponents fall back to `f64` pow.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
