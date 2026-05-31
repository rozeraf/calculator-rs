# Calc

Arbitrary-precision CLI calculator written in Rust. It supports interactive REPL mode, single expression evaluation, and result logging/dumping.

## Features

- **Arbitrary-precision integers** using the GNU Multiple Precision Arithmetic Library (GMP) via the [`rug`](https://crates.io/crates/rug) crate.
- **Floating-point math** fallbacks for fractional and extremely large calculations.
- **Fast factorials** using a prime-counting split-recursive algorithm (based on Peter Luschny's algorithm).
- **Interactive REPL** with command history powered by `rustyline`.
- **Expression logging and dumping**: Save your session results to a file or dump huge results directly.

## Supported Operators

- `+` (Addition)
- `-` (Subtraction / Negation)
- `*` (Multiplication)
- `/` (Division - exact integer division if no remainder, otherwise float)
- `%` (Modulo)
- `^` (Power / Exponentiation)
- `!` (Factorial)
- `(...)` (Parentheses for grouping)

## Interactive Commands

Inside the REPL, you can use the following commands:

- `:save <file>` — Enable logging of all evaluated expressions and results to a specified file.
- `:dump <file>` — Save the full, exact value of the last evaluation to a file (especially useful for massive integers).
- `:full` — Print the complete value of the last result without truncation.
- `:digits` — Show the exact number of digits in the last result.
- `:quit` or `:q` — Exit the REPL.

## Installation

To compile and install the release binary to your local bin directory:

```bash
cargo build --release
install -Dm755 target/release/calc ~/.local/bin/calc
```
