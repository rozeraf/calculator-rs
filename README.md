# calc

A blazingly fast arbitrary-precision calculator for integer arithmetic.

## Versions

| Version | Crate    | Bignum Engine | Supported Platforms   |
|---------|----------|---------------|-----------------------|
| v1      | calc-v1  | rug (GMP)     | Linux (requires GMP)  |
| v2      | calc-v2  | num-bigint    | Linux, macOS, Windows |

## Installation

### Download Precompiled Binary (v2)

You can download the archive for your platform from the Releases page:

- `calc-v2-x86_64-unknown-linux-musl.tar.gz` - Linux x64 (statically linked)
- `calc-v2-aarch64-unknown-linux-musl.tar.gz` - Linux ARM64 (statically linked)
- `calc-v2-x86_64-pc-windows-msvc.zip` - Windows x64
- `calc-v2-x86_64-apple-darwin.tar.gz` - macOS Intel
- `calc-v2-aarch64-apple-darwin.tar.gz` - macOS Apple Silicon
- `calc-v2-universal-apple-darwin.tar.gz` - macOS Universal (Intel + Apple Silicon)

Extract the archive and place the executable `calc` (or `calc.exe` on Windows) into a directory in your `PATH` (e.g., `~/.local/bin/` or `/usr/local/bin/`).

### Build from Source

Requirements: Rust 1.78+

**v2 (recommended):**
```bash
cargo build --release -p calc-v2
```

**v1 (requires GMP system library):**
```bash
cargo build --release -p calc-v1
```

## Usage

The calculator can operate in two modes:

1. **Interactive REPL Mode:** Launch the program without any arguments.
2. **Single Expression Mode:** Pass the mathematical expression as a command-line argument (e.g., `./calc "5! + 10"`). It also supports an optional output file argument `-o <file>` / `--output <file>`.

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

- Calculating `(10!)!` yields a number with over 7 million digits. This is an algorithmic limitation of `num-bigint` (which is slower than GMP), and calculation in `calc-v2` may take about 10 seconds.
- `num-bigint` does not support arbitrary-precision floats, so fractional calculations fall back to using standard `f64`.

## License

This project is licensed under the MIT License. For details, see the [LICENSE](file:///home/raf/projects/calculator-rs/LICENSE) file.
