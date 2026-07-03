//! # Evaluator
//!
//! Walks an AST already annotated by the
//! [`Resolver`](crate::ast::resolver::Resolver) and computes a [`Value`] for
//! each expression.
//!
//! ## Slot-based storage
//!
//! Variable storage is no longer a string-keyed `HashMap` scope stack.
//! Every `let`/`const`/parameter was assigned a [`SlotIndex`] by the
//! Resolver ahead of time, so reads/writes here are direct `Vec<Value>`
//! indexing: [`ASTEvaluator::globals`] for top-level bindings, and the top
//! of [`ASTEvaluator::frames`] for the current function call's locals.
//! Function declarations are registered once (not re-cloned per call) into
//! [`ASTEvaluator::functions`], keyed by the Resolver's `FunctionId`.
//!
//! Because the Resolver already guarantees every name in a program that
//! passed resolution refers to something real, of a compatible type, this
//! evaluator no longer does its own name lookup, "did you mean" suggestion,
//! immutability check, or type-mismatch check — those diagnostics now come
//! from the Resolver, before evaluation ever starts.
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

use crate::ast::{ASTVisitor, ASTBinaryExpression, ASTNumberExpression, ASTBinaryOperatorKind, ASTUnaryExpression, ASTUnaryOperatorKind, ASTVariableDeclaration, ASTAssignment, ASTIdentifierExpression, ASTFunctionCallExpression, ASTPostfixUnaryExpression, ASTIfStatement, ASTFunctionDeclaration, ASTReturnStatement, ASTStatement, ASTStatementKind, ASTExpressionKind, Ast};
use crate::ast::diagnostic::{Diagnostic, DiagnosticKind};
use crate::ast::types::Value;
use crate::ast::lexer::TextSpan;
use crate::ast::resolver::{BuiltinFn, ResolvedBinding, ResolvedCall, SlotIndex};
use std::collections::HashMap;
use std::rc::Rc;

/// A registered user-defined function, ready to be called by `FunctionId`.
///
/// `body` is `Rc`-shared so cloning this struct out of the registry on every
/// call (needed to avoid holding a borrow of `self.functions` across the
/// `&mut self` call that executes it) is O(1), not O(body size).
#[derive(Clone)]
struct FunctionRuntime {
    name: String,
    param_slots: Vec<SlotIndex>,
    frame_size: u32,
    body: Rc<Vec<ASTStatement>>,
}

/// Evaluates an Arc AST, maintaining interpreter state across statements.
///
/// After calling [`ASTEvaluator::execute`]:
/// - [`last_value`](ASTEvaluator::last_value) holds the result of the most
///   recently evaluated expression (if any).
/// - [`errors`](ASTEvaluator::errors) holds any runtime errors encountered.
pub struct ASTEvaluator {
    /// The value produced by the most recently evaluated expression.
    pub last_value: Option<Value>,
    /// Runtime errors accumulated during evaluation.
    pub errors: Vec<Diagnostic>,
    /// Top-level ("global") variable slots, sized by the Resolver.
    globals: Vec<Value>,
    /// Call-frame stack; each frame is one function call's local slots.
    /// Call depth is `frames.len()`.
    frames: Vec<Vec<Value>>,
    /// Registered function bodies, keyed by `FunctionId.0`.
    functions: HashMap<u32, FunctionRuntime>,
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

    /// Creates a new evaluator. `global_slot_count` comes from
    /// [`Resolver::global_slot_count`](crate::ast::resolver::Resolver::global_slot_count)
    /// after a successful resolve pass.
    pub fn new(global_slot_count: u32) -> Self {
        Self {
            last_value: None,
            errors: Vec::new(),
            globals: vec![Value::Integer(0); global_slot_count as usize],
            frames: Vec::new(),
            functions: HashMap::new(),
            return_value: None,
            fatal: false,
        }
    }

    /// Executes an already-resolved program.
    pub fn execute(&mut self, ast: &Ast) {
        self.execute_statements(&ast.statements);
    }

