use crate::ast::lexer::Token;
use crate::ast::ASTBinaryOperator;
use crate::ast::ASTBinaryOperatorKind;
use crate::ast::ASTUnaryOperator;
use crate::ast::ASTUnaryOperatorKind;
use crate::ast::{ASTStatement, ASTExpression};
use crate::ast::lexer::TokenKind;
pub struct Parser {
    tokens: Vec<crate::ast::lexer::Token>,
    current: usize,
}

impl Parser {
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

    

    pub fn next_statement(&mut self) -> Option<ASTStatement>{
        return self.parse_statement();
    }

    pub fn parse_statement(&mut self) -> Option<ASTStatement> {
        let token: &Token = self.current()?;
        if token.kind == TokenKind::EOF {
            return None;
        }
        let expr = self.parse_expression()?;
        return Some(ASTStatement::expression(expr));
    }

    pub fn parse_expression(&mut self) -> Option<ASTExpression> {
        return self.parse_binary_expression(0);
    }

    pub fn parse_binary_expression(&mut self, precedence: u8) -> Option<ASTExpression> {
        let mut left: ASTExpression = self.parse_primary_expression()?;

        loop {
            let operator = self.parse_binary_operator();
            let operator_precedence = match operator.as_ref().map(|op| op.precedence()) {
                Some(op_prec) => op_prec,
                None => break,
            };
            if operator_precedence < precedence {
                break;
            }
            self.consume(); // only consume if we have a valid operator
            let right: ASTExpression = self.parse_binary_expression(operator_precedence)?;
            left = ASTExpression::binary(operator.unwrap(), left, right);
        }

        return Some(left);
    }

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
            TokenKind::LeftParen => {
                self.consume();
                let expression: ASTExpression = self.parse_expression()?;
                if self.consume()?.kind != TokenKind::RightParen {
                    panic!("Expected right parenthesis");
                }
                return Some(ASTExpression::paranthesized(expression));
            },
            TokenKind::Plus | TokenKind::Minus => {
                let operator_token = self.consume()?.clone();
                let kind = match operator_token.kind {
                    TokenKind::Plus => ASTUnaryOperatorKind::Plus,
                    TokenKind::Minus => ASTUnaryOperatorKind::Minus,
                    _ => unreachable!(),
                };
                let operator = ASTUnaryOperator::new(kind, operator_token);
                let operand = self.parse_primary_expression()?;
                return Some(ASTExpression::unary(operator, operand));
            },
            _ => None,
        }
    }

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
            _ => None,
        };
        return kind.map(|kind| ASTBinaryOperator::new(kind, token.clone()));
    }

    pub fn peek(&self, offset: isize) -> Option<&Token> {
        self.tokens.get((self.current as isize + offset) as usize)
    }

    pub fn current(&self) -> Option<&Token> {
        self.peek(0)
    }

    pub fn consume(&mut self) -> Option<&Token> {
        self.current += 1;
        let token: &Token = self.peek(-1)?;
        return Some(token);
    }
}