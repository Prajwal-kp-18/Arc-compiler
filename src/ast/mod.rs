pub mod lexer;
pub mod parser;
pub mod evaluator;

use crate::ast::lexer::Token;

pub struct Ast {
    pub statements: Vec<ASTStatement>,
}

impl Ast {
    pub fn new() -> Self {
        Ast {
            statements: Vec::new(),
        }
    }

    pub fn add_statement(&mut self, statement: ASTStatement) {
        self.statements.push(statement);
    }
    
    pub fn visit(&self, visitor: &mut dyn ASTVisitor) {
        for statement in &self.statements {
            visitor.visit_statement(statement);
        }
    }

    pub fn visualize(&self) -> () {
        let mut printor = ASTPrintor { indent: 0 };
        self.visit(&mut printor);
    }
}

pub trait ASTVisitor {
    fn do_visit_statement(&mut self, statement: &ASTStatement) {
         match &statement.kind {
            ASTStatementKind::Expression(expr) => self.visit_expression(expr),
        }
    }
    fn visit_statement(&mut self, statement: &ASTStatement){
        self.do_visit_statement(statement);
    }
    fn do_visit_expression(&mut self, expression: &ASTExpression) {
        match &expression.kind {
            ASTExpressionKind::Number(number) => self.visit_number(number),
            // doubt just added below statement to remove error
            ASTExpressionKind::Binary(expr) => {
                self.visit_binary_expression(expr);
            }
            ASTExpressionKind::Paranthesized(paren_expr) => {
                self.visit_parenthesized_expression(paren_expr);
            }
            ASTExpressionKind::Unary(unary_expr) => {
                self.visit_unary_expression(unary_expr);
            }
        }
    }
    fn visit_expression(&mut self, expression: &ASTExpression){
        self.do_visit_expression(expression);
    }

    fn visit_number(&mut self, number: &ASTNumberExpression);

    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression){
        self.do_visit_expression(&expr.left);
        self.do_visit_expression(&expr.right);
    }
    fn visit_parenthesized_expression(&mut self, paren_expr: &ASTParanthesizedExpression) {
        self.visit_expression(&paren_expr.expression);
    }

    fn visit_unary_expression(&mut self, unary_expr: &ASTUnaryExpression) {
        self.do_visit_expression(&unary_expr.operand);
    }
}

pub struct ASTPrintor{
    indent: usize,
}
const LEVEL_INDENT: usize = 2;

impl ASTVisitor for ASTPrintor {

    fn visit_statement(&mut self, statement: &ASTStatement) {
        self.print_with_indent("Statement");
        self.indent +=LEVEL_INDENT;
        ASTVisitor::do_visit_statement(self, statement);
        self.indent -=LEVEL_INDENT;
    }

    fn visit_expression(&mut self, expression: &ASTExpression) {
        self.print_with_indent("Expression");
        self.indent +=LEVEL_INDENT;
        ASTVisitor::do_visit_expression(self, expression);
        self.indent -=LEVEL_INDENT;
    }

    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.print_with_indent(&format!("Number: {}", number.number));
    }

    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression) {
        self.print_with_indent("Binary Expression");
        self.indent += LEVEL_INDENT;
        self.print_with_indent(&format!("Operator: {:?}", expr.operator.kind));
        self.visit_expression(&expr.left);
        self.visit_expression(&expr.right);
        self.indent -= LEVEL_INDENT;
    }

    fn visit_parenthesized_expression(&mut self, paren_expr: &ASTParanthesizedExpression) {
        self.print_with_indent("Parenthesized Expression");
        self.indent += LEVEL_INDENT;
        self.visit_expression(&paren_expr.expression);
        self.indent -= LEVEL_INDENT;
    }

    fn visit_unary_expression(&mut self, unary_expr: &ASTUnaryExpression) {
        self.print_with_indent("Unary Expression");
        self.indent += LEVEL_INDENT;
        self.print_with_indent(&format!("Operator: {:?}", unary_expr.operator.kind));
        self.visit_expression(&unary_expr.operand);
        self.indent -= LEVEL_INDENT;
    }


}

