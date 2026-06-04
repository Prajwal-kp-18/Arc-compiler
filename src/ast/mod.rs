//! # AST Module
//!
//! Defines the Abstract Syntax Tree node types and the [`ASTVisitor`] trait
//! used by the evaluator and printer.
//!
//! ## Structure
//!
//! An [`Ast`] holds a list of [`ASTStatement`]s. Each statement is either:
//! - An expression (evaluated for its value)
//! - A variable declaration (`let` / `const`)
//! - An assignment (`x = expr`)
//!
//! Expressions are represented as [`ASTExpression`] nodes, which may be
//! literals, identifiers, binary/unary operations, parenthesized groups,
//! or function calls.
//!
//! ## Visitor Pattern
//!
//! Implement [`ASTVisitor`] to traverse the tree. The evaluator and
//! [`ASTPrintor`] both use this pattern.

pub mod lexer;
pub mod parser;
pub mod evaluator;
pub mod types;
pub mod symbol_table;

use crate::ast::lexer::Token;
use crate::ast::types::Value;

/// The root of a parsed Arc program, containing an ordered list of statements.
pub struct Ast {
    pub statements: Vec<ASTStatement>,
}

impl Ast {
    /// Creates an empty AST.
    pub fn new() -> Self {
        Ast {
            statements: Vec::new(),
        }
    }

    /// Appends a statement to the AST.
    pub fn add_statement(&mut self, statement: ASTStatement) {
        self.statements.push(statement);
    }
    
    /// Walks every statement in order, calling the appropriate visitor methods.
    pub fn visit(&self, visitor: &mut dyn ASTVisitor) {
        for statement in &self.statements {
            visitor.visit_statement(statement);
        }
    }

    /// Pretty-prints the AST structure to stdout for debugging.
    pub fn visualize(&self) -> () {
        let mut printor = ASTPrintor { indent: 0 };
        self.visit(&mut printor);
    }
}

/// Visitor trait for traversing an Arc AST.
///
/// Override only the methods you need; default implementations walk
/// child nodes so the tree is fully traversed even for unoverridden node types.
pub trait ASTVisitor {
    fn do_visit_statement(&mut self, statement: &ASTStatement) {
         match &statement.kind {
            ASTStatementKind::Expression(expr) => self.visit_expression(expr),
            ASTStatementKind::VariableDeclaration(decl) => self.visit_variable_declaration(decl),
            ASTStatementKind::Assignment(assign) => self.visit_assignment(assign),
            ASTStatementKind::IfStatement(if_stmt) => self.visit_if_statement(if_stmt),
            ASTStatementKind::FunctionDeclaration(func) => self.visit_function_declaration(func),
            ASTStatementKind::ReturnStatement(ret) => self.visit_return_statement(ret),
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
            ASTExpressionKind::PostfixUnary(postfix_expr) => {
                self.visit_postfix_unary_expression(postfix_expr);
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

    /// Called for every numeric or boolean literal node.
    fn visit_number(&mut self, number: &ASTNumberExpression);

    /// Called for binary expressions (`left op right`).
    ///
    /// The default implementation visits both operands.
    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression){
        self.do_visit_expression(&expr.left);
        self.do_visit_expression(&expr.right);
    }

    /// Called for parenthesized expressions. Default visits the inner expression.
    fn visit_parenthesized_expression(&mut self, paren_expr: &ASTParanthesizedExpression) {
        self.visit_expression(&paren_expr.expression);
    }

    /// Called for unary expressions (`-x`, `!x`). Default visits the operand.
    fn visit_unary_expression(&mut self, unary_expr: &ASTUnaryExpression) {
        self.do_visit_expression(&unary_expr.operand);
    }

    /// Called for postfix unary expressions (`x++`, `x--`). Default visits the operand.
    fn visit_postfix_unary_expression(&mut self, postfix_expr: &ASTPostfixUnaryExpression) {
        self.do_visit_expression(&postfix_expr.operand);
    }

    /// Called for identifier references. Default is a no-op.
    fn visit_identifier(&mut self, ident: &ASTIdentifierExpression) {
        let _ = ident; // Default implementation
    }

    /// Called for function calls. Default visits each argument expression.
    fn visit_function_call(&mut self, func_call: &ASTFunctionCallExpression) {
        for arg in &func_call.arguments {
            self.visit_expression(arg);
        }
    }

    /// Called for `let` / `const` declarations. Default visits the initializer.
    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        self.visit_expression(&decl.initializer);
    }

    /// Called for function declarations. Default visits the body statements.
    fn visit_function_declaration(&mut self, func: &ASTFunctionDeclaration) {
        for stmt in &func.body {
            self.visit_statement(stmt);
        }
    }

    /// Called for return statements. Default visits the returned expression.
    fn visit_return_statement(&mut self, ret: &ASTReturnStatement) {
        self.visit_expression(&ret.value);
    }

    /// Called for assignment statements. Default visits the right-hand side.
    fn visit_assignment(&mut self, assign: &ASTAssignment) {
        self.visit_expression(&assign.value);
    }

    fn visit_if_statement(&mut self, if_stmt: &ASTIfStatement) {
        self.visit_expression(&if_stmt.condition);
        for stmt in &if_stmt.then_branch {
            self.visit_statement(stmt);
        }
        if let Some(else_branch) = &if_stmt.else_branch {
            for stmt in else_branch {
                self.visit_statement(stmt);
            }
        }
    }
}

/// An [`ASTVisitor`] that pretty-prints the tree structure to stdout.
///
/// Used by [`Ast::visualize`].
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