    /// Grows global storage to fit `global_slot_count` slots, if needed.
    ///
    /// For one-shot file execution this is a no-op after the initial
    /// `new()`. For a REPL, where the Resolver's global slot count grows as
    /// each new line declares more top-level variables, call this before
    /// [`ASTEvaluator::execute`] on every line.
    pub fn ensure_global_capacity(&mut self, global_slot_count: u32) {
        let needed = global_slot_count as usize;
        if self.globals.len() < needed {
            self.globals.resize(needed, Value::Integer(0));
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

    fn get_slot(&self, binding: ResolvedBinding) -> Value {
        match binding {
            ResolvedBinding::Global(slot) => self.globals[slot.0 as usize].clone(),
            ResolvedBinding::Local(slot) => self.frames.last().unwrap()[slot.0 as usize].clone(),
        }
    }

    fn set_slot(&mut self, binding: ResolvedBinding, value: Value) {
        match binding {
            ResolvedBinding::Global(slot) => self.globals[slot.0 as usize] = value,
            ResolvedBinding::Local(slot) => self.frames.last_mut().unwrap()[slot.0 as usize] = value,
        }
    }

    /// Registers every direct `fn` declaration in `statements` (idempotent —
    /// a function already registered from an earlier pass through this same
    /// block is left alone), then executes each statement in order.
    fn execute_statements(&mut self, statements: &[ASTStatement]) {
        self.hoist_functions(statements);
        for statement in statements {
            self.visit_statement(statement);
            if self.return_value.is_some() || self.fatal {
                break;
            }
        }
    }

    fn hoist_functions(&mut self, statements: &[ASTStatement]) {
        for statement in statements {
            if let ASTStatementKind::FunctionDeclaration(func) = &statement.kind {
                let id = func.function_id.get().expect("Resolver must run before evaluation").0;
                self.functions.entry(id).or_insert_with(|| FunctionRuntime {
                    name: func.name.clone(),
                    param_slots: func.parameters.iter().map(|p| p.slot.get().expect("Resolver assigns every parameter a slot")).collect(),
                    frame_size: func.frame_size.get().expect("Resolver computes every function's frame size"),
                    body: Rc::new(func.body.clone()),
                });
            }
        }
    }

    fn call_user_function(&mut self, function_id: u32, argument_values: Vec<Value>) {
        if self.frames.len() >= Self::MAX_CALL_DEPTH {
            let name = self.functions.get(&function_id).map(|f| f.name.as_str()).unwrap_or("?").to_string();
            self.add_error(format!(
                "Stack overflow: maximum recursion depth ({}) exceeded while calling '{}'",
                Self::MAX_CALL_DEPTH,
                name
            ));
            self.fatal = true;
            self.last_value = None;
            return;
        }

        // Cloned (cheap: body is Rc-shared) to avoid holding a borrow of
        // `self.functions` across the `&mut self` call below.
        let function = self.functions.get(&function_id)
            .expect("resolver guarantees every call resolves to a registered function")
            .clone();

        let mut frame = vec![Value::Integer(0); function.frame_size as usize];
        for (slot, value) in function.param_slots.iter().zip(argument_values.into_iter()) {
            frame[slot.0 as usize] = value;
        }

        self.frames.push(frame);
        let saved_last_value = self.last_value.take();
        let saved_return_value = self.return_value.take();

        self.execute_statements(&function.body);
        let result = self.return_value.take().or_else(|| self.last_value.clone());

        self.frames.pop();
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
    /// [`Value::coerce_to_common_type`] — the Resolver has already checked
    /// this operation is legal, so this is purely the runtime arithmetic on
    /// the actual tagged `Value`s.
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
                if let ASTExpressionKind::Identifier(ident) = &unary_expr.operand.kind {
                    let binding = ident.binding.get().expect("Resolver guarantees this identifier is resolved");
                    let value = self.get_slot(binding);
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
                    self.set_slot(binding, new_value.clone());
                    self.last_value = Some(new_value);
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
        if let ASTExpressionKind::Identifier(ident) = &postfix_expr.operand.kind {
            let binding = ident.binding.get().expect("Resolver guarantees this identifier is resolved");
            let value = self.get_slot(binding);
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
            self.set_slot(binding, new_value);
            self.last_value = Some(old_value);
        } else {
            self.add_error("Postfix Increment/Decrement can only be applied to variables".to_string());
            return;
        }
    }

    fn visit_identifier(&mut self, ident: &ASTIdentifierExpression) {
        let binding = ident.binding.get().expect("Resolver guarantees every identifier reference is resolved");
        self.last_value = Some(self.get_slot(binding));
    }

    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        // Evaluate the initializer
        self.visit_expression(&decl.initializer);

        match self.last_value.clone() {
            Some(value) => {
                let binding = decl.binding.get().expect("Resolver guarantees every declaration is resolved");
                self.set_slot(binding, value);
            }
            None => {
                let span = decl.name_span.as_ref().map(|t| t.span.clone());
                self.add_error_at(format!("Failed to evaluate initializer for variable '{}'", decl.name), span);
            }
        }
    }

    /// Registration happens in [`ASTEvaluator::hoist_functions`], run once
    /// before any statement in the enclosing block executes — visiting a
    /// `fn` statement directly is a no-op so its body isn't run immediately.
    fn visit_function_declaration(&mut self, _func: &ASTFunctionDeclaration) {
        self.last_value = None;
    }

    fn visit_return_statement(&mut self, ret: &ASTReturnStatement) {
        if self.frames.is_empty() {
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

        match self.last_value.clone() {
            Some(mut value) => {
                let binding = assign.binding.get().expect("Resolver guarantees every assignment target is resolved");
                if assign.needs_float_widen.get() {
                    if let Value::Integer(i) = value {
                        value = Value::Float(i as f64);
                    }
                }
                self.set_slot(binding, value);
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
            self.execute_statements(&if_stmt.then_branch);
        } else if let Some(else_branch) = &if_stmt.else_branch {
            self.execute_statements(else_branch);
        }

        self.last_value = None;
    }

    fn visit_block(&mut self, statements: &Vec<ASTStatement>) {
        self.execute_statements(statements);
        self.last_value = None;
    }

    /// Dispatches to built-in function implementations.
    ///
    /// Currently `print`, `max`, and `min` are supported.
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

        match func_call.resolved_call.get().expect("Resolver guarantees every call is resolved") {
            ResolvedCall::User(id) => {
                self.call_user_function(id.0, argument_values);
            }
            ResolvedCall::Builtin(BuiltinFn::Print) => {
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
            ResolvedCall::Builtin(BuiltinFn::Max) => {
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
            ResolvedCall::Builtin(BuiltinFn::Min) => {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::lexer::Lexer;
    use crate::ast::parser::Parser;
    use crate::ast::resolver::Resolver;
    use crate::ast::diagnostic::Severity;

    struct RunResult {
        last_value: Option<Value>,
        /// Blocking diagnostics (resolve-time then runtime, in that order)
        /// — tests don't need to care which stage caught what.
        errors: Vec<String>,
        /// Non-blocking Resolver warnings (e.g. "fell back to Any").
        warnings: Vec<String>,
    }

    /// Runs a full program through lex -> parse -> resolve -> evaluate,
    /// mirroring `main.rs`'s pipeline exactly (evaluation is skipped if
    /// resolution reports an error, same as the real CLI).
    fn run(source: &str) -> RunResult {
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

        let mut resolver = Resolver::new();
        resolver.resolve(&ast);
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        for d in &resolver.diagnostics {
            if d.severity == Severity::Error {
                errors.push(d.message.clone());
            } else {
                warnings.push(d.message.clone());
            }
        }
        if resolver.has_errors() {
            return RunResult { last_value: None, errors, warnings };
        }

        let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
        evaluator.execute(&ast);
        errors.extend(evaluator.errors.iter().map(|d| d.message.clone()));
        RunResult { last_value: evaluator.last_value, errors, warnings }
    }

    #[test]
    fn test_arithmetic_and_precedence() {
        let result = run("1 + 2 * 3;");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Integer(7)));
    }

    #[test]
    fn test_int_float_coercion() {
        let result = run("1 + 2.5;");
        assert!(result.errors.is_empty());
        assert_eq!(result.last_value, Some(Value::Float(3.5)));
    }

    #[test]
    fn test_division_by_zero_is_reported() {
        let result = run("1 / 0;");
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Division by zero"));
    }

    #[test]
    fn test_variable_declaration_and_assignment() {
        let result = run("let x = 1; x = 2; x;");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Integer(2)));
    }

    #[test]
    fn test_int_to_float_widening_on_assignment() {
        // The declared type (Float, from the initializer) persists; a later
        // Integer assignment must widen, not silently change the variable's
        // runtime representation to Integer.
        let result = run("let x = 1.5; x = 2; x + 0.5;");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Float(2.5)));
    }

    // -- Diagnostics that moved from the evaluator to the Resolver in Phase 1 --
    //
    // These pin down the exact message text so a future refactor can't
    // silently change what the user sees for these categories, per the
    // Phase 1 exit criterion ("Resolver's diagnostics match today's
    // evaluator's runtime diagnostics").

    #[test]
    fn test_const_reassignment_is_rejected() {
        let result = run("const x = 1; x = 2;");
        assert_eq!(result.errors, vec!["Cannot assign to immutable variable 'x'"]);
    }

    #[test]
    fn test_assign_type_mismatch_is_rejected() {
        let result = run(r#"let x = 1; x = "oops";"#);
        assert_eq!(result.errors, vec!["Type mismatch: variable 'x' has type Integer, cannot assign String"]);
    }

    #[test]
    fn test_undeclared_variable_is_rejected() {
        let result = run("count;");
        assert_eq!(result.errors, vec!["Variable 'count' not found"]);
    }

    #[test]
    fn test_undeclared_variable_suggests_closest_name() {
        let result = run("let count = 1; coutn;");
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Variable 'coutn' not found"));
    }

    #[test]
    fn test_duplicate_declaration_is_rejected() {
        let result = run("let x = 1; let x = 2;");
        assert_eq!(result.errors, vec!["Variable 'x' already declared in this scope"]);
    }

    #[test]
    fn test_arity_mismatch_is_rejected() {
        let result = run("fn add(a, b) { return a + b; } add(1, 2, 3);");
        assert_eq!(result.errors, vec!["Function 'add' expected 2 arguments but got 3"]);
    }

    #[test]
    fn test_if_else_branches() {
        assert_eq!(run("let r = 0; if true { r = 1; } else { r = 2; } r;").last_value, Some(Value::Integer(1)));
        assert_eq!(run("let r = 0; if false { r = 1; } else { r = 2; } r;").last_value, Some(Value::Integer(2)));
    }

    #[test]
    fn test_user_function_call_and_return() {
        let result = run("fn add(a, b) { return a + b; } add(2, 3);");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Integer(5)));
    }

    #[test]
    fn test_recursive_function() {
        let result = run(
            "fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(10);",
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Integer(3628800)));
    }

