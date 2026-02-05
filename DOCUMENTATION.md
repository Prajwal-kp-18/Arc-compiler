# Arc Compiler - Complete Documentation

## Table of Contents
1. [Introduction](#introduction)
2. [Architecture](#architecture)
3. [Language Features](#language-features)
4. [Syntax Reference](#syntax-reference)
5. [Built-in Functions](#built-in-functions)
6. [Type System](#type-system)
7. [Examples](#examples)
8. [API Reference](#api-reference)

---

## Introduction

Arc is a lightweight, interpreted expression language designed for learning compiler construction. It features a clean syntax, strong type system with automatic coercion, and comprehensive error handling.

### Key Characteristics
- **Interpreted**: Executes code directly without compilation to machine code
- **Dynamically typed**: Types are checked at runtime with automatic coercion
- **Expression-oriented**: Most constructs evaluate to values
- **Interactive REPL**: Test code snippets interactively
- **File execution**: Run complete programs from `.arc` files

---

## Architecture

Arc uses a classic four-stage compilation pipeline:

### 1. Lexical Analysis (Lexer)
**Location**: `src/ast/lexer.rs`

Converts source code into a stream of tokens.

**Token Types** (36 total):
- **Literals**: `Number`, `Float`, `Boolean`, `String`
- **Operators**: `+`, `-`, `*`, `/`, `%`, `**`, `&`, `|`, `^`, `<<`, `>>`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`
- **Keywords**: `let`, `const`
- **Delimiters**: `(`, `)`, `,`, `{`, `}`
- **Special**: `=`, `;`, `EOF`, `Whitespace`

**Features**:
- Position tracking for error reporting
- String escape sequences (`\n`, `\t`, `\"`, `\\`)
- Single-line (`//`) and multi-line (`/* */`) comments
- Floating-point number detection

### 2. Parsing (Parser)
**Location**: `src/ast/parser.rs`

Builds an Abstract Syntax Tree (AST) using precedence climbing.

**Statement Types**:
- Expression statements
- Variable declarations (`let`, `const`)
- Assignment statements

**Expression Types**:
- Number literals (integer and float)
- Boolean literals (`true`, `false`)
- String literals
- Identifiers (variable references)
- Binary expressions (with operator precedence)
- Unary expressions (`-x`, `+x`, `!x`)
- Parenthesized expressions
- Function calls

**Operator Precedence** (11 levels, lowest to highest):
1. Logical OR (`||`)
2. Logical AND (`&&`)
3. Bitwise OR (`|`)
4. Bitwise XOR (`^`)
5. Bitwise AND (`&`)
6. Equality (`==`, `!=`)
7. Comparison (`<`, `>`, `<=`, `>=`)
8. Bit shifts (`<<`, `>>`)
9. Addition/Subtraction (`+`, `-`)
10. Multiplication/Division/Modulo (`*`, `/`, `%`)
11. Exponentiation (`**`)

### 3. Symbol Table
**Location**: `src/ast/symbol_table.rs`

Manages variable storage and scope.

**Features**:
- Variable definition with mutability tracking
- Type-safe assignment
- Undefined variable detection
- Redeclaration prevention
- Scope management (ready for future nested scopes)

### 4. Evaluation (Evaluator)
**Location**: `src/ast/evaluator.rs`

Executes the AST using the Visitor pattern.

**Features**:
- Type-aware evaluation
- Automatic type coercion
- Short-circuit evaluation for logical operators
- Error collection without stopping execution
- Built-in function support

---

## Language Features

### Variables

#### Declaration
```arc
let x = 10        // Mutable variable
const PI = 3.14   // Immutable constant
```

#### Assignment
```arc
x = 20           // OK: x is mutable
PI = 3.15        // ERROR: PI is immutable
```

#### Scope
Currently, all variables are in global scope. Block scoping is planned for future releases.

### Comments

```arc
// Single-line comment

/* Multi-line
   comment */

let x = 10  // Inline comment
```

### Data Types

#### Integer
```arc
let age = 25
let negative = -100
```

#### Float
```arc
let pi = 3.14159
let scientific = 1.5
```

#### Boolean
```arc
let isValid = true
let hasError = false
```

#### String
```arc
let name = "Arc"
let message = "Hello, World!"
let escaped = "Line1\nLine2"  // Supports \n, \t, \\, \"
```

### Operators

#### Arithmetic
```arc
let sum = 5 + 3         // 8
let diff = 10 - 4       // 6
let product = 4 * 5     // 20
let quotient = 15 / 3   // 5
let remainder = 17 % 5  // 2
let power = 2 ** 8      // 256
```

#### Comparison
```arc
5 == 5    // true
5 != 3    // true
5 < 10    // true
5 > 3     // true
5 <= 5    // true
10 >= 5   // true
```

#### Logical
```arc
true && false   // false
true || false   // true
!true           // false

// Short-circuit evaluation
false && print("Not executed")  // false, print not called
true || print("Not executed")   // true, print not called
```

#### Bitwise
```arc
12 & 10   // 8  (1100 & 1010 = 1000)
12 | 10   // 14 (1100 | 1010 = 1110)
12 ^ 10   // 6  (1100 ^ 1010 = 0110)
8 << 2    // 32 (shift left)
32 >> 2   // 8  (shift right)
```

#### Unary
```arc
-10       // Negation
+10       // Positive (no-op)
!true     // Logical NOT
```

### Type Coercion

Automatic conversion between compatible types:

```arc
let x = 10        // Integer
let y = 3.14      // Float
let z = x + y     // Float(13.14) - integer promoted to float

5 + 2.5           // Float(7.5)
10 == 10.0        // true - comparison coerces types
```

**Coercion Rules**:
- Integer → Float: Always allowed in arithmetic
- Any → Boolean: In logical contexts (0/empty is false, others true)
- Boolean → Integer: true = 1, false = 0
- String comparison: Lexicographic ordering

---

## Syntax Reference

### Variable Declaration
```
let <identifier> = <expression>
const <identifier> = <expression>
```

### Assignment
```
<identifier> = <expression>
```

### Expression
```
<literal>                          // 42, 3.14, true, "hello"
<identifier>                       // x, myVar
<unary-op> <expression>           // -5, !true
<expression> <binary-op> <expression>  // 5 + 3, x * y
(<expression>)                     // (5 + 3) * 2
<identifier>(<args>)              // print(x)
```

### Function Call
```
<function-name>(<arg1>, <arg2>, ...)
```

---

## Built-in Functions

### print()
Outputs values to the console.

**Syntax**:
```arc
print(expr1)
print(expr1, expr2, ...)
```

**Examples**:
```arc
print(42)                    // Output: 42
print("Hello")               // Output: Hello
print(true)                  // Output: true
print(3.14)                  // Output: 3.14
print("Sum:", 5 + 3)        // Output: Sum: 8
```

**Behavior**:
- Evaluates all arguments
- Prints them space-separated
- Adds newline at end
- Returns no value (statement only)

---

## Type System

### Value Types

#### Integer
- 64-bit signed integer
- Range: -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807
- Operations: All arithmetic, bitwise, comparison

#### Float
- IEEE 754 double-precision floating-point
- Operations: All arithmetic (except bitwise), comparison
- Special values: Infinity, -Infinity, NaN

#### Boolean
- Values: `true`, `false`
- Truthy values: true, non-zero numbers, non-empty strings
- Falsy values: false, 0, 0.0, empty string ""

#### String
- UTF-8 encoded text
- Immutable
- Escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`
- Operations: Comparison (lexicographic)

### Type Checking

Arc performs type checking at evaluation time:

```arc
let x = 10          // Type: Integer
x = 20              // OK: same type
x = 3.14            // ERROR: cannot change type
```

Type coercion is automatic in mixed-type operations:

```arc
let result = 5 + 2.5    // OK: 5 promoted to 5.0
```

---

## Examples

### Example 1: Calculator
```arc
let a = 15
let b = 4

print("Addition:", a + b)        // 19
print("Subtraction:", a - b)     // 11
print("Multiplication:", a * b)  // 60
print("Division:", a / b)        // 3
print("Modulo:", a % b)          // 3
print("Power:", 2 ** 10)         // 1024
```

### Example 2: Boolean Logic
```arc
let x = 10
let y = 20

let inRange = x > 5 && x < 15
print("In range:", inRange)      // true

let isExtreme = x < 0 || x > 100
print("Extreme:", isExtreme)     // false
```

### Example 3: Type Coercion
```arc
let intVal = 42
let floatVal = 3.14

let sum = intVal + floatVal
print("Sum:", sum)               // 45.14

let comparison = intVal == 42.0
print("Equal:", comparison)      // true
```

### Example 4: Variables and State
```arc
let counter = 0
print("Initial:", counter)       // 0

counter = counter + 1
print("Incremented:", counter)   // 1

counter = counter * 10
print("Multiplied:", counter)    // 10
```

### Example 5: Comments and Documentation
```arc
// This program calculates circle properties
const PI = 3.14159

let radius = 5

/* Calculate circumference
   C = 2 * π * r */
let circumference = 2 * PI * radius
print("Circumference:", circumference)

// Calculate area: A = π * r^2
let area = PI * radius ** 2
print("Area:", area)
```

---

## API Reference

### For Library Users

If you're using Arc as a library in your Rust project:

#### Execute Code
```rust
use arc_compiler::ast::{Ast, ASTVisitor};
use arc_compiler::ast::lexer::Lexer;
use arc_compiler::ast::parser::Parser;
use arc_compiler::ast::evaluator::ASTEvaluator;

let source = "let x = 10\nx + 5";

// Tokenize
let mut lexer = Lexer::new(source);
let mut tokens = Vec::new();
while let Some(token) = lexer.next_token() {
    tokens.push(token);
}

// Parse
let mut parser = Parser::new(tokens);
let mut ast = Ast::new();
if let Some(stmt) = parser.next_statement() {
    ast.add_statement(stmt);
}

// Evaluate
let mut evaluator = ASTEvaluator::new();
ast.visit(&mut evaluator);

// Check result
if let Some(value) = evaluator.last_value {
    println!("Result: {:?}", value);
}

// Check errors
for error in evaluator.errors {
    eprintln!("Error: {}", error);
}
```

#### Define Variables Programmatically
```rust
use arc_compiler::ast::symbol_table::SymbolTable;
use arc_compiler::ast::types::Value;

let mut table = SymbolTable::new();
table.define("x".to_string(), Value::Integer(42), true);

match table.get_value("x") {
    Ok(value) => println!("x = {:?}", value),
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Error Handling

Arc provides detailed error messages:

### Undefined Variable
```arc
unknown_var
// Error: Variable 'unknown_var' not found
```

### Immutable Assignment
```arc
const PI = 3.14
PI = 3.15
// Error: Cannot assign to immutable variable 'PI'
```

### Redeclaration
```arc
let x = 10
let x = 20
// Error: Variable 'x' already declared
```

### Type Mismatch
Currently, Arc allows type changes in variables, but this may be restricted in future versions.

---

## Running Arc Programs

### REPL Mode
```bash
cargo run
# Interactive prompt appears
```

### File Execution
```bash
# Create a file: program.arc
cargo run -- program.arc

# Or build and run:
cargo build --release
./target/release/rust-compiler program.arc
```

### Example REPL Session
```
=== Arc Compiler REPL ===
Type expressions to evaluate them. Type 'exit' or 'quit' to exit.

>> let x = 10
Integer(10) : Integer

>> x + 5
Integer(15) : Integer

>> print(x)
10

>> // This is a comment
>> const PI = 3.14
Float(3.14) : Float

>> exit
Goodbye!
```

---

## Performance Considerations

Arc is designed for learning, not performance. However:

- **Lexer**: O(n) where n is source length
- **Parser**: O(n) for expression parsing
- **Symbol Table**: O(1) average lookup (HashMap-based)
- **Evaluator**: O(n) where n is AST nodes

For production use, consider:
- Bytecode compilation
- JIT compilation
- Optimized data structures

---

## Future Enhancements

See [FUTURE_SCOPE.md](FUTURE_SCOPE.md) for the complete roadmap:

**Coming Soon**:
- Control flow (`if`, `while`, `for`)
- Functions and closures
- Nested scopes
- Arrays and tuples
- More built-in functions
- Standard library

---

## Contributing

Contributions welcome! See the main README for guidelines.

**Areas needing help**:
- Enhanced REPL (readline, history, syntax highlighting)
- More built-in functions
- Better error messages
- Performance optimizations
- Documentation improvements

---

## License

See LICENSE file in the repository root.