    fn visit_postfix_unary_expression(&mut self, postfix_expr: &ASTPostfixUnaryExpression) {
        self.print_with_indent("Postfix Unary Expression");
        self.indent += LEVEL_INDENT;
        self.visit_expression(&postfix_expr.operand);
        self.print_with_indent(&format!("Operator: {:?}", postfix_expr.operator.kind));
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

// ---------------------------------------------------------------------------
// Statement types
// ---------------------------------------------------------------------------
 
/// Discriminates between the statement forms in Arc.
#[derive(Clone)]
pub enum  ASTStatementKind{
    Expression(ASTExpression),
    VariableDeclaration(ASTVariableDeclaration),
    Assignment(ASTAssignment),
    IfStatement(ASTIfStatement),
    FunctionDeclaration(ASTFunctionDeclaration),
    ReturnStatement(ASTReturnStatement),
}

/// A top-level statement node.
#[derive(Clone)]
pub struct ASTStatement {
    pub kind: ASTStatementKind,
} 

impl ASTStatement {
    pub fn new(kind: ASTStatementKind) -> Self {
        ASTStatement { kind }
    }

    /// Wraps an expression as a statement.
    pub fn expression(expr: ASTExpression) -> Self {
        ASTStatement::new(ASTStatementKind::Expression(expr))
    }

    /// Wraps a variable declaration as a statement.
    pub fn variable_declaration(decl: ASTVariableDeclaration) -> Self {
        ASTStatement::new(ASTStatementKind::VariableDeclaration(decl))
    }

    /// Wraps an assignment as a statement.
    pub fn assignment(assign: ASTAssignment) -> Self {
        ASTStatement::new(ASTStatementKind::Assignment(assign))
    }

    /// Wraps an if statement as a statement.
    pub fn if_statement(if_stmt: ASTIfStatement) -> Self {
        ASTStatement::new(ASTStatementKind::IfStatement(if_stmt))
    }

    /// Wraps a function declaration as a statement.
    pub fn function_declaration(func: ASTFunctionDeclaration) -> Self {
        ASTStatement::new(ASTStatementKind::FunctionDeclaration(func))
    }

    /// Wraps a return statement as a statement.
    pub fn return_statement(ret: ASTReturnStatement) -> Self {
        ASTStatement::new(ASTStatementKind::ReturnStatement(ret))
    }
}

/// An `if` statement with optional `else`.
#[derive(Clone)]
pub struct ASTIfStatement {
    pub condition: Box<ASTExpression>,
    pub then_branch: Vec<ASTStatement>,
    pub else_branch: Option<Vec<ASTStatement>>,
}

impl ASTIfStatement {
    pub fn new(
        condition: ASTExpression,
        then_branch: Vec<ASTStatement>,
        else_branch: Option<Vec<ASTStatement>>,
    ) -> Self {
        Self {
            condition: Box::new(condition),
            then_branch,
            else_branch,
        }
    }
}

/// A function declaration statement.
#[derive(Clone)]
pub struct ASTFunctionDeclaration {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: Vec<ASTStatement>,
}

impl ASTFunctionDeclaration {
    pub fn new(name: String, parameters: Vec<String>, body: Vec<ASTStatement>) -> Self {
        Self { name, parameters, body }
    }
}

/// A return statement.
#[derive(Clone)]
pub struct ASTReturnStatement {
    pub value: Box<ASTExpression>,
}

impl ASTReturnStatement {
    pub fn new(value: ASTExpression) -> Self {
        Self { value: Box::new(value) }
    }
}

// ---------------------------------------------------------------------------
// Expression types
// ---------------------------------------------------------------------------
 
/// Discriminates between the expression node types in Arc.
#[derive(Clone)]
pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
    Binary(ASTBinaryExpression),   
    Paranthesized(ASTParanthesizedExpression),
    Unary(ASTUnaryExpression),
    PostfixUnary(ASTPostfixUnaryExpression),
    Identifier(ASTIdentifierExpression),
    FunctionCall(ASTFunctionCallExpression),
}

/// A binary expression node: `left operator right`.
#[derive(Clone)]
pub struct ASTBinaryExpression {
    left: Box<ASTExpression>,
    operator: ASTBinaryOperator,
    right: Box<ASTExpression>,
}

/// A binary operator with its kind and the originating token.
#[derive(Clone)]
pub struct ASTBinaryOperator {
    pub kind: ASTBinaryOperatorKind,
    pub token: Token,
}

impl ASTBinaryOperator {
    pub fn new(kind: ASTBinaryOperatorKind, token: Token) -> Self {
        ASTBinaryOperator { kind, token }
    }