    #[test]
    fn test_mutual_recursion() {
        let result = run(
            "fn is_even(n) { if n == 0 { return true; } else { return is_odd(n - 1); } } \
             fn is_odd(n) { if n == 0 { return false; } else { return is_even(n - 1); } } \
             is_even(10);",
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Boolean(true)));
    }

    #[test]
    fn test_unbounded_recursion_is_reported_once_without_crashing() {
        let result = run("fn boom(n) { return boom(n + 1); } boom(0);");
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Stack overflow"));
    }

    #[test]
    fn test_typed_parameters_produce_no_fallback_warning() {
        let result = run("fn add(a: Int, b: Int) { return a + b; } add(2, 3);");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn test_untyped_parameters_produce_a_fallback_warning() {
        let result = run("fn add(a, b) { return a + b; } add(2, 3);");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(!result.warnings.is_empty(), "expected an Any-fallback warning, got none");
    }

    #[test]
    fn test_typed_parameter_mismatch_is_rejected() {
        // Passing a String where the parameter is typed Int should be
        // caught as a static type mismatch, not run.
        let result = run(r#"fn add(a: Int, b: Int) { return a + b; } add("x", 3);"#);
        assert_eq!(result.errors, vec!["Type mismatch: argument 1 to 'add' expected Integer but got String"]);
    }

    #[test]
    fn test_nested_function_does_not_capture_enclosing_locals() {
        // Documented Phase 1 limitation (design doc §9): a nested `fn` gets
        // its own frame and cannot see its enclosing function's locals.
        let result = run("fn outer() { let x = 1; fn inner() { return x; } return inner(); } outer();");
        assert_eq!(result.errors, vec!["Variable 'x' not found"]);
    }

    #[test]
    fn test_nested_function_sees_globals() {
        let result = run("let g = 41; fn outer() { fn inner() { return g + 1; } return inner(); } outer();");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.last_value, Some(Value::Integer(42)));
    }
}