impl ASTPrintor {
    fn print_with_indent(&self, text: &str) {
        println!("{}{}", " ".repeat(self.indent), text);
    }
}

pub enum  ASTStatementKind{
    Expression(ASTExpression),
}

pub struct ASTStatement {
    kind: ASTStatementKind,
} 

impl ASTStatement {
    pub fn new(kind: ASTStatementKind) -> Self {
        ASTStatement { kind }
    }

    pub fn expression(expr: ASTExpression) -> Self {
        ASTStatement::new(ASTStatementKind::Expression(expr))
    }
}

pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
    Binary(ASTBinaryExpression),   
    Paranthesized(ASTParanthesizedExpression),
    Unary(ASTUnaryExpression),
}

pub struct ASTBinaryExpression {
    left: Box<ASTExpression>,
    operator: ASTBinaryOperator,
    right: Box<ASTExpression>,
}

pub struct ASTBinaryOperator {
    pub kind: ASTBinaryOperatorKind,
    pub token: Token,
}

impl ASTBinaryOperator {
    pub fn new(kind: ASTBinaryOperatorKind, token: Token) -> Self {
        ASTBinaryOperator { kind, token }
    }

    pub fn precedence(&self) -> u8 {
        match self.kind {
            ASTBinaryOperatorKind::BitwiseOr => 1,
            ASTBinaryOperatorKind::BitwiseXor => 2,
            ASTBinaryOperatorKind::BitwiseAnd => 3,
            ASTBinaryOperatorKind::LeftShift | ASTBinaryOperatorKind::RightShift => 4,
            ASTBinaryOperatorKind::Plus | ASTBinaryOperatorKind::Minus => 5,
            ASTBinaryOperatorKind::Multiply | ASTBinaryOperatorKind::Divide | ASTBinaryOperatorKind::Modulo => 6,
            ASTBinaryOperatorKind::Exponentiation => 7,
        }
    }
}
#[derive(Debug)]
pub enum ASTBinaryOperatorKind {
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Exponentiation,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LeftShift,
    RightShift,
}

#[derive(Debug)]
pub enum ASTUnaryOperatorKind {
    Plus,
    Minus,
}
pub struct ASTNumberExpression {
    number: i64,
}

pub struct ASTParanthesizedExpression {
    expression: Box<ASTExpression>,
}

pub struct ASTUnaryExpression {
    operator: ASTUnaryOperator,
    operand: Box<ASTExpression>,
}

pub struct ASTUnaryOperator {
    pub kind: ASTUnaryOperatorKind,
    pub token: Token,
}

impl ASTUnaryOperator {
    pub fn new(kind: ASTUnaryOperatorKind, token: Token) -> Self {
        ASTUnaryOperator { kind, token }
    }
}

pub struct ASTExpression {
    kind: ASTExpressionKind,
}

impl ASTExpression {
    pub fn new(kind: ASTExpressionKind) -> Self {
        ASTExpression { kind }
    }

    pub fn number(number: i64) -> Self {
        ASTExpression::new(ASTExpressionKind::Number(ASTNumberExpression { number }))
    }

    pub fn binary(operator: ASTBinaryOperator, left: ASTExpression, right: ASTExpression) -> Self {
        ASTExpression::new(ASTExpressionKind::Binary(ASTBinaryExpression { left: Box::new(left), operator, right: Box::new(right) }))
    }

    pub fn paranthesized(expression: ASTExpression) -> Self {
        ASTExpression::new(ASTExpressionKind::Paranthesized(ASTParanthesizedExpression { expression: Box::new(expression) }))
    }

    pub fn unary(operator: ASTUnaryOperator, operand: ASTExpression) -> Self {
        ASTExpression::new(ASTExpressionKind::Unary(ASTUnaryExpression { operator, operand: Box::new(operand) }))
    }
}