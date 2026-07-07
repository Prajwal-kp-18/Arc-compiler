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
- **Statically resolved, gradually typed**: A resolver pass infers types and catches
  undeclared-name, redeclaration, mutability, arity, and type-mismatch errors
  before evaluation starts. Types it can't pin down statically (e.g. an untyped
  parameter) fall back to a dynamically-checked `Any` type, with a warning.
- **Expression-oriented**: Most constructs evaluate to values
- **Interactive REPL**: Test code snippets interactively
- **File execution**: Run complete programs from `.arc` files

---

## Architecture

Arc uses a classic staged compilation pipeline: lex → parse → resolve, then
one of two interchangeable execution backends — the tree-walking evaluator
(the default) or the bytecode compiler + virtual machine (`--backend=vm`).
An optional optimizing IR pipeline (`--opt`) inserts a three-address-code
IR with constant folding, dead code elimination, and common subexpression
elimination between the resolved AST and the VM. All execution modes are
held to byte-identical output on a golden test suite.

### 1. Lexical Analysis (Lexer)
**Location**: `src/ast/lexer.rs`

Converts source code into a stream of tokens.

**Token Types**:
- **Literals**: `Number`, `Float`, `Boolean`, `String`
- **Operators**: `+`, `-`, `*`, `/`, `%`, `**`, `&`, `|`, `^`, `<<`, `>>`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `++`, `--`
- **Keywords**: `let`, `const`, `fn`, `return`, `if`, `else`
- **Delimiters**: `(`, `)`, `,`, `:`, `{`, `}`, `;`
- **Special**: `=`, `Identifier`, `EOF`, `Whitespace`, `Bad`

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
- Bare block statements (`{ ... }`)
- If/else statements
- Function declarations
- Return statements

**Expression Types**:
- Number literals (integer and float)
- Boolean literals (`true`, `false`)
- String literals
- Identifiers (variable references)
- Binary expressions (with operator precedence)
- Unary expressions (`-x`, `+x`, `!x`)
- Prefix increment/decrement (`++x`, `--x`) and postfix increment/decrement (`x++`, `x--`) are supported. Prefix forms evaluate to the updated value; postfix forms evaluate to the original value and then update the variable.
- Parenthesized expressions
- Function calls

**Operator Precedence** (11 levels, lowest to highest):
1. Logical OR (`||`)
2. Logical AND (`&&`)
3. Equality (`==`, `!=`)
4. Comparison (`<`, `>`, `<=`, `>=`)
5. Bitwise OR (`|`)
6. Bitwise XOR (`^`)
7. Bitwise AND (`&`)
8. Bit shifts (`<<`, `>>`)
9. Addition/Subtraction (`+`, `-`)
10. Multiplication/Division/Modulo (`*`, `/`, `%`)
11. Exponentiation (`**`, right-associative)

### 3. Resolver
**Location**: `src/ast/resolver.rs`

Walks the AST once, after parsing and before evaluation, and statically
resolves every variable and function reference.

**Features**:
- Assigns every `let`/`const`/parameter a stable slot index (function-frame-relative
  for locals, a flat space for globals) — the evaluator addresses variables by
  slot, not by a runtime name lookup
- Infers a concrete type for every expression using the same widening rules
  the evaluator's coercion uses, falling back to a dynamically-checked `Any`
  type (with a warning) where it can't be pinned down statically
- Resolves function declarations with lexical scoping and hoisting, so
  sibling functions can call each other regardless of declaration order
  (forward references and mutual recursion) without a runtime name-lookup stack
- Catches undeclared-variable, redeclaration, immutable-assignment,
  type-mismatch, and arity errors **before evaluation starts**, using the
  same diagnostic text the evaluator used to produce at runtime
- Nested function declarations do not capture their enclosing function's
  locals (only their own frame and the global scope are visible) — a
  documented simplification, not a full closure model

### 4. Evaluation (Evaluator)
**Location**: `src/ast/evaluator.rs`

Executes the resolved AST using the Visitor pattern.

**Features**:
- Slot-indexed variable storage (a flat `Vec` per call frame, plus one for
  globals) instead of a scoped, string-keyed symbol table
- Automatic type coercion for the runtime arithmetic itself (the Resolver
  already validated the operation is legal ahead of time)
- Short-circuit evaluation for logical operators
- Error collection without stopping execution
- Built-in function support
- User-defined function support; function bodies are registered once (not
  re-cloned per call)

### 5. Bytecode Compiler (VM backend, part 1)
**Location**: `src/bytecode/` (`opcode.rs`, `chunk.rs`, `compiler.rs`, `disassembler.rs`)

Lowers the Resolver-annotated AST into bytecode `Chunk`s — one per function
plus one for the top-level script — executed by the VM below, or inspected
via the built-in disassembler (`--dump-bytecode`).

