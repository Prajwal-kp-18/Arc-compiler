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
//! - `fn` → function declaration
//! - `return` → return statement
//! - `identifier =` → assignment
//! - anything else → expression statement

use crate::ast::lexer::Token;
use crate::ast::ASTBinaryOperator;
use crate::ast::ASTBinaryOperatorKind;
use crate::ast::ASTUnaryOperator;
use crate::ast::ASTUnaryOperatorKind;
use crate::ast::{ASTStatement, ASTExpression, ASTVariableDeclaration, ASTAssignment, ASTIfStatement, ASTFunctionDeclaration, ASTReturnStatement};
use crate::ast::lexer::TokenKind;
use crate::ast::diagnostic::{Diagnostic, DiagnosticKind};

/// Parses a token stream into an AST.
///
/// Whitespace tokens are stripped on construction. Call
/// [`next_statement`](Parser::next_statement) in a loop until it returns
/// `None` (EOF) to obtain all statements.
pub struct Parser {
    tokens: Vec<crate::ast::lexer::Token>,
    current: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parser {
    /// Creates a parser from a token list, filtering out whitespace.
    pub fn new(
        tokens: Vec<Token>,
    ) -> Self {
        Parser {
            tokens: tokens.iter().filter(|token| token.kind != TokenKind::Whitespace).cloned().collect(),
            current: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn from_tokens(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            current: 0,
            diagnostics: Vec::new(),
        }
    }

    fn emit_error(&mut self, message: impl Into<String>, span: Option<crate::ast::lexer::TextSpan>) {
        self.diagnostics.push(Diagnostic::new(DiagnosticKind::ParseError, message, span));
    }

    fn emit_error_with_suggestion(
        &mut self,
        message: impl Into<String>,
        span: Option<crate::ast::lexer::TextSpan>,
        suggestion: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::new(DiagnosticKind::ParseError, message, span)
                .with_suggestion(suggestion),
        );
    }

    

    /// Returns the next parsed statement, or `None` at EOF.
    ///
    /// If a statement fails to parse, synchronizes to the next statement
    /// boundary and keeps going, so a single bad statement doesn't hide
    /// diagnostics for the rest of the file.
    pub fn next_statement(&mut self) -> Option<ASTStatement>{
        self.parse_statement_recovering()
    }

    fn parse_statement_recovering(&mut self) -> Option<ASTStatement> {
        loop {
            match self.current().map(|t| &t.kind) {
                Some(TokenKind::EOF) | None => return None,
                _ => {}
            }

            let start = self.current;
            if let Some(stmt) = self.parse_statement() {
                return Some(stmt);
            }

            match self.current().map(|t| &t.kind) {
                Some(TokenKind::EOF) | None => return None,
                _ => {}
            }

            self.synchronize();
            if self.current == start {
                // Nothing was consumed (e.g. a stray closing brace); force
                // progress so we can't spin forever on the same token.
                self.consume();
            }
        }
    }

    /// Skips tokens until a likely statement boundary: past a `;`, or right
    /// before `}` / a statement-starting keyword.
    fn synchronize(&mut self) {
        while let Some(token) = self.current() {
            match token.kind {
                TokenKind::EOF => return,
                TokenKind::Semicolon => {
                    self.consume();
                    return;
                }
                TokenKind::RightBrace
                | TokenKind::Let
                | TokenKind::Const
                | TokenKind::Fn
                | TokenKind::Return
                | TokenKind::IF
                | TokenKind::LeftBrace => return,
                _ => {
                    self.consume();
                }
            }
        }
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

        if token.kind == TokenKind::Fn {
            return self.parse_function_declaration();
        }

        if token.kind == TokenKind::Return {
            return self.parse_return_statement();
        }

        // Check for if statement
        if token.kind == TokenKind::IF {
            return self.parse_if_statement();
        }

        // Check for bare block statement `{ ... }`
        if token.kind == TokenKind::LeftBrace {
            let block = self.parse_block()?;
            return Some(crate::ast::ASTStatement::block(block));
        }

        // Check for assignment - needs lookahead to distinguish from identifier expression
        if let TokenKind::Identifier(_) = token.kind {
            if self.peek(1).map(|t| &t.kind) == Some(&TokenKind::Equal) {
                return self.parse_assignment();
            }
        }
        
        // Otherwise, parse as expression statement
        let expr = self.parse_expression().or_else(|| {
            let span = self.current().map(|t| t.span.clone());
            self.emit_error("Expected a statement", span);
            None
        })?;
        
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

    /// Parses `fn name(param1, param2) { ... }`.
    pub fn parse_function_declaration(&mut self) -> Option<ASTStatement> {
        self.consume(); // consume `fn`

        let name_token = self.consume()?.clone();
        let name = match &name_token.kind {
            TokenKind::Identifier(identifier) => identifier.clone(),
            _ => {
                self.emit_error_with_suggestion(
                    "Expected function name after 'fn'",
                    Some(name_token.span.clone()),
                    "Declare a name like: fn my_function() { ... }",
                );
                return None;
            }
        };

        if self.consume()?.kind != TokenKind::LeftParen {
            let span = self.peek(-1).map(|t| t.span.clone());
            self.emit_error("Expected '(' after function name", span);
            return None;
        }

        let mut parameters = Vec::new();
        if self.current().map(|t| &t.kind) != Some(&TokenKind::RightParen) {
            loop {
                let parameter_token = self.consume()?.clone();
                let param_name = match &parameter_token.kind {
                    TokenKind::Identifier(identifier) => identifier.clone(),
                    _ => {
                        self.emit_error("Expected parameter name in function declaration", Some(parameter_token.span.clone()));
                        return None;
                    }
                };

                let type_annotation = self.parse_optional_type_annotation()?;
                parameters.push(crate::ast::Param::new(param_name, type_annotation));

                if self.current().map(|t| &t.kind) == Some(&TokenKind::Comma) {
                    self.consume();
                    continue;
                }
                break;
            }
        }

        if self.consume()?.kind != TokenKind::RightParen {
            let span = self.peek(-1).map(|t| t.span.clone());
            self.emit_error("Expected ')' after function parameters", span);
            return None;
        }

        let body = self.parse_block()?;
        Some(ASTStatement::function_declaration(ASTFunctionDeclaration::new(name, parameters, body)))
    }

    /// Parses an optional `: TypeName` annotation on a parameter, where
    /// `TypeName` is a soft keyword (`Int`, `Float`, `Bool`, `String`) —
    /// not a reserved lexer token, just a plain identifier the parser
    /// recognizes here. Returns `Some(None)` if there's no `:` at all.
    fn parse_optional_type_annotation(&mut self) -> Option<Option<crate::ast::types::DataType>> {
        if self.current().map(|t| &t.kind) != Some(&TokenKind::Colon) {
            return Some(None);
        }
        self.consume(); // consume ':'

        let type_token = self.consume()?.clone();
        let type_name = match &type_token.kind {
            TokenKind::Identifier(name) => name.clone(),
            _ => {
                self.emit_error("Expected a type name after ':'", Some(type_token.span.clone()));
                return None;
            }
        };

        use crate::ast::types::DataType;
        match type_name.as_str() {
            "Int" => Some(Some(DataType::Integer)),
            "Float" => Some(Some(DataType::Float)),
            "Bool" => Some(Some(DataType::Boolean)),
            "String" => Some(Some(DataType::String)),
            _ => {
                self.emit_error_with_suggestion(
                    format!("Unknown type annotation '{}'", type_name),
                    Some(type_token.span.clone()),
                    "Expected one of: Int, Float, Bool, String",
                );
                None
            }
        }
    }

    /// Parses `return <expr>`.
    pub fn parse_return_statement(&mut self) -> Option<ASTStatement> {
        self.consume(); // consume `return`

        let value = self.parse_expression()?;
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }

        Some(ASTStatement::return_statement(ASTReturnStatement::new(value)))
    }

    pub fn parse_block(&mut self) -> Option<Vec<ASTStatement>> {
        let left_brace = self.consume()?.clone();
        if left_brace.kind != TokenKind::LeftBrace {
            self.emit_error("Expected '{' to start block", Some(left_brace.span.clone()));
            return None;
        }

        let mut statements = Vec::new();

        while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBrace) {
            if matches!(self.current().map(|t| &t.kind), None | Some(TokenKind::EOF)) {
                self.emit_error_with_suggestion(
                    "Unclosed block",
                    Some(left_brace.span.clone()),
                    "Add a closing '}' for this block",
                );
                return None;
            }

            let stmt = self.parse_statement_recovering()?;
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
        
        let name_token = self.consume()?.clone();
        let name = match name_token.kind {
            TokenKind::Identifier(ref n) => n.clone(),
            _ => {
                self.emit_error(
                    format!("Expected identifier after '{}' keyword", if is_mutable { "let" } else { "const" }),
                    Some(name_token.span.clone()),
                );
                return None;
            }
        };
        
        let equal_token = self.consume()?.clone();
        if equal_token.kind != TokenKind::Equal {
            self.emit_error("Expected '=' after variable name", Some(equal_token.span.clone()));
            return None;
        }
        
        let initializer = self.parse_expression()?;
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }
        
        Some(ASTStatement::variable_declaration(
            ASTVariableDeclaration::new(name, initializer, is_mutable, Some(name_token))
        ))
    }

