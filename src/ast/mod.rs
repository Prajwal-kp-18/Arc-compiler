pub mod lexer;
pub mod parser;
pub mod evaluator;
pub mod types;
pub mod symbol_table;

use crate::ast::lexer::Token;
use crate::ast::types::Value;

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
            ASTStatementKind::VariableDeclaration(decl) => self.visit_variable_declaration(decl),
            ASTStatementKind::Assignment(assign) => self.visit_assignment(assign),
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
            ASTExpressionKind::Identifier(ident) => {
                self.visit_identifier(ident);
            }
            ASTExpressionKind::FunctionCall(func_call) => {
                self.visit_function_call(func_call);
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

    fn visit_identifier(&mut self, ident: &ASTIdentifierExpression) {
        let _ = ident; // Default implementation
    }

    fn visit_function_call(&mut self, func_call: &ASTFunctionCallExpression) {
        for arg in &func_call.arguments {
            self.visit_expression(arg);
        }
    }

    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        self.visit_expression(&decl.initializer);
    }

    fn visit_assignment(&mut self, assign: &ASTAssignment) {
        self.visit_expression(&assign.value);
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
        self.print_with_indent(&format!("Literal: {:?}", number.value));
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

    fn visit_identifier(&mut self, ident: &ASTIdentifierExpression) {
        self.print_with_indent(&format!("Identifier: {}", ident.name));
    }

    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        self.print_with_indent(&format!(
            "Variable Declaration: {} {} {}",
            if decl.is_mutable { "let" } else { "const" },
            decl.name,
            "="
        ));
        self.indent += LEVEL_INDENT;
        self.visit_expression(&decl.initializer);
        self.indent -= LEVEL_INDENT;
    }

    fn visit_assignment(&mut self, assign: &ASTAssignment) {
        self.print_with_indent(&format!("Assignment: {} =", assign.name));
        self.indent += LEVEL_INDENT;
        self.visit_expression(&assign.value);
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
    VariableDeclaration(ASTVariableDeclaration),
    Assignment(ASTAssignment),
}

pub struct ASTStatement {
    pub kind: ASTStatementKind,
} 

impl ASTStatement {
    pub fn new(kind: ASTStatementKind) -> Self {
        ASTStatement { kind }
    }

    pub fn expression(expr: ASTExpression) -> Self {
        ASTStatement::new(ASTStatementKind::Expression(expr))
    }

    pub fn variable_declaration(decl: ASTVariableDeclaration) -> Self {
        ASTStatement::new(ASTStatementKind::VariableDeclaration(decl))
    }

    pub fn assignment(assign: ASTAssignment) -> Self {
        ASTStatement::new(ASTStatementKind::Assignment(assign))
    }
}

pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
    Binary(ASTBinaryExpression),   
    Paranthesized(ASTParanthesizedExpression),
    Unary(ASTUnaryExpression),
    Identifier(ASTIdentifierExpression),
    FunctionCall(ASTFunctionCallExpression),
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
            ASTBinaryOperatorKind::LogicalOr => 1,
            ASTBinaryOperatorKind::LogicalAnd => 2,
            ASTBinaryOperatorKind::Equal | ASTBinaryOperatorKind::NotEqual => 3,
            ASTBinaryOperatorKind::Less | ASTBinaryOperatorKind::Greater |
            ASTBinaryOperatorKind::LessEqual | ASTBinaryOperatorKind::GreaterEqual => 4,
            ASTBinaryOperatorKind::BitwiseOr => 5,
            ASTBinaryOperatorKind::BitwiseXor => 6,
            ASTBinaryOperatorKind::BitwiseAnd => 7,
            ASTBinaryOperatorKind::LeftShift | ASTBinaryOperatorKind::RightShift => 8,
            ASTBinaryOperatorKind::Plus | ASTBinaryOperatorKind::Minus => 9,
            ASTBinaryOperatorKind::Multiply | ASTBinaryOperatorKind::Divide | ASTBinaryOperatorKind::Modulo => 10,
            ASTBinaryOperatorKind::Exponentiation => 11,
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
    // Comparison operators
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    // Logical operators
    LogicalAnd,
    LogicalOr,
}

#[derive(Debug)]
pub enum ASTUnaryOperatorKind {
    Plus,
    Minus,
    LogicalNot,
}
pub struct ASTNumberExpression {
    pub value: Value,
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

    pub fn literal(value: Value) -> Self {
        ASTExpression::new(ASTExpressionKind::Number(ASTNumberExpression { value }))
    }

    pub fn number(number: i64) -> Self {
        ASTExpression::literal(Value::Integer(number))
    }

    pub fn float(float: f64) -> Self {
        ASTExpression::literal(Value::Float(float))
    }

    pub fn boolean(boolean: bool) -> Self {
        ASTExpression::literal(Value::Boolean(boolean))
    }

    pub fn string(string: String) -> Self {
        ASTExpression::literal(Value::String(string))
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

    pub fn identifier(name: String) -> Self {
        ASTExpression::new(ASTExpressionKind::Identifier(ASTIdentifierExpression { name }))
    }

    pub fn function_call(name: String, arguments: Vec<ASTExpression>) -> Self {
        ASTExpression::new(ASTExpressionKind::FunctionCall(ASTFunctionCallExpression { name, arguments }))
    }
}

// Variable-related AST nodes
pub struct ASTVariableDeclaration {
    pub name: String,
    pub initializer: Box<ASTExpression>,
    pub is_mutable: bool, // true for 'let', false for 'const'
}

impl ASTVariableDeclaration {
    pub fn new(name: String, initializer: ASTExpression, is_mutable: bool) -> Self {
        ASTVariableDeclaration {
            name,
            initializer: Box::new(initializer),
            is_mutable,
        }
    }
}

pub struct ASTAssignment {
    pub name: String,
    pub value: Box<ASTExpression>,
}

impl ASTAssignment {
    pub fn new(name: String, value: ASTExpression) -> Self {
        ASTAssignment {
            name,
            value: Box::new(value),
        }
    }
}

pub struct ASTIdentifierExpression {
    pub name: String,
}

impl ASTIdentifierExpression {
    pub fn new(name: String) -> Self {
        ASTIdentifierExpression { name }
    }
}
pub struct ASTFunctionCallExpression {
    pub name: String,
    pub arguments: Vec<ASTExpression>,
}

impl ASTFunctionCallExpression {
    pub fn new(name: String, arguments: Vec<ASTExpression>) -> Self {
        ASTFunctionCallExpression { name, arguments }
    }
}
