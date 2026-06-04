//! # Parser
//!
//! Converts a flat token stream from the [`Lexer`](crate::ast::lexer::Lexer)
//! into an Abstract Syntax Tree using **recursive descent** with
//! **operator-precedence climbing** for binary expressions.
//!
//! ## Precedence table (lowest → highest)
//!
//! | Level | Operators |
//! |-------|-----------|
//! | 1 | `\|\|` |
//! | 2 | `&&` |
//! | 3 | `==` `!=` |
//! | 4 | `<` `>` `<=` `>=` |
//! | 5 | `\|` |
//! | 6 | `^` |
//! | 7 | `&` |
//! | 8 | `<<` `>>` |
//! | 9 | `+` `-` |
//! | 10 | `*` `/` `%` |
//! | 11 | `**` |
//!
//! ## Statement dispatch
//!
//! [`Parser::parse_statement`] uses one token of lookahead to decide:
//! - `let` / `const` → variable declaration
//! - `identifier =` → assignment
//! - anything else → expression statement

use crate::ast::lexer::Token;
use crate::ast::ASTBinaryOperator;
use crate::ast::ASTBinaryOperatorKind;
use crate::ast::ASTUnaryOperator;
use crate::ast::ASTUnaryOperatorKind;
use crate::ast::{ASTStatement, ASTExpression, ASTVariableDeclaration, ASTAssignment, ASTIfStatement};
use crate::ast::lexer::TokenKind;

/// Parses a token stream into an AST.
///
/// Whitespace tokens are stripped on construction. Call
/// [`next_statement`](Parser::next_statement) in a loop until it returns
/// `None` (EOF) to obtain all statements.
pub struct Parser {
    tokens: Vec<crate::ast::lexer::Token>,
    current: usize,
}

impl Parser {
    /// Creates a parser from a token list, filtering out whitespace.
    pub fn new(
        tokens: Vec<Token>,
    ) -> Self {
        Parser {
            tokens: tokens.iter().filter(|token| token.kind != TokenKind::Whitespace).cloned().collect(),
            current: 0,
        }
    }