**Features**:
- Small, generic instruction set (~35 opcodes) dispatching dynamically over
  the same tagged `Value` enum the evaluator uses
- Trusts the Resolver completely: reads slot indices, function ids, and
  resolved bindings straight off the AST — infallible, no error type
- `++`/`--` and `&&`/`||` desugar to generic get/set/arithmetic/jump
  sequences instead of dedicated opcodes
- Statement compilation is stack-neutral; every chunk ends in an explicit
  `RETURN`, and the implicit-return rule matches the evaluator exactly (only
  a trailing expression statement produces a value — anything else produces
  `Unit`, the "no value" value)
- Per-byte source-offset table in each chunk for error reporting
- Disassembler prints one line per instruction: offset, source position,
  mnemonic, decoded operands, and inline constant values

### 6. Virtual Machine (VM backend, part 2)
**Location**: `src/bytecode/vm.rs`

A stack-based VM executing compiled chunks — the second, independently
verified execution engine (`--backend=vm`).

**Features**:
- One shared operand stack; one call frame per active call holding its own
  instruction pointer and local slots (direct slot indexing, no name lookups)
- Calls are iterative, so Arc recursion consumes no host stack — but call
  depth is capped at the same limit as the evaluator, with the same
  stack-overflow error message
- Binary-operator semantics come from the same shared `apply_binary`
  function the evaluator uses, so the two backends cannot drift apart
- Runtime errors carry source positions recovered from the chunks'
  offset tables — same line/column rendering as evaluator errors
- Halts on the first runtime error (the evaluator records and continues) —
  identical behavior on error-free programs
- Meaningfully faster than the tree-walker: ~1.7× on a recursive-fibonacci
  benchmark (`examples/bench.arc`)

### 7. Optimizing IR (optional pipeline stage)
**Location**: `src/ir/` (`instr.rs`, `lower.rs`, `passes.rs`, `dump.rs`, `to_bytecode.rs`)

With `--opt`, the resolved AST lowers to a three-address-code IR (virtual
registers, basic blocks, explicit control-flow graph), runs a pass pipeline,
then lowers back to bytecode for the VM to execute.

**Passes** (each preserves behavior *including runtime errors* — a fold that
would error doesn't fold, and only provably non-erroring instructions are
eliminated or deduplicated):
- **Constant folding** — evaluates constant expressions at compile time via
  the same shared arithmetic both backends execute; constant branch
  conditions turn into unconditional jumps
- **Dead code elimination** — deletes pure instructions with unused results
  and blocks unreachable from entry
- **Common subexpression elimination** (block-local) — reuses the result of
  a repeated computation instead of recomputing it

Inspect the IR before and after optimization:
```bash
cargo run -- --dump-ir program.arc       # after lowering
cargo run -- --dump-ir=opt program.arc   # after the pass pipeline
```

On a constant-heavy program the pipeline reduces the IR by ~70%; the output
is verified byte-identical to the unoptimized backends by the golden suite.

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
Variables use lexical scoping. Bare blocks, `if`/`else` branches, and function calls create inner scopes. Lookup searches from the innermost scope outward, and redeclaration is only rejected within the same scope.

```arc
let value = 5
{
    let value = 99
    print(value)  // 99
}
print(value)      // 5
```

### Blocks and Conditionals

```arc
let score = 10

if score == 10 {
    print("perfect")
} else {
    print("try again")
}

{
    let local = "inside block"
    print(local)
}
```

Conditions use truthiness: `false`, `0`, `0.0`, and `""` are false; everything else is true.

### User-Defined Functions

```arc
fn add(a, b) {
    return a + b
}

fn square(n: Int) {
    return n * n
}

print(add(12, 8))
print(square(7))
```

Functions are declared with `fn`, take positional parameters, and may return a value with `return`. A function without an explicit `return` evaluates to the value of its **trailing expression statement** — and only that: if the body ends in a `let`, an assignment, a block, or an `if`, the function produces no value, and using its result is a runtime error. (This rule is uniform across the tree-walking evaluator and the bytecode compiler.)

Parameters can optionally carry a type annotation — `n: Int` above — using one
of the soft-keyword type names `Int`, `Float`, `Bool`, or `String`. An
annotated parameter is type-checked at the call site before the function ever
runs; an unannotated one is checked dynamically, same as before type
annotations existed. Return types are always inferred from the function's
`return` statements, never annotated.

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

#### Increment and Decrement
```arc
let counter = 10
++counter   // 11, counter is now 11
counter++   // 11, counter is now 12
--counter   // 11, counter is now 11
counter--   // 11, counter is now 10
```

Increment and decrement only work on mutable integer and float variables.

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

### Block
```
{
    <statement>
    ...
}
```

### If/Else
```
if <expression> {
    <statement>
    ...
} else {
    <statement>
    ...
}
```

