# Arc Compiler

A compiler for a custom expression language built in Rust, supporting variables, a rich type system, arithmetic and logical operations, and error diagnostics.

## Features

### Language

- **Variables**: `let x = 10` (mutable), `const PI = 3.14` (immutable)
- **Assignment**: `x = x + 5` with mutability and type checking
- **Built-in functions**: `print(x)`, `print("Hello", "World")`
- **Comments**: Single-line `//` and multi-line `/* */`

### Types

- `Integer`, `Float`, `Boolean`, `String`
- Automatic `int → float` coercion where applicable

### Operators

| Category   | Operators                          |
|------------|------------------------------------|
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `**`      |
| Bitwise    | `&`, `|`, `^`, `<<`, `>>`          |
| Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=`   |
| Logical    | `&&`, `||`, `!`                    |
| Unary      | `-x`, `+x`, `!x`                   |

**Operator Precedence** (11 levels, lowest to highest):
1. Logical OR (`||`)
2. Logical AND (`&&`)
3. Bitwise OR (`|`)
4. Bitwise XOR (`^`)
5. Bitwise AND (`&`)
6. Equality (`==`, `!=`)
7. Comparison (`<`, `>`, `<=`, `>=`)
8. Bit Shifts (`<<`, `>>`)
9. Addition/Subtraction (`+`, `-`)
10. Multiplication/Division/Modulo (`*`, `/`, `%`)
11. Exponentiation (`**`) Right-associative

Parentheses `()` can be used for explicit grouping and operator override.

### Error Handling

- Type checking at runtime with descriptive error messages
- Variable existence and mutability validation
- Division by zero detection
- Operation compatibility checking (e.g., cannot add boolean and string)
- Error accumulation execution continues after errors, all errors reported
- Integration with REPL and file execution modes

## Project Structure

```
src/
├── main.rs          # Entry point and REPL/file execution
├── lib.rs           # Library interface
└── ast/
    ├── mod.rs       # AST node definitions
    ├── lexer.rs     # Tokenizer with position tracking
    ├── parser.rs    # Precedence-climbing parser
    ├── evaluator.rs # Type-aware evaluator with error handling
    ├── types.rs     # Value types and coercion rules
    └── symbol_table.rs # Variable storage and scope management
```

## How It Works

Code is processed through a four-stage pipeline:

1. **Lexer:** Scans source text and produces tokens
2. **Parser:** Builds an Abstract Syntax Tree (AST)
3. **Symbol Table:** Resolves variables and enforces scope and mutability
4. **Evaluator:** Traverses the AST and produces values

## Getting Started

### Prerequisites

Install Rust and Cargo via [rust-lang.org](https://www.rust-lang.org/tools/install).

### Build and Run

```bash
# Clone the repository
git clone https://github.com/Prajwal-kp-18/Arc-compiler
cd Arc-compiler

# Build
cargo build --release

# Run in interactive (REPL) mode
cargo run

# Execute a source file
cargo run -- program.arc
```

## Usage Examples

```arc
let x = 10
x = x * 2          // x is now 20

const PI = 3.14
PI = 3.15           // Error: Cannot assign to immutable variable 'PI'

print("Hello", "World")
print(x + 5)

let flag = x > 10 && true
```

**Error Example:**
```
Error:
  Cannot assign to immutable variable 'PI'
```

## Contributing

Contributions are welcome. Please follow standard fork → branch → pull request workflow.

### Commit Format

```
<type>(<scope>): <subject>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Example: `feat(parser): add support for unary minus`

When opening a pull request, include a description of the change, any related issue, and how it was tested.