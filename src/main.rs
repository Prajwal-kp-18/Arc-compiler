mod ast;
use ast::lexer::Token;
use ast::Ast;
use ast::parser::Parser;

fn main() {
    println!("=== Arc Compiler - Data Types & Type System Test ===\n");

    // Test cases for all data types
    let test_cases = vec![
        // Integer operations
        ("7 - (30 + 7) * 8 / 2", "Integer arithmetic"),
        ("10 % 3", "Integer modulo"),
        ("2 ** 3", "Integer exponentiation"),
        
        // Floating-point operations
        ("3.14 + 2.86", "Float addition: 3.14 + 2.86"),
        ("10.5 * 2.0", "Float multiplication"),
        ("15.0 / 3.0", "Float division"),
        ("2.5 ** 2.0", "Float exponentiation"),
        
        // Mixed integer and float (type coercion)
        ("10 + 5.5", "Mixed int + float (coercion to float)"),
        ("3.5 * 2", "Mixed float * int"),
        ("10 / 2.5", "Mixed int / float"),
        
        // Boolean literals
        ("true", "Boolean true"),
        ("false", "Boolean false"),
        
        // String literals
        ("\"Hello, World!\"", "String literal"),
        ("\"Arc\" + \" Compiler\"", "String concatenation"),
        ("\"Value: \" + \"42\"", "String + string"),
        
        // Unary operators with different types
        ("-5", "Unary minus on integer"),
        ("-3.14", "Unary minus on float"),
        ("+42", "Unary plus on integer"),
        
        // Complex expressions with mixed types
        ("2.0 + 3 * 4.5", "Complex: 2.0 + 3*4.5"),
        ("10.0 % 3.0", "Float modulo"),
        
        // Bitwise operations (integers only)
        ("12 & 10", "Bitwise AND"),
        ("4 << 2", "Left shift"),
        
        // Error cases (will show type errors)
        // These might fail but demonstrate type checking
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
    println!("Tests Completed: {}/{} passed", passed_tests, total_tests);
    println!("═════════════════════════════════════");
}
