# Arc Compiler

A learning-focused compiler project written in Rust that has evolved from a simple arithmetic evaluator into a comprehensive expression language with rich type system, professional error handling, and now supports **variables and assignments**.

## Current Features (Phase 2.2 - Enhanced Usability)

### Variables & State Management
- **Variable declarations**: `let x = 10` (mutable), `const y = 20` (immutable)
- **Assignment**: `x = x + 5` with mutability checking
- **Symbol table**: Tracks variables, types, and mutability
- **Type safety**: Type checking on assignment with automatic int→float coercion
- **Error detection**: Undefined variables, redeclaration, immutable assignment attempts

### Built-in Functions
- **print()**: Output values to console - `print(x)`, `print("Hello")`, `print(42)`
- Multiple arguments supported

### Code Organization
- **Comments**: Single-line `//` and multi-line `/* */` comments
- **File execution**: Run `.arc` files directly
- **REPL**: Interactive mode for testing expressions

### Arithmetic Operations
- **Basic**: `+`, `-`, `*`, `/`
- **Extended**: `%` (modulo), `**` (exponentiation)
- **Bitwise**: `&`, `|`, `^`, `<<`, `>>`
- **Unary**: `-x`, `+x`, `!x`

### Rich Type System
- **Integer**: Full precision integer arithmetic
- **Float**: IEEE 754 floating-point numbers
- **Boolean**: `true` and `false` with truthy/falsy conversion
- **String**: String literals with comparison support
- **Automatic type coercion**: Seamless int ↔ float conversions

### Comparison & Logical Operators
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical**: `&&` (AND), `||` (OR), `!` (NOT)
- **Short-circuit evaluation**: Efficient and safe evaluation

### Professional Error Handling
- Line and column tracking
- Colored terminal output (red errors, yellow warnings)
- Source code snippets at error location
- Helpful suggestions for fixing errors
- Error recovery (continues after recoverable errors)

## How it Works

The compiler processes code through a **four-stage pipeline**:

1.  **Lexical Analysis (Lexer)**: Scans input and produces tokens (keywords, identifiers, operators, literals)
2.  **Parsing**: Builds an Abstract Syntax Tree (AST) using precedence climbing for expressions
3.  **Symbol Table**: Manages variables, scopes, and type information
4.  **Evaluation**: Traverses the AST with type-aware evaluation, variable lookups, and error collection

## Example Usage

### Variables and Assignment
```rust
Input: let x = 10
Result: Integer(10) : Integer

Input: x + 5
Result: Integer(15) : Integer

Input: x = x * 2
Result: Integer(20) : Integer

Input: const PI = 3.14
Result: Float(3.14) : Float

Input: PI = 3.15  // Error!
Error: Cannot assign to immutable variable 'PI'
```

### Print Function and Comments
```rust
Input: // This is a comment
Input: let name = "Arc"
Result: String("Arc") : String

Input: print(name)
Output: Arc

Input: print(10 + 5)
Output: 15

/* Multi-line
   comment */
Input: print("Hello", "World")
Output: Hello World
```

### Successful Evaluation
```rust
Input: 5 > 3 && 10.0 == 10
Result: true
Type: Boolean
```

### Error Detection
```
Input: (5 + 3

error: Expected closing parenthesis ')'
  --> input:1:7
1 | (5 + 3
  |       ^
  help: Add ')' to close the expression
```

## Features

*   **Variables & State**: `let` and `const` declarations with mutability checking
*   **Symbol Table**: Variable storage with scope management
*   **Built-in Functions**: `print()` for output
*   **Comments**: Single-line `//` and multi-line `/* */`
*   **File Execution**: Run `.arc` source files
*   **Arithmetic Operations**: Addition, subtraction, multiplication, division, modulo, exponentiation
*   **Bitwise Operations**: AND, OR, XOR, left shift, right shift
*   **Multiple Data Types**: Integers, floats, booleans, strings
*   **Type Coercion**: Automatic conversion between compatible types
*   **Comparison Operators**: All standard comparison operations
*   **Logical Operators**: AND, OR, NOT with short-circuit evaluation
*   **Operator Precedence**: 11 levels of precedence for correct evaluation order
*   **Parentheses**: Grouping of expressions
*   **Error Handling**: Professional diagnostics with source context
## Getting Started

