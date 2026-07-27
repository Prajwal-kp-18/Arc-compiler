// Arc Compiler Demo Program
// This file demonstrates the supported features of the Arc language.

print("=== Arc Compiler Demo ===")
print("This demo walks through the features currently supported by Arc.")

// Variables and constants
print("")
print("1) Variables and constants")
let x = 10
let y = 20
const PI = 3.14159
print("x is a mutable integer:", x)
print("y is another mutable integer:", y)
print("PI is an immutable constant:", PI)

// Arithmetic
print("")
print("2) Arithmetic and precedence")
let sum = x + y
let product = x * y
let complex_math = (x + y) * 2 - 10
print("x + y =", sum)
print("x * y =", product)
print("(x + y) * 2 - 10 =", complex_math)

// Type coercion
print("")
print("3) Type coercion")
let mixed = x + PI
print("x + PI automatically becomes a float:", mixed)

// Comparisons and logical operators
print("")
print("4) Comparisons and logical operators")
let is_greater = x > 5
let in_range = x < 20 && y > 10
let out_of_range = x < 5 || y > 10
print("x > 5:", is_greater)
print("x < 20 && y > 10:", in_range)
print("x < 5 || y > 10:", out_of_range)

// Strings
print("")
print("5) Strings")
let greeting = "Hello"
let language = "Arc"
print("Greeting:", greeting)
print("Language name:", language)

// Bitwise operators
print("")
print("6) Bitwise operators")
let bitwise_and = x & 7
let bitwise_or = x | 3
let bitwise_xor = x ^ 6
let shifted_left = x << 1
let shifted_right = y >> 1
print("x & 7 =", bitwise_and)
print("x | 3 =", bitwise_or)
print("x ^ 6 =", bitwise_xor)
print("x << 1 =", shifted_left)
print("y >> 1 =", shifted_right)

// Unary operators
print("")
print("7) Unary operators")
let unary_sample = -x
let positive_sample = +y
print("-x =", unary_sample)
print("+y =", positive_sample)

// Prefix and postfix increment/decrement
print("")
print("8) Prefix and postfix increment/decrement")
let counter = 10
print("counter starts at:", counter)
print("++counter returns the incremented value:", ++counter)
print("counter after prefix increment:", counter)
print("counter++ returns the old value:", counter++)
print("counter after postfix increment:", counter)
print("--counter returns the decremented value:", --counter)
print("counter-- returns the old value:", counter--)
print("counter at the end:", counter)

// Function calls
print("")
print("9) Built-in functions")
print("min(4, 2, 9, 1) =", min(4, 2, 9, 1))
print("max(4, 2, 9, 1) =", max(4, 2, 9, 1))
print("int(\"42\") =", int("42"))
print("float(\"3.5\") =", float("3.5"))
print("str(42) =", str(42))
print("len(\"hello\") =", len("hello"))
print("substr(\"hello world\", 6, 5) =", substr("hello world", 6, 5))
print("find(\"hello world\", \"world\") =", find("hello world", "world"))
print("upper(\"hi\") =", upper("hi"))
print("lower(\"HI\") =", lower("HI"))
print("trim(\"  hi  \") =", trim("  hi  "))
print("char_at(\"hello\", 1) =", char_at("hello", 1))
print("ord(\"A\") =", ord("A"))
print("chr(65) =", chr(65))
print("abs(-5) =", abs(-5))
print("sqrt(16.0) =", sqrt(16.0))
print("floor(3.7) =", floor(3.7))
print("ceil(3.2) =", ceil(3.2))
print("round(3.5) =", round(3.5))
assert(x + y == 30, "assert() would raise this on failure")
print("assert(x + y == 30) passed")

// User-defined functions
print("")
print("10) User-defined functions")
fn add(a, b) {
    return a + b
}

fn square(n: Int) {
    return n * n
}

fn hypotenuse(a: Int, b: Int) {
    return (a * a + b * b) ** 0.5
}

print("add(12, 8) =", add(12, 8))
print("square(7) =", square(7))
print("hypotenuse(6, 8) =", hypotenuse(6, 8))

// Assignment and if/else blocks
print("")
print("11) Assignment and if/else blocks")
let score = 10
print("score before the if statement:", score)
if score == 10 {
    print("The condition 'score == 10' is true.")
    print("Inside the then-branch we add 5 to score.")
    score = score + 5
    let branch_only = 10
    print("branch_only is visible inside the branch:", branch_only)
} else {
    print("The condition 'score == 10' is false.")
}
print("score after the if statement:", score)

// While and for loops
print("")
print("12) While and for loops")
let loop_sum = 0
let i = 0
while i < 5 {
    loop_sum = loop_sum + i
    i = i + 1
}
print("sum of 0..4 via while:", loop_sum)

let loop_total = 0
for j in 0..5 {
    loop_total = loop_total + j
}
print("sum of 0..4 via for:", loop_total)

fn first_over(limit) {
    let n = 0
    while true {
        if n > limit {
            return n
        }
        n = n + 1
    }
}
print("first_over(3) (return inside a loop):", first_over(3))

// Comments are ignored by the lexer.
// Single-line comments are fine.
/* Multi-line comments are also supported. */

// -----------------------------------------------------------------------------
// 13) Regression tests (manual)
// -----------------------------------------------------------------------------
print("")
print("13) Regression tests (manual)")

// Test A: bare block works and does not leak shadowed value
let outer = 5
{
    let outer = 99
    print("inside block, outer =", outer) // expected: 99
}
print("outside block, outer =", outer) // expected: 5

// Test B: if block scope still works
if true {
    let inside_if = 123
    print("inside if block =", inside_if) // expected: 123
}

// Test C: quick PASS/FAIL checks
let tests_passed = 0
let tests_failed = 0

let t1 = (outer == 5)
if t1 {
    tests_passed = tests_passed + 1
    print("[PASS] outer value preserved after block")
} else {
    tests_failed = tests_failed + 1
    print("[FAIL] outer value preserved after block")
}

let t2 = (add(2, 3) == 5)
if t2 {
    tests_passed = tests_passed + 1
    print("[PASS] function add(2,3)")
} else {
    tests_failed = tests_failed + 1
    print("[FAIL] function add(2,3)")
}

let t3 = (square(4) == 16)
if t3 {
    tests_passed = tests_passed + 1
    print("[PASS] function square(4)")
} else {
    tests_failed = tests_failed + 1
    print("[FAIL] function square(4)")
}

let t4 = (loop_sum == 10)
if t4 {
    tests_passed = tests_passed + 1
    print("[PASS] while loop sum")
} else {
    tests_failed = tests_failed + 1
    print("[FAIL] while loop sum")
}

let t5 = (loop_total == 10)
if t5 {
    tests_passed = tests_passed + 1
    print("[PASS] for loop sum")
} else {
    tests_failed = tests_failed + 1
    print("[FAIL] for loop sum")
}

print("tests_passed =", tests_passed)
print("tests_failed =", tests_failed)

// Expected-runtime-error tests (uncomment one by one to validate diagnostics):
// print(not_declared_var)
// prnit("typo in function name")

print("")
print("=== Demo Complete ===")