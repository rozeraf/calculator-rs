# calc

A blazingly fast arbitrary-precision calculator for integer arithmetic.

## Versions

| Version | Crate    | Bignum Engine | Supported Platforms   | Performance | Requirements |
|---------|----------|---------------|-----------------------|-------------|--------------|
| **flash** | `calc-flash`| rug (GMP)     | Linux                 | **Extremely Fast** | Requires system `gmp`, `mpfr`, `mpc` |
| **cross** | `calc-cross`| dashu         | Linux, macOS, Windows | **Fast**    | None (pure Rust, zero-dependency) |

> [!NOTE]
> **Difference between Versions:**
> The primary difference is **portability vs. raw performance**, rather than one version being obsolete.
> - **flash** uses the C-based **GNU Multiple Precision Arithmetic Library (GMP)** via the `rug` crate. It is **extremely fast** (using state-of-the-art assembly-optimized algorithms for massive calculations like large factorials), but is hard to compile cross-platform (especially on Windows) without installing GNU dev tools.
> - **cross** is written in **pure Rust** (`dashu`). It has zero external dependencies and compiles out-of-the-box on Windows, macOS, and Linux with a simple `cargo build`. Thanks to `dashu`'s implementation of Karatsuba multiplication and advanced division algorithms, it is significantly faster than standard `num-bigint` and bridges the gap with GMP on mid-to-large inputs.


## Installation


### Download Precompiled Binary

You can download the archive for your platform from the Releases page:

**flash (Statically linked with MPFR/GMP via system libs, Linux only — Extremely Fast):**
- `calc-flash-x86_64-unknown-linux-gnu.tar.gz` - Linux x64

**cross (Pure Rust, Zero dependency — Highly portable):**
- `calc-cross-x86_64-unknown-linux-musl.tar.gz` - Linux x64 (statically linked)
- `calc-cross-aarch64-unknown-linux-musl.tar.gz` - Linux ARM64 (statically linked)
- `calc-cross-x86_64-pc-windows-msvc.zip` - Windows x64
- `calc-cross-x86_64-apple-darwin.tar.gz` - macOS Intel
- `calc-cross-aarch64-apple-darwin.tar.gz` - macOS Apple Silicon
- `calc-cross-universal-apple-darwin.tar.gz` - macOS Universal (Intel + Apple Silicon)

Extract the archive and place the executable `calc-flash` or `calc-cross` (or `calc-cross.exe` on Windows) into a directory in your `PATH` (e.g., `~/.local/bin/` or `/usr/local/bin/`).


### Build from Source

Requirements: Rust 1.78+

**cross (recommended):**
```bash
cargo build --release -p calc-cross
```

**flash (requires GMP system library):**
```bash
cargo build --release -p calc-flash
```

## Usage

The calculator can operate in two modes:

1. **Interactive REPL Mode:** Launch the program without any arguments.
2. **Single Expression Mode:** Pass the mathematical expression as a command-line argument (e.g., `./calc-cross "5! + 10"`). It also supports an optional output file argument `-o <file>` / `--output <file>`.

### Operators

| Operator | Description | Example |
| :--- | :--- | :--- |
| `+` | Addition | `2 + 2` |
| `-` | Subtraction / Negation | `-5 - 10` |
| `*` | Multiplication | `3 * 4` |
| `/` | Division (exact integer division if remainder is zero, otherwise falls back to float) | `10 / 3` |
| `%` | Modulo (remainder of division) | `10 % 3` |
| `^` | Exponentiation | `2 ^ 10` (limited to exponent `4_000_000` for integer math) |
| `!` | Factorial | `5!` (only supports non-negative integers) |
| `(...)` | Parentheses for grouping | `(2 + 2) * 2` |

### REPL Commands

The following commands are available inside interactive mode:

| Command | Description |
| :--- | :--- |
| `:save <file>` | Start logging all inputs and evaluation results into the specified file. |
| `:dump <file>` | Save the full exact value of the last evaluation result directly to a file (useful for massive integers). |
| `:full` | Print the full exact value of the last result without truncation. |
| `:digits` | Show the exact number of digits in the last result. |
| `:quit` or `:q` | Exit the REPL. |

## Limitations

- Calculating `(10!)!` yields a number with over 7 million digits. This is an algorithmic limitation of pure Rust arithmetic engines compared to GMP; while `dashu` is extremely optimized and highly competitive, calculating `(10!)!` in `calc-cross` may still take around 10–30 seconds.
- `dashu` does not support arbitrary-precision floats, so fractional calculations fall back to using standard `f64`.

## License

This project is licensed under the MIT License. For details, see the [LICENSE](file:///home/raf/projects/calculator-rs/LICENSE) file.