This section will guide you through running the expression evaluator on your local machine.

### Prerequisites

Make sure you have Rust and Cargo installed on your system. You can install them by following the official instructions at [rust-lang.org](https://www.rust-lang.org/tools/install).

### Building and Running

1.  **Clone the repository**:
    ```bash
    git clone https://github.com/Prajwal-kp-18/rust-compiler.git
    cd rust-compiler
    ```

2.  **Build the project**:
    ```bash
    cargo build --release
    ```

3.  **Run in REPL mode** (interactive):
    ```bash
    cargo run
    ```

4.  **Execute a file**:
    ```bash
    cargo run -- program.arc
    # or after building:
    ./target/release/rust-compiler program.arc
    ```

### Example8 test cases covering:

- Variable declarations (`let` and `const`)
- Variable assignment and reassignment
- Mutability enforcement
- Variable usage in expressions
- Type coercion with variables
- Integer, float, boolean, and string variables
- Comparison operations with variables
- Logical operations with variables
- Bitwise operations with variables
- Error cases (undefined variables, redeclaration, immutable assignment)erations
- Logical operations with short-circuit evaluation
- Type coercion
- Complex boolean expressions
- Error handling demonstration

Run `cargo run` to see all tests execute and the error handling system in action!

## Project Structure

```
src/
├── main.rs              # Test suite and demonstrations
├── lib.rs               # Library interface
└── ast/
    ├── mod.rs          # AST node definitions (21 operators, 11 precedence levels)
    ├── lexer.rs        # Tokenization with position tracking (33 token types)
    ├── parser.rs       # Precedence-climbing parser with error recovery
    ├── evaluator.rs    # Type-aware expression evaluator
    ├── types.rs        # Value system and type coercion
    └── error.rs        # Error reporting infrastructure (11 error types)
```

## Test Results

All 42 test cases pass successfully:
- Integer arithmetic
- Float arithmetic
- Boolean operations
- String comparisons
- Type coercion
- Short-circuit evaluation
- Complex expressions
- Error handling

## Future Roadmap

See [FUTURE_SCOPE.md](FUTURE_SCOPE.md) for the complete development roadmap.

**Phase 1: Foundation Building** - COMPLETE
- Extended arithmetic operations
- Rich type system
- Comparison & logical operators
- Professional error handling

**Phase 2: Language Features** (Coming Next)
- Variables and assignment
- Control flow (if/else, loops)
- Functions and scope
- More to come!

## Contributing

Contributions are welcome! If you have ideas for new features, improvements, or bug fixes, feel free to open an issue or submit a pull request.

### How to Contribute

1.  **Fork the repository** on GitHub.
2.  **Create a new branch** for your feature or bug fix:
    ```bash
    git checkout -b feature-name
    ```
3.  **Make your changes** and commit them with a clear message.
4.  **Push your branch** to your fork:
    ```bash
    git push origin feature-name
    ```
5.  **Open a pull request** to the `main` branch of the original repository.

### Commit Message Format

To maintain a clear and organized commit history, please follow this format for your commit messages:

```
<type>(<scope>): <subject>
```

*   **type**: `feat` (new feature), `fix` (bug fix), `docs` (documentation), `style` (code style changes), `refactor` (code refactoring), `test` (adding or improving tests), or `chore` (build-related changes).
*   **scope** (optional): The part of the codebase you're changing (e.g., `parser`, `lexer`, `evaluator`).
*   **subject**: A concise description of the change.

**Example:**

```
feat(parser): Add support for unary minus
```

### Pull Request Guidelines

When submitting a pull request, please include the following in your description:

*   **A brief description of the changes**: Explain what you've changed and why.
*   **Related issue**: If your PR addresses an open issue, link to it (e.g., `Closes #123`).
*   **Testing**: Describe the tests you've added or how you've tested your changes manually.

This will help reviewers understand your contribution and provide feedback more effectively.

### Suggestions

If you have suggestions for improving the project, you can open an issue with the `enhancement` label. Please provide a clear and detailed description of your suggestion.
