//! # Evaluator
//!
//! Walks an AST produced by the [`Parser`](crate::ast::parser::Parser) and
//! computes a [`Value`] for each expression.
//!
//! ## Error handling
//!
//! Errors are accumulated in [`ASTEvaluator::errors`] rather than aborting
//! immediately, so a single pass can surface multiple problems.
//!
//! ## Short-circuit evaluation
//!
//! `&&` and `||` skip the right operand when the result is determined by the
//! left operand alone, matching standard boolean semantics.

use crate::ast::{ASTVisitor, ASTBinaryExpression, ASTNumberExpression, ASTBinaryOperatorKind, ASTUnaryExpression, ASTUnaryOperatorKind, ASTVariableDeclaration, ASTAssignment, ASTIdentifierExpression, ASTFunctionCallExpression, ASTPostfixUnaryExpression, ASTIfStatement, ASTFunctionDeclaration, ASTReturnStatement, ASTStatement};
use crate::ast::diagnostic::{Diagnostic, DiagnosticKind};
use crate::ast::types::Value;
use crate::ast::lexer::TextSpan;
use crate::ast::symbol_table::SymbolTable;
use std::collections::HashMap;

/// A user-defined function.
///
/// `closure` snapshots variable bindings lexically, by value, at the point
/// the `fn` is declared (cheap to clone: `SymbolTable` shares scopes via
/// `Rc` until mutated). Function *names*, by contrast, are deliberately
/// **not** captured here — they resolve dynamically through
/// [`ASTEvaluator::function_scopes`] at call time. That asymmetry is
/// intentional: snapshotting function names at declaration time would break
/// forward references and mutual recursion between sibling functions
/// (`fn a` calling `fn b` declared later in the same scope).
#[derive(Clone)]
struct UserFunction {
    name: String,
    parameters: Vec<String>,
    body: Vec<ASTStatement>,
    closure: SymbolTable,
}

#[derive(Clone)]
struct FunctionScope {
    functions: HashMap<String, UserFunction>,
}

impl FunctionScope {
    fn new() -> Self {
        Self { functions: HashMap::new() }
    }
}

/// Evaluates an Arc AST, maintaining interpreter state across statements.
///
/// After calling [`Ast::visit`](crate::ast::Ast::visit):
/// - [`last_value`](ASTEvaluator::last_value) holds the result of the most
///   recently evaluated expression (if any).
/// - [`errors`](ASTEvaluator::errors) holds any runtime errors encountered.
pub struct ASTEvaluator {
    /// The value produced by the most recently evaluated expression.
    pub last_value: Option<Value>,
    /// Runtime errors accumulated during evaluation.
    pub errors: Vec<Diagnostic>,
    /// Variable storage for the current execution context.
    pub symbol_table: SymbolTable,
    function_scopes: Vec<FunctionScope>,
    call_stack: Vec<String>,
    return_value: Option<Value>,
    /// Set once a fatal error (e.g. stack overflow) is reported, so the
    /// unwind back through enclosing calls doesn't pile on a "failed to
    /// evaluate" diagnostic at every stack frame for the same root cause.
    fatal: bool,
}

impl ASTEvaluator {
    /// Maximum function call nesting depth before we bail with a diagnostic
    /// instead of overflowing the host stack (Arc recursion runs directly on
    /// Rust's call stack via the visitor).
    const MAX_CALL_DEPTH: usize = 200;

    /// Creates a new evaluator with an empty symbol table and no errors.
    pub fn new() -> Self {
        Self { 
            last_value: None,
            errors: Vec::new(),
            symbol_table: SymbolTable::new(),
            function_scopes: vec![FunctionScope::new()],
            call_stack: Vec::new(),
            return_value: None,
            fatal: false,
        }
    }

    fn add_error(&mut self, error: impl Into<String>) {
        if self.fatal {
            return;
        }
        self.errors.push(Diagnostic::new(DiagnosticKind::RuntimeError, error, None));
    }

    fn add_error_at(&mut self, error: impl Into<String>, span: Option<TextSpan>) {
        if self.fatal {
            return;
        }
        self.errors.push(Diagnostic::new(DiagnosticKind::RuntimeError, error, span));
    }

    fn add_error_with_suggestion_at(
        &mut self,
        error: impl Into<String>,
        span: Option<TextSpan>,
        suggestion: impl Into<String>,
    ) {
        if self.fatal {
            return;
        }
        self.errors.push(
            Diagnostic::new(DiagnosticKind::RuntimeError, error, span)
                .with_suggestion(suggestion),
        );
    }

