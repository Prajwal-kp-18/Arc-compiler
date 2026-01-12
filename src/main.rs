mod ast;
use ast::lexer::Token;
use ast::Ast;
use ast::parser::Parser;

fn main() {
    println!("=== Arc Compiler - Comparison & Logical Operators Test ===\n");

    // Test cases for comparison and logical operators
    let test_cases = vec![
        // Comparison operators with integers
        ("5 == 5", "Integer equality: 5 == 5"),
        ("5 == 3", "Integer inequality: 5 == 3"),
        ("5 != 3", "Not equal: 5 != 3"),
        ("10 > 5", "Greater than: 10 > 5"),
        ("3 < 7", "Less than: 3 < 7"),
        ("5 >= 5", "Greater or equal: 5 >= 5"),
        ("3 <= 5", "Less or equal: 3 <= 5"),
        
        // Comparison with floats
        ("3.14 == 3.14", "Float equality"),
        ("2.5 > 1.5", "Float greater than"),
        ("1.0 < 2.0", "Float less than"),
        
        // Mixed type comparisons (int and float)
        ("5 == 5.0", "Mixed equality: 5 == 5.0"),
        ("10 > 5.5", "Mixed comparison: 10 > 5.5"),
        ("3.5 < 7", "Mixed comparison: 3.5 < 7"),
        
        // Boolean comparisons
        ("true == true", "Boolean equality"),
        ("true != false", "Boolean inequality"),
        
        // String comparisons
        ("\"hello\" == \"hello\"", "String equality"),
        ("\"abc\" < \"xyz\"", "String less than"),
        
        // Logical AND with short-circuit
        ("true && true", "Logical AND: true && true"),
        ("true && false", "Logical AND: true && false"),
        ("false && true", "Logical AND: false && true (short-circuit)"),
        
        // Logical OR with short-circuit
        ("true || false", "Logical OR: true || false (short-circuit)"),
        ("false || true", "Logical OR: false || true"),
        ("false || false", "Logical OR: false || false"),
        
        // Logical NOT
        ("!true", "Logical NOT: !true"),
        ("!false", "Logical NOT: !false"),
        ("!(5 > 3)", "Logical NOT with expression"),
        
        // Complex logical expressions
        ("true && true && true", "Multiple AND"),
        ("false || false || true", "Multiple OR"),
        ("true && false || true", "Mixed: true && false || true"),
        ("(true || false) && true", "Parenthesized: (true || false) && true"),
        
        // Comparison with arithmetic
        ("5 + 3 == 8", "Arithmetic in comparison: 5+3 == 8"),
        ("10 - 2 > 5", "Arithmetic in comparison: 10-2 > 5"),
        ("3 * 4 <= 12", "Arithmetic in comparison: 3*4 <= 12"),
        
        // Complex boolean expressions
        ("5 > 3 && 10 < 20", "Comparison && Comparison"),
        ("5 == 5 || 3 > 10", "Comparison || Comparison"),
        ("!(5 < 3) && 10 == 10", "NOT with AND"),
        
        // Type coercion in comparisons
        ("5 > 2.5 && 10.0 == 10", "Mixed types in logical expression"),
        
        // Truthy/falsy evaluation
        ("!0", "NOT on integer (falsy)"),
        ("!5", "NOT on non-zero integer (truthy)"),
        
        // Edge cases
        ("true && (5 > 3)", "Boolean && Comparison"),
        ("false || (10 == 10)", "Boolean || Comparison"),
        ("(3 < 5) == true", "Comparison result compared to boolean"),
    ];

    let mut total_tests = 0;
    let mut passed_tests = 0;

    for (input, description) in test_cases {
        total_tests += 1;
        println!("─────────────────────────────────────");
        println!("Test: {}", description);
        println!("Input: {}", input);
        
        let mut lexer = ast::lexer::Lexer::new(input);
        let mut tokens: Vec<Token> = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }

        let mut ast: Ast = Ast::new();
        let mut parser = Parser::new(tokens);
        
        match parser.next_statement() {
            Some(statement) => {
                ast.add_statement(statement);
                
                let mut evaluator = ast::evaluator::ASTEvaluator::new();
                ast.visit(&mut evaluator);
                
                if let Some(value) = &evaluator.last_value {
                    println!("Result: {}", value);
                    println!("Type: {}", value.get_type());
                    passed_tests += 1;
                } else {
                    println!("Result: Error");
                }
                
                if !evaluator.errors.is_empty() {
                    println!("Errors:");
                    for error in &evaluator.errors {
                        println!("  - {}", error);
                    }
                }
            },
            None => {
                println!("Result: Parse error");
            }
        }
        println!();
    }

    println!("═════════════════════════════════════");
    println!("═════════════════════════════════════");
    println!("Tests Completed: {}/{} passed", passed_tests, total_tests);
    println!("═════════════════════════════════════");
}
