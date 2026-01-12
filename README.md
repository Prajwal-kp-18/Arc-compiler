# Arc Compiler

A learning-focused compiler project written in Rust that has evolved from a simple arithmetic evaluator into a comprehensive expression language with rich type system and professional error handling.

## ✨ Current Features (Phase 1 Complete!)

### 🔢 Arithmetic Operations
- **Basic**: `+`, `-`, `*`, `/`
- **Extended**: `%` (modulo), `**` (exponentiation)
- **Bitwise**: `&`, `|`, `^`, `<<`, `>>`
- **Unary**: `-x`, `+x`, `!x`

### 📊 Rich Type System
- **Integer**: Full precision integer arithmetic
- **Float**: IEEE 754 floating-point numbers
- **Boolean**: `true` and `false` with truthy/falsy conversion
- **String**: String literals with comparison support
- **Automatic type coercion**: Seamless int ↔ float conversions

### 🔍 Comparison & Logical Operators
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical**: `&&` (AND), `||` (OR), `!` (NOT)
- **Short-circuit evaluation**: Efficient and safe evaluation

### 🎨 Professional Error Handling
- Line and column tracking
- Colored terminal output (red errors, yellow warnings)
- Source code snippets at error location
- Helpful suggestions for fixing errors
- Error recovery (continues after recoverable errors)

## How it Works

The compiler processes expressions through a three-stage pipeline:

1.  **Lexical Analysis (Lexer)**: Scans input and produces tokens with position tracking
2.  **Parsing**: Builds an Abstract Syntax Tree (AST) using precedence climbing
3.  **Evaluation**: Traverses the AST with type-aware evaluation and error collection

## Example Usage

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

2.  **Build and run the project**:
    You can build and run the project with a single Cargo command:
    ```bash
    cargo run
    ```

### Example

The `main` function in `src/main.rs` contains a comprehensive test suite with 42 test cases covering:

- Integer and float arithmetic
- Comparison operations
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
- ✅ Integer arithmetic
- ✅ Float arithmetic
- ✅ Boolean operations
- ✅ String comparisons
- ✅ Type coercion
- ✅ Short-circuit evaluation
- ✅ Complex expressions
- ✅ Error handling

## Future Roadmap

See [FUTURE_SCOPE.md](FUTURE_SCOPE.md) for the complete development roadmap.

**Phase 1: Foundation Building** ✅ COMPLETE
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
