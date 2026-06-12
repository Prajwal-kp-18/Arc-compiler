//! Arc Compiler - Supports REPL mode and file execution

mod ast;
use ast::lexer::Token;
use ast::lexer::TokenKind;
use ast::Ast;
use ast::parser::Parser;
use ast::evaluator::ASTEvaluator;
use std::io::{self, Write};
use std::env;
use std::fs;

/// Entry point - runs REPL or executes file from command line
fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        // File execution mode
        let filename = &args[1];
        execute_file(filename);
    } else {
        // REPL mode
        run_repl();
    }
}

/// Reads and executes Arc source file as a complete program
fn execute_file(filename: &str) {
    let contents = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            return;
        }
    };
    
    println!("=== Executing {} ===", filename);
    let mut evaluator = ASTEvaluator::new();

    let mut lexer = ast::lexer::Lexer::new(&contents);
    let mut tokens: Vec<Token> = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token);
    }

    let mut ast: Ast = Ast::new();
    let mut parser = Parser::new(tokens);

    loop {
        match parser.current().map(|token| &token.kind) {
            Some(TokenKind::EOF) | None => break,
            _ => {}
        }

        match parser.next_statement() {
            Some(statement) => ast.add_statement(statement),
            None => {
                if parser.diagnostics.is_empty() {
                    eprintln!("Parse error: Invalid syntax in file '{}'.", filename);
                } else {
                    eprintln!("\n=== Parse Errors ===");
                    for diagnostic in &parser.diagnostics {
                        eprintln!("{}", diagnostic.format_with_source(&contents));
                    }
                }
                return;
            }
        }
    }

    if !parser.diagnostics.is_empty() {
        eprintln!("\n=== Parse Errors ===");
        for diagnostic in &parser.diagnostics {
            eprintln!("{}", diagnostic.format_with_source(&contents));
        }
        return;
    }

    ast.visit(&mut evaluator);
    
    if !evaluator.errors.is_empty() {
        println!("\n=== Errors ===");
        for diagnostic in &evaluator.errors {
            eprintln!("{}", diagnostic.format_with_source(&contents));
        }
    }
}

/// Interactive Read-Eval-Print Loop for testing expressions
fn run_repl() {
    println!("=== Arc Compiler REPL ===");
    println!("Type expressions to evaluate them. Type 'exit' or 'quit' to exit.\n");
    println!("Examples:");
    println!("  let x = 10");
    println!("  x + 5");
    println!("  print(x)");
    println!("  // This is a comment");
    println!("  const pi = 3.14\n");

    let mut evaluator = ASTEvaluator::new();
    let stdin = io::stdin();
    
    loop {
        print!(">> ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        match stdin.read_line(&mut input) {
            Ok(_) => {
                let input = input.trim();
                
                // Exit commands
                if input == "exit" || input == "quit" {
                    println!("Session ended. Goodbye.");
                    break;
                }
                
                // Skip empty lines
                if input.is_empty() {
                    continue;
                }
                
                // Tokenize
                let mut lexer = ast::lexer::Lexer::new(input);
                let mut tokens: Vec<Token> = Vec::new();
                while let Some(token) = lexer.next_token() {
                    tokens.push(token);
                }

                // Parse
                let mut ast: Ast = Ast::new();
                let mut parser = Parser::new(tokens);
                
                match parser.next_statement() {
                    Some(statement) => {
                        ast.add_statement(statement);
                        
                        // Evaluate
                        let error_count_before = evaluator.errors.len();
                        ast.visit(&mut evaluator);
                        let error_count_after = evaluator.errors.len();
                        
                        // Display result
                        if !parser.diagnostics.is_empty() {
                            println!("Parse error:");
                            for diagnostic in &parser.diagnostics {
                                println!("  {}", diagnostic.format_with_source(input));
                            }
                        } else if error_count_after > error_count_before {
                            println!("Error:");
                            for i in error_count_before..error_count_after {
                                println!("  {}", evaluator.errors[i].format_with_source(input));
                            }
                        } else {
                            match &evaluator.last_value {
                                Some(value) => {
                                    println!("{:?} : {:?}", value, value.get_type());
                                }
                                None => {
                                    // Statement executed without producing a value
                                }
                            }
                        }
                    }
                    None => {
                        println!("Parse error: Invalid syntax");
                    }
                }
            }
            Err(error) => {
                println!("Error reading input: {}", error);
                break;
            }
        }
        println!();
    }
}