    fn closest_name(target: &str, candidates: &[String]) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for candidate in candidates {
            let d = Self::levenshtein(target, candidate);
            if d <= 2 {
                match &best {
                    Some((best_d, _)) if d >= *best_d => {}
                    _ => best = Some((d, candidate.clone())),
                }
            }
        }
        best.map(|(_, s)| s)
    }

    fn levenshtein(a: &str, b: &str) -> usize {
        if a.is_empty() {
            return b.chars().count();
        }
        if b.is_empty() {
            return a.chars().count();
        }

        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
        let mut curr: Vec<usize> = vec![0; b_chars.len() + 1];

        for (i, ca) in a_chars.iter().enumerate() {
            curr[0] = i + 1;
            for (j, cb) in b_chars.iter().enumerate() {
                let cost = if ca == cb { 0 } else { 1 };
                curr[j + 1] = (curr[j] + 1)
                    .min(prev[j + 1] + 1)
                    .min(prev[j] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[b_chars.len()]
    }

    fn enter_lexical_scope(&mut self) {
        self.symbol_table.enter_scope();
        self.function_scopes.push(FunctionScope::new());
    }

    fn exit_lexical_scope(&mut self) -> Result<(), String> {
        self.function_scopes.pop().ok_or_else(|| "Cannot exit function scope".to_string())?;
        self.symbol_table.exit_scope()
    }

    fn define_user_function(&mut self, function: UserFunction) -> Result<(), String> {
        let current_scope = self.function_scopes.last_mut().ok_or_else(|| "No active function scope".to_string())?;
        if current_scope.functions.contains_key(&function.name) {
            return Err(format!("Function '{}' already declared in this scope", function.name));
        }
        current_scope.functions.insert(function.name.clone(), function);
        Ok(())
    }

    fn lookup_user_function(&self, name: &str) -> Option<UserFunction> {
        for scope in self.function_scopes.iter().rev() {
            if let Some(function) = scope.functions.get(name) {
                return Some(function.clone());
            }
        }
        None
    }

    fn all_function_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for scope in self.function_scopes.iter().rev() {
            for name in scope.functions.keys() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    fn execute_statements(&mut self, statements: &[ASTStatement]) {
        for statement in statements {
            self.visit_statement(statement);
            if self.return_value.is_some() {
                break;
            }
        }
    }

    fn call_user_function(&mut self, function: UserFunction, argument_values: Vec<Value>) {
        if self.call_stack.len() >= Self::MAX_CALL_DEPTH {
            self.add_error(format!(
                "Stack overflow: maximum recursion depth ({}) exceeded while calling '{}'",
                Self::MAX_CALL_DEPTH,
                function.name
            ));
            self.fatal = true;
            self.last_value = None;
            return;
        }

        if argument_values.len() != function.parameters.len() {
            self.add_error(format!(
                "Function '{}' expected {} arguments but got {}",
                function.name,
                function.parameters.len(),
                argument_values.len()
            ));
            self.last_value = None;
            return;
        }

        let saved_symbol_table = std::mem::replace(&mut self.symbol_table, function.closure.clone());
        let saved_last_value = self.last_value.clone();
        let saved_return_value = self.return_value.clone();
        self.call_stack.push(function.name.clone());

        self.enter_lexical_scope();
        for (parameter, argument) in function.parameters.iter().zip(argument_values.into_iter()) {
            if let Err(error) = self.symbol_table.define(parameter.clone(), argument, true) {
                self.add_error(error);
                let _ = self.exit_lexical_scope();
                self.call_stack.pop();
                self.symbol_table = saved_symbol_table;
                self.last_value = saved_last_value;
                self.return_value = saved_return_value;
                return;
            }
        }

        self.return_value = None;
        self.execute_statements(&function.body);
        let result = self.return_value.take().or_else(|| self.last_value.clone());

        if let Err(error) = self.exit_lexical_scope() {
            self.add_error(error);
        }

        self.call_stack.pop();
        self.symbol_table = saved_symbol_table;
        self.last_value = saved_last_value;
        self.return_value = saved_return_value;

        self.last_value = result;
    }
}

impl ASTVisitor for ASTEvaluator {
    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.last_value = Some(number.value.clone());
    }

    /// Evaluates a binary expression.
    ///
    /// `&&` and `||` use short-circuit logic: if the left operand determines
    /// the result, the right operand is not evaluated.
    ///
    /// All other operators evaluate both operands before computing the result.
    /// Type coercion (e.g. `int + float`) is handled by
    /// [`Value::coerce_to_common_type`].
    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression) {
        let op_span = Some(expr.operator.token.span.clone());
        // Handle short-circuit evaluation for logical operators (optimization + correctness)
        match expr.operator.kind {
            ASTBinaryOperatorKind::LogicalAnd => {
                // Evaluate left operand first
                self.visit_expression(&expr.left);
                let left = match &self.last_value {
                    Some(v) => v.clone(),
                    None => return,
                };
                
                // If left is false, result is false without evaluating right
                if !left.to_boolean() {
                    self.last_value = Some(Value::Boolean(false));
                    return;
                }
                
                self.visit_expression(&expr.right);
                let right = match &self.last_value {
                    Some(v) => v.clone(),
                    None => return,
                };
                
                self.last_value = Some(Value::Boolean(right.to_boolean()));
                return;
            },
            ASTBinaryOperatorKind::LogicalOr => {
                // Short-circuit: if left is true, don't evaluate right
                self.visit_expression(&expr.left);
                let left = match &self.last_value {
                    Some(v) => v.clone(),
                    None => return,
                };
                
                if left.to_boolean() {
                    self.last_value = Some(Value::Boolean(true));
                    return;
                }
                
                self.visit_expression(&expr.right);
                let right = match &self.last_value {
                    Some(v) => v.clone(),
                    None => return,
                };
                
                self.last_value = Some(Value::Boolean(right.to_boolean()));
                return;
            },
            _ => {}, // Continue with normal evaluation
        }

        // Normal evaluation for non-short-circuit operators
        self.visit_expression(&expr.left);
        let left = match &self.last_value {
            Some(v) => v.clone(),
            None => {
                self.add_error_at("Left operand evaluation failed", op_span.clone());
                return;
            }
        };
        
        self.visit_expression(&expr.right);
        let right = match &self.last_value {
            Some(v) => v.clone(),
            None => {
                self.add_error_at("Right operand evaluation failed", op_span.clone());
                return;
            }
        };

        self.last_value = match expr.operator.kind {
            ASTBinaryOperatorKind::Plus => {
                // Try to coerce operands to compatible types (e.g., int + float -> float + float)
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Some(Value::Integer(a + b)),
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a + b)),
                        (Value::String(a), Value::String(b)) => Some(Value::String(format!("{}{}", a, b))),
                        _ => {
                            self.add_error_at(format!("Cannot add {:?} and {:?}", left.get_type(), right.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Minus => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Some(Value::Integer(a - b)),
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a - b)),
                        _ => {
                            self.add_error_at(format!("Cannot subtract {:?} from {:?}", right.get_type(), left.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Multiply => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Some(Value::Integer(a * b)),
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a * b)),
                        _ => {
                            self.add_error_at(format!("Cannot multiply {:?} and {:?}", left.get_type(), right.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Divide => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            // Check for division by zero at runtime
                            if b == 0 {
                                self.add_error_at("Division by zero", op_span.clone());
                                None
                            } else {
                                Some(Value::Integer(a / b))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => {
                            // Floating point division by zero check
                            if b == 0.0 {
                                self.add_error_at("Division by zero", op_span.clone());
                                None
                            } else {
                                Some(Value::Float(a / b))
                            }
                        },
                        _ => {
                            self.add_error_at(format!("Cannot divide {:?} by {:?}", left.get_type(), right.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Modulo => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            if b == 0 {
                                self.add_error_at("Modulo by zero", op_span.clone());
                                None
                            } else {
                                Some(Value::Integer(a % b))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a % b)),
                        _ => {
                            self.add_error_at(format!("Cannot compute modulo of {:?} and {:?}", left.get_type(), right.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Exponentiation => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            // Negative exponent requires float result (e.g., 2^-1 = 0.5)
                            if b < 0 {
                                Some(Value::Float((a as f64).powf(b as f64)))
                            } else {
                                Some(Value::Integer(a.pow(b as u32)))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a.powf(b))),
                        _ => {
                            self.add_error_at(format!("Cannot exponentiate {:?} and {:?}", left.get_type(), right.get_type()), op_span.clone());
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            // Bitwise operations only work on integers
            ASTBinaryOperatorKind::BitwiseAnd => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l & r)),
                    _ => {
                        self.add_error_at("Bitwise AND requires integer operands", op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::BitwiseOr => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l | r)),
                    _ => {
                        self.add_error_at("Bitwise OR requires integer operands", op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::BitwiseXor => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l ^ r)),
                    _ => {
                        self.add_error_at("Bitwise XOR requires integer operands", op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::LeftShift => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l << r)),
                    _ => {
                        self.add_error_at("Left shift requires integer operands", op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::RightShift => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l >> r)),
                    _ => {
                        self.add_error_at("Right shift requires integer operands", op_span.clone());
                        None
                    }
                }
            },
            // Comparison operators
            ASTBinaryOperatorKind::Equal => {
                match left.equals(&right) {
                    Ok(result) => Some(Value::Boolean(result)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::NotEqual => {
                match left.equals(&right) {
                    Ok(result) => Some(Value::Boolean(!result)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Less => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering == std::cmp::Ordering::Less)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Greater => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering == std::cmp::Ordering::Greater)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::LessEqual => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering != std::cmp::Ordering::Greater)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::GreaterEqual => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering != std::cmp::Ordering::Less)),
                    Err(e) => {
                        self.add_error_at(e, op_span.clone());
                        None
                    }
                }
            },
            // Logical operators are handled at the beginning with short-circuit
            ASTBinaryOperatorKind::LogicalAnd | ASTBinaryOperatorKind::LogicalOr => {
                unreachable!("Logical operators should be handled by short-circuit evaluation")
            },
        };
    }

    fn visit_unary_expression(&mut self, unary_expr: &ASTUnaryExpression) {
        // For increment/decrement, we need to handle them specially
        match unary_expr.operator.kind {
            ASTUnaryOperatorKind::Increment | ASTUnaryOperatorKind::Decrement => {
                // For prefix ++/-- the operand must be an identifier
                if let crate::ast::ASTExpressionKind::Identifier(ident) = &unary_expr.operand.kind {
                    let name = &ident.name;
                    // Get current value
                    match self.symbol_table.get_value(name) {
                        Ok(value) => {
                            let new_value = match unary_expr.operator.kind {
                                ASTUnaryOperatorKind::Increment => match value {
                                    Value::Integer(i) => Value::Integer(i + 1),
                                    Value::Float(f) => Value::Float(f + 1.0),
                                    _ => {
                                        self.add_error(format!("Cannot increment {:?}", value.get_type()));
                                        return;
                                    }
                                },
                                ASTUnaryOperatorKind::Decrement => match value {
                                    Value::Integer(i) => Value::Integer(i - 1),
                                    Value::Float(f) => Value::Float(f - 1.0),
                                    _ => {
                                        self.add_error(format!("Cannot decrement {:?}", value.get_type()));
                                        return;
                                    }
                                },
                                _ => unreachable!(),
                            };
                            // Assign the new value (prefix returns new value)
                            if let Err(e) = self.symbol_table.assign(name, new_value.clone()) {
                                self.add_error(e);
                                return;
                            }
                            self.last_value = Some(new_value);
                        }
                        Err(e) => {
                            self.add_error(e);
                            return;
                        }
                    }
                } else {
                    self.add_error("Increment/Decrement can only be applied to variables".to_string());
                    return;
                }
            }
            _ => {
                // For other unary operators, evaluate the operand normally
                self.visit_expression(&unary_expr.operand);
                let operand = match &self.last_value {
                    Some(v) => v.clone(),
                    None => {
                        self.add_error("Operand evaluation failed".to_string());
                        return;
                    }
                };
                
                self.last_value = match unary_expr.operator.kind {
                    ASTUnaryOperatorKind::Plus => Some(operand),
                    ASTUnaryOperatorKind::Minus => match operand {
                        Value::Integer(i) => Some(Value::Integer(-i)),
                        Value::Float(f) => Some(Value::Float(-f)),
                        _ => {
                            self.add_error(format!("Cannot negate {:?}", operand.get_type()));
                            None
                        }
                    },
                    ASTUnaryOperatorKind::LogicalNot => {
                        Some(Value::Boolean(!operand.to_boolean()))
                    },
                    _ => unreachable!(),
                };
            }
        }
    }

    fn visit_postfix_unary_expression(&mut self, postfix_expr: &ASTPostfixUnaryExpression) {
        // For postfix ++/--, the operand must be an identifier
        if let crate::ast::ASTExpressionKind::Identifier(ident) = &postfix_expr.operand.kind {
            let name = &ident.name;
            // Get current value
            match self.symbol_table.get_value(name) {
                Ok(value) => {
                    // For postfix, we return the old value before incrementing/decrementing
                    let old_value = value.clone();
                    let new_value = match postfix_expr.operator.kind {
                        ASTUnaryOperatorKind::Increment => match value {
                            Value::Integer(i) => Value::Integer(i + 1),
                            Value::Float(f) => Value::Float(f + 1.0),
                            _ => {
                                self.add_error(format!("Cannot increment {:?}", value.get_type()));
                                return;
                            }
                        },
                        ASTUnaryOperatorKind::Decrement => match value {
                            Value::Integer(i) => Value::Integer(i - 1),
                            Value::Float(f) => Value::Float(f - 1.0),
                            _ => {
                                self.add_error(format!("Cannot decrement {:?}", value.get_type()));
                                return;
                            }
                        },
                        _ => {
                            self.add_error("Invalid postfix operator".to_string());
                            return;
                        }
                    };
                    // Assign the new value but return the old value (standard postfix semantics)
                    if let Err(e) = self.symbol_table.assign(name, new_value) {
                        self.add_error(e);
                        return;
                    }
                    self.last_value = Some(old_value);
                }
                Err(e) => {
                    self.add_error(e);
                    return;
                }
            }
        } else {
            self.add_error("Postfix Increment/Decrement can only be applied to variables".to_string());
            return;
        }
    }

    fn visit_identifier(&mut self, ident: &ASTIdentifierExpression) {
        match self.symbol_table.get_value(&ident.name) {
            Ok(value) => self.last_value = Some(value),
            Err(e) => {
                let span = ident.token.as_ref().map(|t| t.span.clone());
                let visible_names = self.symbol_table.all_names();
                if let Some(suggestion_name) = Self::closest_name(&ident.name, &visible_names) {
                    self.add_error_with_suggestion_at(
                        e,
                        span,
                        format!("did you mean '{}' ?", suggestion_name),
                    );
                } else {
                    self.add_error_at(e, span);
                }
                self.last_value = None;
            }
        }
    }

    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        // Evaluate the initializer
        self.visit_expression(&decl.initializer);
        
        match &self.last_value {
            Some(value) => {
                if let Err(e) = self.symbol_table.define(
                    decl.name.clone(),
                    value.clone(),
                    decl.is_mutable
                ) {
                    let span = decl.name_span.as_ref().map(|t| t.span.clone());
                    self.add_error_at(e, span);
                }
            }
            None => {
                let span = decl.name_span.as_ref().map(|t| t.span.clone());
                self.add_error_at(format!("Failed to evaluate initializer for variable '{}'", decl.name), span);
            }
        }
    }

    fn visit_function_declaration(&mut self, func: &ASTFunctionDeclaration) {
        let function = UserFunction {
            name: func.name.clone(),
            parameters: func.parameters.clone(),
            body: func.body.clone(),
            closure: self.symbol_table.clone(),
        };

        if let Err(error) = self.define_user_function(function) {
            self.add_error(error);
        }

        self.last_value = None;
    }

    fn visit_return_statement(&mut self, ret: &ASTReturnStatement) {
        if self.call_stack.is_empty() {
            self.add_error("'return' can only be used inside a function".to_string());
            self.last_value = None;
            return;
        }

        self.visit_expression(&ret.value);
        match &self.last_value {
            Some(value) => {
                self.return_value = Some(value.clone());
            }
            None => {
                self.add_error("Failed to evaluate return value".to_string());
            }
        }
    }

    fn visit_assignment(&mut self, assign: &ASTAssignment) {
        // Evaluate the value expression
        self.visit_expression(&assign.value);
        
        match &self.last_value {
            Some(value) => {
                if let Err(e) = self.symbol_table.assign(&assign.name, value.clone()) {
                    let span = assign.name_span.as_ref().map(|t| t.span.clone());
                    // Only offer a "did you mean" suggestion when the name
                    // itself is the problem (undefined). If it exists but
                    // the assignment failed (immutable / type mismatch),
                    // suggesting the exact same name back is nonsensical.
                    let suggestion_name = if self.symbol_table.exists(&assign.name) {
                        None
                    } else {
                        Self::closest_name(&assign.name, &self.symbol_table.all_names())
                    };
                    if let Some(suggestion_name) = suggestion_name {
                        self.add_error_with_suggestion_at(
                            e,
                            span,
                            format!("did you mean '{}' ?", suggestion_name),
                        );
                    } else {
                        self.add_error_at(e, span);
                    }
                }
            }
            None => {
                let span = assign.name_span.as_ref().map(|t| t.span.clone());
                self.add_error_at(format!("Failed to evaluate value for assignment to '{}'", assign.name), span);
            }
        }
    }

    fn visit_if_statement(&mut self, if_stmt: &ASTIfStatement) {
        self.visit_expression(&if_stmt.condition);

        let condition = match &self.last_value {
            Some(value) => value.to_boolean(),
            None => {
                self.add_error("Failed to evaluate if condition".to_string());
                return;
            }
        };

        if condition {
            self.enter_lexical_scope();
            self.execute_statements(&if_stmt.then_branch);
            if let Err(e) = self.exit_lexical_scope() {
                self.add_error(e);
            }
        } else if let Some(else_branch) = &if_stmt.else_branch {
            self.enter_lexical_scope();
            self.execute_statements(else_branch);
            if let Err(e) = self.exit_lexical_scope() {
                self.add_error(e);
            }
        }

        self.last_value = None;
    }

    fn visit_block(&mut self, statements: &Vec<ASTStatement>) {
        self.enter_lexical_scope();
        self.execute_statements(statements);
        if let Err(e) = self.exit_lexical_scope() {
            self.add_error(e);
        }
        self.last_value = None;
    }

    /// Dispatches to built-in function implementations.
    ///
    /// Currently `print`, `max`, and `min` are supported. Unknown function names are reported
    /// as errors via [`ASTEvaluator::errors`].
    fn visit_function_call(&mut self, func_call: &ASTFunctionCallExpression) {
        let call_span = func_call.token.as_ref().map(|t| t.span.clone());
        let mut argument_values = Vec::new();
        for arg in &func_call.arguments {
            self.visit_expression(arg);
            if let Some(value) = &self.last_value {
                argument_values.push(value.clone());
            } else {
                self.add_error_at(format!("Failed to evaluate argument to '{}'", func_call.name), call_span.clone());
                self.last_value = None;
                return;
            }
        }

        if let Some(function) = self.lookup_user_function(&func_call.name) {
            self.call_user_function(function, argument_values);
            return;
        }

        match func_call.name.as_str() {
            "print" => {
                // Print the values
                for (i, value) in argument_values.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    match value {
                        Value::Integer(n) => print!("{}", n),
                        Value::Float(f) => print!("{}", f),
                        Value::Boolean(b) => print!("{}", b),
                        Value::String(s) => print!("{}", s),
                    }
                }
                println!();
                
                // print() doesn't return a value
                self.last_value = None;
            }
            "max" => {
                if argument_values.is_empty() {
                    self.add_error_at("max() requires at least one argument", call_span.clone());
                    self.last_value = None;
                    return;
                }

                // Reduce using compare
                let mut current = argument_values[0].clone();
                for v in argument_values.into_iter().skip(1) {
                    match current.compare(&v) {
                        Ok(std::cmp::Ordering::Less) => current = v,
                        Ok(_) => (),
                        Err(e) => {
                            self.add_error_at(format!("max() comparison error: {}", e), call_span.clone());
                            self.last_value = None;
                            return;
                        }
                    }
                }

                self.last_value = Some(current);
            }
            "min" => {
                if argument_values.is_empty() {
                    self.add_error_at("min() requires at least one argument", call_span.clone());
                    self.last_value = None;
                    return;
                }

                // Reduce using compare
                let mut current = argument_values[0].clone();
                for v in argument_values.into_iter().skip(1) {
                    match current.compare(&v) {
                        Ok(std::cmp::Ordering::Greater) => current = v,
                        Ok(_) => (),
                        Err(e) => {
                            self.add_error_at(format!("min() comparison error: {}", e), call_span.clone());
                            self.last_value = None;
                            return;
                        }
                    }
                }

                self.last_value = Some(current);
            }
            _ => {
                let mut all_functions = vec!["print".to_string(), "max".to_string(), "min".to_string()];
                all_functions.extend(self.all_function_names());

                if let Some(suggestion_name) = Self::closest_name(&func_call.name, &all_functions) {
                    self.add_error_with_suggestion_at(
                        format!("Unknown function: '{}'", func_call.name),
                        call_span,
                        format!("did you mean '{}' ?", suggestion_name),
                    );
                } else {
                    self.add_error_at(format!("Unknown function: '{}'", func_call.name), call_span);
                }
                self.last_value = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::lexer::Lexer;
    use crate::ast::parser::Parser;
    use crate::ast::Ast;

    /// Runs a full source program through the lexer, parser, and evaluator.
    /// Panics if the program has parse errors, since these tests are about
    /// evaluator behavior, not parsing.
    fn run(source: &str) -> ASTEvaluator {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }

        let mut parser = Parser::new(tokens);
        let mut ast = Ast::new();
        while let Some(stmt) = parser.next_statement() {
            ast.add_statement(stmt);
        }
        assert!(parser.diagnostics.is_empty(), "unexpected parse errors: {:?}", parser.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());

        let mut evaluator = ASTEvaluator::new();
        ast.visit(&mut evaluator);
        evaluator
    }

    #[test]
    fn test_arithmetic_and_precedence() {
        let evaluator = run("1 + 2 * 3;");
        assert!(evaluator.errors.is_empty());
        assert_eq!(evaluator.last_value, Some(Value::Integer(7)));
    }

    #[test]
    fn test_int_float_coercion() {
        let evaluator = run("1 + 2.5;");
        assert!(evaluator.errors.is_empty());
        assert_eq!(evaluator.last_value, Some(Value::Float(3.5)));
    }

    #[test]
    fn test_division_by_zero_is_reported() {
        let evaluator = run("1 / 0;");
        assert_eq!(evaluator.errors.len(), 1);
        assert!(evaluator.errors[0].message.contains("Division by zero"));
    }

    #[test]
    fn test_variable_declaration_and_assignment() {
        let evaluator = run("let x = 1; x = 2; x;");
        assert!(evaluator.errors.is_empty());
        assert_eq!(evaluator.last_value, Some(Value::Integer(2)));
    }

    #[test]
    fn test_const_reassignment_is_rejected() {
        let evaluator = run("const x = 1; x = 2;");
        assert_eq!(evaluator.errors.len(), 1);
        assert!(evaluator.errors[0].message.contains("immutable"));
    }

    #[test]
    fn test_assign_type_mismatch_is_rejected() {
        let evaluator = run(r#"let x = 1; x = "oops";"#);
        assert_eq!(evaluator.errors.len(), 1);
        assert!(evaluator.errors[0].message.contains("Type mismatch"));
        // The variable name itself is fine; a "did you mean 'x'?" suggestion
        // here would be nonsensical.
        assert!(evaluator.errors[0].suggestion.is_none());
    }

    #[test]
    fn test_if_else_branches() {
        assert_eq!(run("let r = 0; if true { r = 1; } else { r = 2; } r;").last_value, Some(Value::Integer(1)));
        assert_eq!(run("let r = 0; if false { r = 1; } else { r = 2; } r;").last_value, Some(Value::Integer(2)));
    }

    #[test]
    fn test_user_function_call_and_return() {
        let evaluator = run("fn add(a, b) { return a + b; } add(2, 3);");
        assert!(evaluator.errors.is_empty());
        assert_eq!(evaluator.last_value, Some(Value::Integer(5)));
    }

    #[test]
    fn test_recursive_function() {
        let evaluator = run(
            "fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(10);",
        );
        assert!(evaluator.errors.is_empty());
        assert_eq!(evaluator.last_value, Some(Value::Integer(3628800)));
    }

    #[test]
    fn test_unbounded_recursion_is_reported_once_without_crashing() {
        let evaluator = run("fn boom(n) { return boom(n + 1); } boom(0);");
        assert_eq!(evaluator.errors.len(), 1);
        assert!(evaluator.errors[0].message.contains("Stack overflow"));
    }

    #[test]
    fn test_undefined_variable_suggests_closest_name() {
        let evaluator = run("let count = 1; coutn;");
        assert_eq!(evaluator.errors.len(), 1);
        assert_eq!(evaluator.errors[0].suggestion.as_deref(), Some("did you mean 'count' ?"));
    }
}

