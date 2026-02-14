// Arc Compiler Demo Program
// This file demonstrates the key features of the Arc language

// Variable declarations
let x = 10
let y = 20
const PI = 3.14159

// Print basic values
print("=== Arc Compiler Demo ===")
print("Variables:")
print(x)
print(y)
print(PI)

// Arithmetic operations
let sum = x + y
print("sum: " + sum)

let product = x * y
print("product: " + product)

// Type coercion
let mixed = x + PI
print(mixed)

// Boolean operations
let isGreater = x > 5
print("isGreater: " + isGreater)

let result = x < 20 && y > 10
print(result)

// Comments are ignored
// This is a single-line comment

// String variables
let greeting = "Hello"
let name = "Arc"
print(greeting)
print(name)

// Complex expressions
let calculation = (x + y) * 2 - 10
print(calculation)

// Mutable variables can be reassigned
x = x + 5
print(x)

// Const variables cannot be reassigned (this would cause an error)
// PI = 3.14  // Uncommenting this will cause an error

print("=== Demo Complete ===")