### Function Declaration
```
fn <identifier>(<param1>[: <Type>], <param2>[: <Type>], ...) {
    <statement>
    return <expression>
}
```

`<Type>` is one of `Int`, `Float`, `Bool`, `String`. Annotations are optional per-parameter.

### Return
```
return <expression>
```

### Expression
```
<literal>                          // 42, 3.14, true, "hello"
<identifier>                       // x, myVar
<unary-op> <expression>            // -5, !true, ++x
<expression> <postfix-op>          // x++, x--
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
- Returns no value

---

### min()
Returns the smallest of the given arguments. Arguments must be comparable (integers and floats are supported together; strings compare lexicographically).

**Syntax**:
```arc
min(expr1, expr2, ...)
```

**Examples**:
```arc
min(5, 2, 8)        // 2
min(3.5, 2, 4.1)    // 2.0
min("b", "a")    // "a"
```

**Behavior**:
- Evaluates all arguments; returns the minimum value according to the language's ordering rules
- Requires at least one argument; otherwise a runtime error is reported

---

### max()
Returns the largest of the given arguments. Arguments must be comparable (integers and floats are supported together; strings compare lexicographically).

**Syntax**:
```arc
max(expr1, expr2, ...)
```

**Examples**:
```arc
max(5, 2, 8)        // 8
max(3.5, 2, 4.1)    // 4.1
max("b", "a")    // "b"
```

**Behavior**:
- Evaluates all arguments; returns the maximum value according to the language's ordering rules
- Requires at least one argument; otherwise a runtime error is reported

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
- Operations: Concatenation with `+` and comparison (lexicographic)

### Type Checking

Arc's Resolver performs type checking statically, before evaluation starts,
wherever it can determine a concrete type:

```arc
let x = 10          // Type: Integer, inferred from the initializer
x = 20              // OK: same type
x = 3.14            // Resolve error: cannot change type
```

Type coercion is automatic in mixed-type operations, and the widening
decision itself is made statically by the Resolver:

```arc
let result = 5 + 2.5    // OK: 5 promoted to 5.0
```

Where a type genuinely can't be determined statically — an untyped function
parameter with no type-revealing usage, or a recursive function's return type
with no reachable non-recursive base case — the Resolver assigns it the `Any`
type instead of failing. `Any` is checked dynamically at runtime, exactly like
every value was before the Resolver existed, and the Resolver emits a
non-fatal warning when it falls back to it so the gap is visible rather than
silent.

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
use arc_compiler::ast::Ast;
use arc_compiler::ast::lexer::Lexer;
use arc_compiler::ast::parser::Parser;
use arc_compiler::ast::resolver::Resolver;
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

// Resolve: statically resolve names/types before evaluation runs
let mut resolver = Resolver::new();
resolver.resolve(&ast);
for diagnostic in &resolver.diagnostics {
    eprintln!("{}", diagnostic.message);
}

// Only evaluate if resolution found no blocking errors
if !resolver.has_errors() {
    let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
    evaluator.execute(&ast);

    // Check result
    if let Some(value) = evaluator.last_value {
        println!("Result: {:?}", value);
    }

    // Check errors
    for error in evaluator.errors {
        eprintln!("Error: {}", error);
    }
}
```

---

## Error Handling

Arc provides detailed error messages. The four categories below are all
caught by the Resolver before evaluation starts (they used to be runtime-only
checks) — the message text is unchanged, only the timing improved.

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
Once a variable is declared, its type is fixed. Assigning a value of a different type is rejected,
except that an `Integer` can be assigned to a `Float` variable (implicit widening).
```arc
let x = 10
x = "hello"
// Error: Type mismatch: variable 'x' has type Integer, cannot assign String
```

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

### Choosing an Execution Backend
```bash
# Tree-walking evaluator (default)
cargo run -- program.arc --backend=tree-walk

# Bytecode VM — same output, faster
cargo run -- program.arc --backend=vm

# Optimizing IR pipeline (fold/DCE/CSE), executed on the VM
cargo run -- program.arc --opt
```

### Bytecode Disassembly
```bash
# Compile to bytecode and print the disassembly (no execution)
cargo run -- --dump-bytecode program.arc
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
- **Resolver**: O(n) single pass over the AST; variable/function lookups are O(1) slot indexing, no hashing
- **Evaluator**: O(n) where n is AST nodes; variable access is direct `Vec` indexing by slot
- **VM**: same asymptotics but a flat dispatch loop instead of a recursive
  AST walk — ~1.7× faster on a call-heavy benchmark (`examples/bench.arc`)

For production use, consider:
- JIT / native compilation
- Optimized data structures

---

## Future Enhancements

**Coming Soon**:
- `while`/`for` loops
- Real closures (nested functions capturing enclosing locals)
- Arrays and tuples
- More built-in functions
- Standard library
- A native backend (LLVM or Cranelift) consuming the optimizing IR

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
