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
//! 3. [`ast::resolver::Resolver`] — statically resolves every variable/function
//!    reference to a slot, infers expression types, and catches
//!    undeclared-name, duplicate-declaration, immutability, arity, and
//!    type-mismatch errors before evaluation ever starts
//! 4. [`ast::evaluator::ASTEvaluator`] — traverses the resolved AST and
//!    produces values, indexing variable storage directly by slot
//!
//! ## Quick Start
//!
//! ```rust
//! use arc_compiler::ast::{Ast, ASTVisitor};
//! use arc_compiler::ast::lexer::Lexer;
//! use arc_compiler::ast::parser::Parser;
//! use arc_compiler::ast::resolver::Resolver;
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
//! let mut resolver = Resolver::new();
//! resolver.resolve(&ast);
//! assert!(!resolver.has_errors());
//!
//! let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
//! evaluator.execute(&ast);
//! ```

pub mod ast;
pub mod bytecode;
pub mod ir;
