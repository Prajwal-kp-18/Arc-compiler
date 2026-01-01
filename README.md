# Arc-compiler

A simple expression evaluator written in Rust. This project serves as a learning exercise to understand the fundamentals of building a compiler. It can parse and evaluate basic arithmetic expressions.

## How it Works

The evaluator processes arithmetic expressions in a pipeline inspired by modern compilers:

1.  **Lexical Analysis**: The `Lexer` scans the input string and converts it into a sequence of tokens. For example, the input `"7 - (30 + 7)"` is transformed into tokens representing numbers, operators, and parentheses.

2.  **Parsing**: The `Parser` takes the stream of tokens and constructs an Abstract Syntax Tree (AST). The AST is a tree representation of the code's structure that respects operator precedence and grouping.

3.  **Evaluation**: The `ASTEvaluator` traverses the AST to compute the final result of the expression. It recursively evaluates the nodes of the tree to produce a single numerical value.

The project also includes a `ASTPrintor` to visualize the structure of the AST.

## Features

*   **Arithmetic Operations**: Supports addition (`+`), subtraction (`-`), multiplication (`*`), and division (`/`).
*   **Integer Literals**: Can parse and evaluate integer values.
*   **Operator Precedence**: Correctly handles the order of operations (e.g., multiplication before addition).
*   **Parentheses**: Supports grouping of expressions with `(` and `)`.

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

The `main` function in `src/main.rs` contains a hardcoded example expression:

```rust
let input: &str = "7- (30 + 7) * 8/2";
```

When you run the project, it will perform the following steps:

1.  **Tokenization**: It will print the list of tokens generated from the input string.
2.  **AST Visualization**: It will display a tree structure representing the parsed expression.
3.  **Evaluation**: It will compute and print the final result.

For the example input, the output will look like this:

```
Tokens: [Token { kind: Number(7), ... }, Token { kind: Minus, ... }, ...]
Statement
  Expression
    Binary Expression
      Operator: Minus
      Expression
        Number: 7
      Expression
        Binary Expression
          Operator: Divide
          Expression
            Binary Expression
              Operator: Multiply
              Expression
                Parenthesized Expression
                  Expression
                    Binary Expression
                      Operator: Plus
                      Expression
                        Number: 30
                      Expression
                        Number: 7
              Expression
                Number: 8
          Expression
            Number: 2
Last evaluated value: Some(-141)
```

You can modify the `input` variable in `src/main.rs` to evaluate different arithmetic expressions.

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
