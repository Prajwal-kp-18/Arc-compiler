mod ast;
use ast::lexer::Token;
use ast::Ast;
use ast::parser::Parser;

fn main() {
    println!("=== Arc Compiler - Extended Operations Test ===\n");

    // Test cases for all new operations
    let test_cases = vec![
        // Basic arithmetic (existing)
        ("7 - (30 + 7) * 8 / 2", "Basic arithmetic with parentheses"),
        
        // Modulo operator
        ("10 % 3", "Modulo: 10 % 3 = 1"),
        ("17 % 5", "Modulo: 17 % 5 = 2"),
        
        // Exponentiation
        ("2 ** 3", "Exponentiation: 2^3 = 8"),
        ("5 ** 2", "Exponentiation: 5^2 = 25"),
        
        // Unary operators
        ("-5", "Unary minus: -5"),
        ("+10", "Unary plus: +10"),
        ("-(3 + 2)", "Unary minus with expression: -(3+2) = -5"),
        
        // Bitwise AND
        ("12 & 10", "Bitwise AND: 12 & 10 = 8"),
        
        // Bitwise OR
        ("12 | 10", "Bitwise OR: 12 | 10 = 14"),
        
        // Bitwise XOR
        ("12 ^ 10", "Bitwise XOR: 12 ^ 10 = 6"),
        
        // Left Shift
        ("4 << 2", "Left Shift: 4 << 2 = 16"),
        
        // Right Shift
        ("16 >> 2", "Right Shift: 16 >> 2 = 4"),
        
        // Complex expressions
        ("2 ** 3 + 5 * 2", "Complex: 2^3 + 5*2 = 18"),
        ("10 % 3 * 2", "Complex: (10%3)*2 = 2"),
        ("-5 + 10", "Unary with binary: -5+10 = 5"),
    ];

    for (input, description) in test_cases {
        println!("Test: {}", description);
        println!("Input: {}", input);
        
        let mut lexer = ast::lexer::Lexer::new(input);
        let mut tokens: Vec<Token> = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }

        let mut ast: Ast = Ast::new();
        let mut parser = Parser::new(tokens);
        while let Some(statement) = parser.next_statement() {
            ast.add_statement(statement);
        }

        let mut evaluator = ast::evaluator::ASTEvaluator::new();
        ast.visit(&mut evaluator);
        
        println!("Result: {:?}\n", evaluator.last_value);
    }

    println!("=== All tests completed! ===");
}