    /// Parses `<name> = <expr>`.
    pub fn parse_assignment(&mut self) -> Option<ASTStatement> {
        let name_token = self.consume()?.clone();
        let name = match &name_token.kind {
            TokenKind::Identifier(n) => n.clone(),
            _ => return None,
        };

        let equal_token = self.consume()?.clone();
        if equal_token.kind != TokenKind::Equal {
            self.emit_error("Expected '=' in assignment", Some(equal_token.span.clone()));
            return None;
        }

        let value = self.parse_expression()?;
        if self.current().map(|t| &t.kind) == Some(&TokenKind::Semicolon) {
            self.consume();
        }
        
        Some(ASTStatement::assignment(ASTAssignment::new(name, value, Some(name_token))))
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
                let ident_token = self.consume()?.clone();
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
                        self.emit_error(
                            "Expected closing parenthesis after function arguments",
                            self.peek(-1).map(|t| t.span.clone()),
                        );
                        return None;
                    }

                    ASTExpression::function_call(name, arguments, Some(ident_token.clone()))
                } else {
                    ASTExpression::identifier(name, Some(ident_token.clone()))
                };

                // Check for postfix operators (++ and --)
                expr = self.parse_postfix_operators(expr)?;
                return Some(expr);
            },
            TokenKind::LeftParen => {
                self.consume();
                let expression: ASTExpression = self.parse_expression()?;
                if self.consume()?.kind != TokenKind::RightParen {
                    self.emit_error("Expected right parenthesis", self.peek(-1).map(|t| t.span.clone()));
                    return None;
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
            _ => {
                self.emit_error("Expected expression", Some(token.span.clone()));
                None
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::lexer::Lexer;
    use crate::ast::{ASTExpressionKind, ASTStatementKind};

    /// Parses every statement in `source`, returning the parsed statements
    /// and any diagnostics collected along the way.
    fn parse_all(source: &str) -> (Vec<ASTStatement>, Vec<Diagnostic>) {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens);
        let mut statements = Vec::new();
        while let Some(stmt) = parser.next_statement() {
            statements.push(stmt);
        }
        (statements, parser.diagnostics)
    }

    #[test]
    fn test_variable_declaration() {
        let (statements, diagnostics) = parse_all("let x = 42;");
        assert!(diagnostics.is_empty());
        assert_eq!(statements.len(), 1);
        match &statements[0].kind {
            ASTStatementKind::VariableDeclaration(decl) => {
                assert_eq!(decl.name, "x");
                assert!(decl.is_mutable);
            }
            _ => panic!("expected a variable declaration"),
        }
    }

    #[test]
    fn test_binary_expression_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3): the top-level operator is `+`.
        let (statements, diagnostics) = parse_all("1 + 2 * 3;");
        assert!(diagnostics.is_empty());
        match &statements[0].kind {
            ASTStatementKind::Expression(expr) => match &expr.kind {
                ASTExpressionKind::Binary(bin) => {
                    assert!(matches!(bin.operator.kind, ASTBinaryOperatorKind::Plus));
                }
                _ => panic!("expected a binary expression"),
            },
            _ => panic!("expected an expression statement"),
        }
    }

    #[test]
    fn test_function_declaration_and_if_else() {
        let (statements, diagnostics) = parse_all(
            "fn max2(a, b) { if a > b { return a; } else { return b; } }",
        );
        assert!(diagnostics.is_empty());
        match &statements[0].kind {
            ASTStatementKind::FunctionDeclaration(func) => {
                assert_eq!(func.name, "max2");
                let param_names: Vec<&str> = func.parameters.iter().map(|p| p.name.as_str()).collect();
                assert_eq!(param_names, vec!["a", "b"]);
                assert!(func.parameters.iter().all(|p| p.type_annotation.is_none()));
                assert_eq!(func.body.len(), 1);
                assert!(matches!(func.body[0].kind, ASTStatementKind::IfStatement(_)));
            }
            _ => panic!("expected a function declaration"),
        }
    }

    #[test]
    fn test_typed_parameters() {
        let (statements, diagnostics) = parse_all("fn add(a: Int, b: Int) { return a + b; }");
        assert!(diagnostics.is_empty());
        match &statements[0].kind {
            ASTStatementKind::FunctionDeclaration(func) => {
                let types: Vec<_> = func.parameters.iter().map(|p| p.type_annotation).collect();
                assert_eq!(types, vec![Some(crate::ast::types::DataType::Integer), Some(crate::ast::types::DataType::Integer)]);
            }
            _ => panic!("expected a function declaration"),
        }
    }

    #[test]
    fn test_unknown_type_annotation_is_a_parse_error() {
        let (_, diagnostics) = parse_all("fn f(a: NotAType) { return a; }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Unknown type annotation"));
    }

    #[test]
    fn test_error_recovery_reports_every_bad_statement() {
        // Two independently broken statements, each surrounded by valid
        // ones. Recovery should surface both diagnostics, not just the first.
        let (statements, diagnostics) = parse_all(
            "let x = 1;\nlet = 2;\nlet y = 3;\nconst = 4;\nlet z = 5;",
        );
        assert_eq!(diagnostics.len(), 2);
        // The three well-formed declarations should still come through.
        let names: Vec<&str> = statements
            .iter()
            .filter_map(|s| match &s.kind {
                ASTStatementKind::VariableDeclaration(decl) => Some(decl.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_unclosed_block_reports_diagnostic() {
        let (_, diagnostics) = parse_all("fn f() { let x = 1;");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Unclosed block"));
    }
}