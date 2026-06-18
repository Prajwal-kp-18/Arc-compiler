# Arc Compiler

Arc is a small interpreted expression language implemented in Rust. It includes
a lexer, precedence-aware parser, scoped symbol table, evaluator, REPL, file
runner, and diagnostics with source spans and suggestions.

[![Docs](https://img.shields.io/badge/docs-online-blue)](https://prajwal-kp-18.github.io/Arc-compiler/arc_compiler/index.html)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

## Features

### Language

- **Variables and constants**: `let` creates mutable variables, `const` creates immutable bindings.
- **Assignment**: `x = x + 5` with mutability and assignment type checks.
- **Lexical scopes**: Bare blocks, `if` branches, and function calls create nested scopes.
- **Conditionals**: `if condition { ... } else { ... }` with truthy/falsy condition evaluation.
- **User-defined functions**: `fn name(arg1, arg2) { return value }`.
- **Built-in functions**: `print(...)`, `min(...)`, and `max(...)`.
- **Comments**: Single-line `//` and multi-line `/* ... */`.
- **REPL and file execution**: Run interactively or execute `.arc` files.

### Types

- `Integer`
- `Float`
- `Boolean`
- `String`

Arc performs runtime type checking. Arithmetic widens `Integer` to `Float` when
needed, logical contexts use truthiness, and assignments keep the declared
variable type except for assigning an integer into a float variable.

### Operators

| Group | Operators |
| --- | --- |
| Arithmetic | `+` `-` `*` `/` `%` `**` |
| Bitwise | `&` `|` `^` `<<` `>>` |
| Comparison | `==` `!=` `<` `>` `<=` `>=` |
| Logical | `&&` `||` `!` |
| Unary prefix | `+` `-` `!` `++` `--` |
| Postfix | `++` `--` |

Operator precedence, lowest to highest:

1. Logical OR: `||`
2. Logical AND: `&&`
3. Equality: `==`, `!=`
4. Comparison: `<`, `>`, `<=`, `>=`
5. Bitwise OR: `|`
6. Bitwise XOR: `^`
7. Bitwise AND: `&`
8. Bit shifts: `<<`, `>>`
9. Addition/subtraction: `+`, `-`
10. Multiplication/division/modulo: `*`, `/`, `%`
11. Exponentiation: `**` (right-associative)

Parentheses `()` can be used for explicit grouping.

## Getting Started

Install Rust and Cargo from [rust-lang.org](https://www.rust-lang.org/tools/install).

```bash
git clone https://github.com/Prajwal-kp-18/Arc-compiler
cd Arc-compiler

cargo build
cargo run
cargo run -- examples/demo.arc
```

Run tests and generate Rust API docs:

```bash
cargo test
cargo doc --no-deps
```

## Example

`examples/demo.arc` contains a broad demo of the current language. A shorter
sample:

```arc
print("=== Arc sample ===")

let x = 10
const scale = 2.5
let total = x * scale
print("total =", total)

fn square(n) {
    return n * n
}

if square(4) == 16 {
    print("square works")
} else {
    print("unexpected result")
}

let counter = 1
print(++counter)  // 2
print(counter++)  // 2
print(counter)    // 3

{
    let counter = 99
    print("inner counter =", counter)
}

print("outer counter =", counter)
print("min =", min(4, 2, 9, 1))
print("max =", max(4, 2, 9, 1))
```

## Error Handling

Arc collects parse and runtime diagnostics instead of stopping at the first
error. Current diagnostics cover:

- Unknown variables and functions, with close-name suggestions when available.
- Redeclaration in the same scope.
- Assignment to immutable `const` bindings.
- Assignment type mismatches.
- Invalid operator/type combinations.
- Division and integer modulo by zero.
- Invalid function arity and invalid `return` outside functions.
- Unclosed blocks and malformed declarations.

## Project Structure

```text
src/
|-- main.rs              # CLI entry point, REPL, and file execution
|-- lib.rs               # Library interface
`-- ast/
    |-- mod.rs           # AST node definitions and visitor trait
    |-- lexer.rs         # Tokenizer with source spans
    |-- parser.rs        # Recursive descent and precedence climbing parser
    |-- evaluator.rs     # Interpreter, functions, blocks, built-ins, diagnostics
    |-- types.rs         # Runtime values, coercion, comparison, truthiness
    |-- symbol_table.rs  # Lexically scoped variable storage
    `-- diagnostic.rs    # Diagnostic formatting and suggestions
```

## How It Works

1. **Lexer** converts source text into tokens.
2. **Parser** builds an AST from statements and expressions.
3. **Symbol table** resolves scoped variables and enforces mutability/type rules.
4. **Evaluator** executes statements, calls functions, and records diagnostics.

## Contributing

Use the standard fork, branch, and pull request workflow. Include a short
description of the change and how it was tested.

Suggested commit format:

```text
<type>(<scope>): <subject>
```

Example:

```text
feat(parser): add support for unary minus
```
