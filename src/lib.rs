//! # Arc Compiler
//!
//! Arc is a lightweight interpreted expression language built in Rust,
//! designed as a learning project in compiler construction.
//!
//! ## Pipeline
//!
//! Source code is processed through four stages:
//!
//! 1. [`ast::lexer::Lexer`] — tokenizes source text
//! 2. [`ast::parser::Parser`] — builds an Abstract Syntax Tree
//! 3. [`ast::symbol_table::SymbolTable`] — resolves variables and enforces scope
//! 4. [`ast::evaluator::ASTEvaluator`] — traverses the AST and produces values
//!
//! ## Quick Start
//!
//! ```rust
//! use arc_compiler::ast::{Ast, ASTVisitor};
//! use arc_compiler::ast::lexer::Lexer;
//! use arc_compiler::ast::parser::Parser;
//! use arc_compiler::ast::evaluator::ASTEvaluator;
//!
//! let source = "let x = 10";
//!
//! let mut lexer = Lexer::new(source);
//! let mut tokens = Vec::new();
//! while let Some(token) = lexer.next_token() {
//!     tokens.push(token);
//! }
//!
//! let mut parser = Parser::new(tokens);
//! let mut ast = Ast::new();
//! if let Some(stmt) = parser.next_statement() {
//!     ast.add_statement(stmt);
//! }
//!
//! let mut evaluator = ASTEvaluator::new();
//! ast.visit(&mut evaluator);
//! ```

pub mod ast;