    /// Returns the operator's precedence level (1 = lowest, 11 = highest).
    ///
    /// Used by the precedence-climbing parser to determine association.
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

/// All binary operators supported by Arc.
#[derive(Debug, Clone)]
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

/// Unary operator kinds supported by Arc.
#[derive(Debug, Clone)]
pub enum ASTUnaryOperatorKind {
    Plus,
    Minus,
    LogicalNot,
    Increment,
    Decrement,
}

/// A literal value node (integer, float, boolean, or string).
#[derive(Clone)]
pub struct ASTNumberExpression {
    pub value: Value,
}

/// A parenthesized sub-expression: `( expr )`.
#[derive(Clone)]
pub struct ASTParanthesizedExpression {
    expression: Box<ASTExpression>,
}

/// A unary expression: `operator operand`.
#[derive(Clone)]
pub struct ASTUnaryExpression {
    operator: ASTUnaryOperator,
    operand: Box<ASTExpression>,
}

/// A postfix unary expression: `operand operator` (for ++ and --).
#[derive(Clone)]
pub struct ASTPostfixUnaryExpression {
    operand: Box<ASTExpression>,
    operator: ASTUnaryOperator,
}

#[derive(Clone)]
pub struct ASTUnaryOperator {
    pub kind: ASTUnaryOperatorKind,
    pub token: Token,
}

impl ASTUnaryOperator {
    pub fn new(kind: ASTUnaryOperatorKind, token: Token) -> Self {
        ASTUnaryOperator { kind, token }
    }
}


/// An expression node.
#[derive(Clone)]
pub struct ASTExpression {
    kind: ASTExpressionKind,
}

impl ASTExpression {
    pub fn new(kind: ASTExpressionKind) -> Self {
        ASTExpression { kind }
    }

    /// Creates a literal expression from a [`Value`].
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

    pub fn postfix_unary(operand: ASTExpression, operator: ASTUnaryOperator) -> Self {
        ASTExpression::new(ASTExpressionKind::PostfixUnary(ASTPostfixUnaryExpression { operand: Box::new(operand), operator }))
    }

    pub fn identifier(name: String) -> Self {
        ASTExpression::new(ASTExpressionKind::Identifier(ASTIdentifierExpression { name }))
    }

    pub fn function_call(name: String, arguments: Vec<ASTExpression>) -> Self {
        ASTExpression::new(ASTExpressionKind::FunctionCall(ASTFunctionCallExpression { name, arguments }))
    }
}

// ---------------------------------------------------------------------------
// Variable-related AST nodes
// ---------------------------------------------------------------------------
 
/// A `let` or `const` variable declaration.
#[derive(Clone)]
pub struct ASTVariableDeclaration {
    /// The variable name.
    pub name: String,
    /// The expression assigned at declaration time.
    pub initializer: Box<ASTExpression>,
    /// `true` for `let` (mutable), `false` for `const` (immutable).
    pub is_mutable: bool,
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

/// An assignment statement: `name = value`.
#[derive(Clone)]
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

/// A reference to a named variable.
#[derive(Clone)]
pub struct ASTIdentifierExpression {
    pub name: String,
}

impl ASTIdentifierExpression {
    pub fn new(name: String) -> Self {
        ASTIdentifierExpression { name }
    }
}

/// A function call expression: `name(arg1, arg2, ...)`.
#[derive(Clone)]
pub struct ASTFunctionCallExpression {
    pub name: String,
    pub arguments: Vec<ASTExpression>,
}

impl ASTFunctionCallExpression {
    pub fn new(name: String, arguments: Vec<ASTExpression>) -> Self {
        ASTFunctionCallExpression { name, arguments }
    }
}