    pub fn from_tokens(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            current: 0,
        }
    }

    

    /// Returns the next parsed statement, or `None` at EOF.
    pub fn next_statement(&mut self) -> Option<ASTStatement>{
        return self.parse_statement();
    }

    /// Dispatches to the correct statement parser based on the current token.
    ///
    /// - `let` / `const` → [`parse_variable_declaration`](Self::parse_variable_declaration)
    /// - `identifier =` → [`parse_assignment`](Self::parse_assignment)
    /// - otherwise → expression statement
    pub fn parse_statement(&mut self) -> Option<ASTStatement> {
        let token: &Token = self.current()?;
        if token.kind == TokenKind::EOF {
            return None;
        }
        
        // Check for variable declaration (let or const)
        if matches!(token.kind, TokenKind::Let | TokenKind::Const) {
            return self.parse_variable_declaration();
        }

        // Check for if statement
        if token.kind == TokenKind::IF {
            return self.parse_if_statement();
        }

        // Check for assignment - needs lookahead to distinguish from identifier expression
        if let TokenKind::Identifier(_) = token.kind {
            if self.peek(1).map(|t| &t.kind) == Some(&TokenKind::Equal) {
                return self.parse_assignment();
            }
        }
        
        // Otherwise, parse as expression statement
        let expr = self.parse_expression()?;
        
        // Consume optional semicolon
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }
        
        return Some(ASTStatement::expression(expr));
    }

    /// Parses an `if` statement.
    pub fn parse_if_statement(&mut self) -> Option<ASTStatement> {
        self.consume(); // Consume the 'if' token
        let condition = self.parse_expression()?;
        let then_branch = self.parse_block()?;
        let else_branch = if self.current().map(|t| &t.kind) == Some(&TokenKind::ELSE) {
            self.consume(); // Consume the 'else' token
            Some(self.parse_block()?)
        } else {
            None
        };
        Some(ASTStatement::if_statement(ASTIfStatement::new(condition, then_branch, else_branch)))
    }

    pub fn parse_block(&mut self) -> Option<Vec<ASTStatement>> {
        if self.consume()?.kind != TokenKind::LeftBrace {
            return None;
        }

        let mut statements = Vec::new();

        while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBrace) {
            if self.current().is_none() {
                eprintln!("Unclosed block");
                return None;
            }

            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }

        self.consume(); // consume RightBrace
        Some(statements)
    }

    /// Parses `let <name> = <expr>` or `const <name> = <expr>`.
    ///
    /// Returns `None` and prints a diagnostic if the syntax is malformed.
    pub fn parse_variable_declaration(&mut self) -> Option<ASTStatement> {
        let keyword = self.consume()?;
        let is_mutable = keyword.kind == TokenKind::Let;
        
        let name_token = self.consume()?;
        let name = match name_token.kind {
            TokenKind::Identifier(ref n) => n.clone(),
            _ => {
                eprintln!("Expected identifier after '{}' keyword", 
                    if is_mutable { "let" } else { "const" });
                return None;
            }
        };
        
        if self.consume()?.kind != TokenKind::Equal {
            eprintln!("Expected '=' after variable name");
            return None;
        }
        
        let initializer = self.parse_expression()?;
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }
        
        Some(ASTStatement::variable_declaration(
            ASTVariableDeclaration::new(name, initializer, is_mutable)
        ))
    }

    /// Parses `<name> = <expr>`.
    pub fn parse_assignment(&mut self) -> Option<ASTStatement> {
        let name_token = self.consume()?;
        let name = match &name_token.kind {
            TokenKind::Identifier(n) => n.clone(),
            _ => return None,
        };

        if self.consume()?.kind != TokenKind::Equal {
            eprintln!("Expected '=' in assignment");
            return None;
        }

        let value = self.parse_expression()?;
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }
        
        Some(ASTStatement::assignment(ASTAssignment::new(name, value)))
    }

    pub fn parse_expression(&mut self) -> Option<ASTExpression> {
        return self.parse_binary_expression(0);
    }

    /// Parses a binary expression using operator-precedence climbing.
    ///
    /// Continues consuming operators whose precedence is ≥ `precedence`,
    /// recursing with a higher threshold for left-associative operators.
    pub fn parse_binary_expression(&mut self, precedence: u8) -> Option<ASTExpression> {
        let mut left: ASTExpression = self.parse_primary_expression()?;

        loop {
            // Check if next token is an operator
            let operator = self.parse_binary_operator();
            let operator_precedence = match operator.as_ref().map(|op| op.precedence()) {
                Some(op_prec) => op_prec,
                None => break,
            };
            if operator_precedence < precedence {
                break;
            }
            self.consume();
            let next_precedence = if matches!(
                operator.as_ref().map(|op| &op.kind),
                Some(ASTBinaryOperatorKind::Exponentiation)
            ) {
                operator_precedence
            } else {
                operator_precedence + 1
            };
            let right: ASTExpression = self.parse_binary_expression(next_precedence)?;
            left = ASTExpression::binary(operator.unwrap(), left, right);
        }

        return Some(left);
    }

    /// Parses a primary (non-binary) expression.
    ///
    /// Handles literals, identifiers, function calls (identifier followed by
    /// `(`), parenthesized expressions, and unary operators.
    pub fn parse_primary_expression(&mut self) -> Option<ASTExpression> {
        let token: &Token = self.current()?;
        let token_kind = token.kind.clone();
        
        match token_kind {
            TokenKind::Number(number) => {
                self.consume();
                return Some(ASTExpression::number(number));
            },
            TokenKind::Float(float) => {
                self.consume();
                return Some(ASTExpression::float(float));
            },
            TokenKind::Boolean(boolean) => {
                self.consume();
                return Some(ASTExpression::boolean(boolean));
            },
            TokenKind::String(string) => {
                self.consume();
                return Some(ASTExpression::string(string));
            },
            TokenKind::Identifier(name) => {
                self.consume();
                let mut expr = if self.current().map(|t| &t.kind) == Some(&TokenKind::LeftParen) {
                    self.consume();
                    let mut arguments = Vec::new();

                    if self.current().map(|t| &t.kind) != Some(&TokenKind::RightParen) {
                        loop {
                            let arg = self.parse_expression()?;
                            arguments.push(arg);

                            if self.current().map(|t| &t.kind) == Some(&TokenKind::Comma) {
                                self.consume();
                            } else {
                                break;
                            }
                        }
                    }

                    if self.consume()?.kind != TokenKind::RightParen {
                        panic!("Expected closing parenthesis after function arguments");
                    }

                    ASTExpression::function_call(name, arguments)
                } else {
                    ASTExpression::identifier(name)
                };

                // Check for postfix operators (++ and --)
                expr = self.parse_postfix_operators(expr)?;
                return Some(expr);
            },
            TokenKind::LeftParen => {
                self.consume();
                let expression: ASTExpression = self.parse_expression()?;
                if self.consume()?.kind != TokenKind::RightParen {
                    panic!("Expected right parenthesis");
                }
                return Some(ASTExpression::paranthesized(expression));
            },
            TokenKind::Plus | TokenKind::Minus | TokenKind::Bang | TokenKind::PlusPlus | TokenKind::MinusMinus => {
                let operator_token = self.consume()?.clone();
                let kind = match operator_token.kind {
                    TokenKind::Plus => ASTUnaryOperatorKind::Plus,
                    TokenKind::Minus => ASTUnaryOperatorKind::Minus,
                    TokenKind::Bang => ASTUnaryOperatorKind::LogicalNot,
                    TokenKind::PlusPlus => ASTUnaryOperatorKind::Increment,
                    TokenKind::MinusMinus => ASTUnaryOperatorKind::Decrement,
                    _ => unreachable!(),
                };
                let operator = ASTUnaryOperator::new(kind, operator_token);
                let operand = self.parse_primary_expression()?;
                return Some(ASTExpression::unary(operator, operand));
            },
            _ => None,
        }
    }

    /// Attempts to parse the current token as a binary operator.
    ///
    /// Returns `None` if the current token is not an operator, which signals
    /// the end of a binary expression to the precedence climber.
    pub fn parse_binary_operator(&mut self) -> Option<ASTBinaryOperator> {
        let token: &Token = self.current()?;
        let kind = match token.kind {
            TokenKind::Plus => Some(ASTBinaryOperatorKind::Plus),
            TokenKind::Minus => Some(ASTBinaryOperatorKind::Minus),
            TokenKind::Asterisk => Some(ASTBinaryOperatorKind::Multiply),
            TokenKind::Slash => Some(ASTBinaryOperatorKind::Divide),
            TokenKind::Percent => Some(ASTBinaryOperatorKind::Modulo),
            TokenKind::DoubleStar => Some(ASTBinaryOperatorKind::Exponentiation),
            TokenKind::Ampersand => Some(ASTBinaryOperatorKind::BitwiseAnd),
            TokenKind::Pipe => Some(ASTBinaryOperatorKind::BitwiseOr),
            TokenKind::Caret => Some(ASTBinaryOperatorKind::BitwiseXor),
            TokenKind::LeftShift => Some(ASTBinaryOperatorKind::LeftShift),
            TokenKind::RightShift => Some(ASTBinaryOperatorKind::RightShift),
            // Comparison operators
            TokenKind::EqualEqual => Some(ASTBinaryOperatorKind::Equal),
            TokenKind::BangEqual => Some(ASTBinaryOperatorKind::NotEqual),
            TokenKind::Less => Some(ASTBinaryOperatorKind::Less),
            TokenKind::Greater => Some(ASTBinaryOperatorKind::Greater),
            TokenKind::LessEqual => Some(ASTBinaryOperatorKind::LessEqual),
            TokenKind::GreaterEqual => Some(ASTBinaryOperatorKind::GreaterEqual),
            // Logical operators
            TokenKind::DoubleAmpersand => Some(ASTBinaryOperatorKind::LogicalAnd),
            TokenKind::DoublePipe => Some(ASTBinaryOperatorKind::LogicalOr),
            _ => None,
        };
        return kind.map(|kind| ASTBinaryOperator::new(kind, token.clone()));
    }

    /// Parses postfix operators (++ and --) that follow an expression.
    pub fn parse_postfix_operators(&mut self, expr: ASTExpression) -> Option<ASTExpression> {
        let mut result = expr;
        loop {
            match self.current().map(|t| &t.kind) {
                Some(TokenKind::PlusPlus) => {
                    let operator_token = self.consume()?.clone();
                    let operator = ASTUnaryOperator::new(ASTUnaryOperatorKind::Increment, operator_token);
                    result = ASTExpression::postfix_unary(result, operator);
                }
                Some(TokenKind::MinusMinus) => {
                    let operator_token = self.consume()?.clone();
                    let operator = ASTUnaryOperator::new(ASTUnaryOperatorKind::Decrement, operator_token);
                    result = ASTExpression::postfix_unary(result, operator);
                }
                _ => break,
            }
        }
        Some(result)
    }

    /// Returns the token at `current + offset` without advancing.
    pub fn peek(&self, offset: isize) -> Option<&Token> {
        self.tokens.get((self.current as isize + offset) as usize)
    }

    pub fn current(&self) -> Option<&Token> {
        self.peek(0)
    }

    /// Advances past the current token and returns it.
    pub fn consume(&mut self) -> Option<&Token> {
        self.current += 1;
        let token: &Token = self.peek(-1)?;
        return Some(token);
    }
}