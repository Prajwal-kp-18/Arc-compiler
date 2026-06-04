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
use crate::ast::types::Value;
use crate::ast::symbol_table::SymbolTable;
use std::collections::HashMap;

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
    pub errors: Vec<String>,
    /// Variable storage for the current execution context.
    pub symbol_table: SymbolTable,
    function_scopes: Vec<FunctionScope>,
    call_stack: Vec<String>,
    return_value: Option<Value>,
}

impl ASTEvaluator {
    /// Creates a new evaluator with an empty symbol table and no errors.
    pub fn new() -> Self {
        Self { 
            last_value: None,
            errors: Vec::new(),
            symbol_table: SymbolTable::new(),
            function_scopes: vec![FunctionScope::new()],
            call_stack: Vec::new(),
            return_value: None,
        }
    }

    fn add_error(&mut self, error: String) {
        self.errors.push(error);
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

    fn execute_statements(&mut self, statements: &[ASTStatement]) {
        for statement in statements {
            self.visit_statement(statement);
            if self.return_value.is_some() {
                break;
            }
        }
    }

    fn call_user_function(&mut self, function: UserFunction, argument_values: Vec<Value>) {
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
                self.add_error("Left operand evaluation failed".to_string());
                return;
            }
        };
        
        self.visit_expression(&expr.right);
        let right = match &self.last_value {
            Some(v) => v.clone(),
            None => {
                self.add_error("Right operand evaluation failed".to_string());
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
                            self.add_error(format!("Cannot add {:?} and {:?}", left.get_type(), right.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
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
                            self.add_error(format!("Cannot subtract {:?} from {:?}", right.get_type(), left.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
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
                            self.add_error(format!("Cannot multiply {:?} and {:?}", left.get_type(), right.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
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
                                self.add_error("Division by zero".to_string());
                                None
                            } else {
                                Some(Value::Integer(a / b))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => {
                            // Floating point division by zero check
                            if b == 0.0 {
                                self.add_error("Division by zero".to_string());
                                None
                            } else {
                                Some(Value::Float(a / b))
                            }
                        },
                        _ => {
                            self.add_error(format!("Cannot divide {:?} by {:?}", left.get_type(), right.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Modulo => {
                match Value::coerce_to_common_type(&left, &right) {
                    Ok((l, r)) => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            if b == 0 {
                                self.add_error("Modulo by zero".to_string());
                                None
                            } else {
                                Some(Value::Integer(a % b))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => Some(Value::Float(a % b)),
                        _ => {
                            self.add_error(format!("Cannot compute modulo of {:?} and {:?}", left.get_type(), right.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
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
                            self.add_error(format!("Cannot exponentiate {:?} and {:?}", left.get_type(), right.get_type()));
                            None
                        }
                    },
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            // Bitwise operations only work on integers
            ASTBinaryOperatorKind::BitwiseAnd => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l & r)),
                    _ => {
                        self.add_error("Bitwise AND requires integer operands".to_string());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::BitwiseOr => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l | r)),
                    _ => {
                        self.add_error("Bitwise OR requires integer operands".to_string());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::BitwiseXor => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l ^ r)),
                    _ => {
                        self.add_error("Bitwise XOR requires integer operands".to_string());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::LeftShift => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l << r)),
                    _ => {
                        self.add_error("Left shift requires integer operands".to_string());
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::RightShift => {
                match (left.to_integer(), right.to_integer()) {
                    (Ok(l), Ok(r)) => Some(Value::Integer(l >> r)),
                    _ => {
                        self.add_error("Right shift requires integer operands".to_string());
                        None
                    }
                }
            },
            // Comparison operators
            ASTBinaryOperatorKind::Equal => {
                match left.equals(&right) {
                    Ok(result) => Some(Value::Boolean(result)),
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::NotEqual => {
                match left.equals(&right) {
                    Ok(result) => Some(Value::Boolean(!result)),
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Less => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering == std::cmp::Ordering::Less)),
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::Greater => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering == std::cmp::Ordering::Greater)),
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::LessEqual => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering != std::cmp::Ordering::Greater)),
                    Err(e) => {
                        self.add_error(e);
                        None
                    }
                }
            },
            ASTBinaryOperatorKind::GreaterEqual => {
                match left.compare(&right) {
                    Ok(ordering) => Some(Value::Boolean(ordering != std::cmp::Ordering::Less)),
                    Err(e) => {
                        self.add_error(e);
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
                self.add_error(e);
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
                    self.add_error(e);
                }
            }
            None => {
                self.add_error(format!("Failed to evaluate initializer for variable '{}'", decl.name));
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
                    self.add_error(e);
                }
            }
            None => {
                self.add_error(format!("Failed to evaluate value for assignment to '{}'", assign.name));
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

    /// Dispatches to built-in function implementations.
    ///
    /// Currently `print`, `max`, and `min` are supported. Unknown function names are reported
    /// as errors via [`ASTEvaluator::errors`].
    fn visit_function_call(&mut self, func_call: &ASTFunctionCallExpression) {
        let mut argument_values = Vec::new();
        for arg in &func_call.arguments {
            self.visit_expression(arg);
            if let Some(value) = &self.last_value {
                argument_values.push(value.clone());
            } else {
                self.add_error(format!("Failed to evaluate argument to '{}'", func_call.name));
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
                    self.add_error("max() requires at least one argument".to_string());
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
                            self.add_error(format!("max() comparison error: {}", e));
                            self.last_value = None;
                            return;
                        }
                    }
                }

                self.last_value = Some(current);
            }
            "min" => {
                if argument_values.is_empty() {
                    self.add_error("min() requires at least one argument".to_string());
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
                            self.add_error(format!("min() comparison error: {}", e));
                            self.last_value = None;
                            return;
                        }
                    }
                }

                self.last_value = Some(current);
            }
            _ => {
                self.add_error(format!("Unknown function: '{}'", func_call.name));
                self.last_value = None;
            }
        }
    }
}

