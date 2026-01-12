use crate::ast::{ASTVisitor, ASTBinaryExpression, ASTNumberExpression, ASTBinaryOperatorKind, ASTUnaryExpression, ASTUnaryOperatorKind};
use crate::ast::types::Value;


pub struct ASTEvaluator {
    pub last_value: Option<Value>,
    pub errors: Vec<String>,
}

impl ASTEvaluator {
    pub fn new() -> Self {
        Self { 
            last_value: None,
            errors: Vec::new(),
        }
    }

    fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
}

impl ASTVisitor for ASTEvaluator {
    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.last_value = Some(number.value.clone());
    }

    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression) {
        // Handle short-circuit evaluation for logical operators
        match expr.operator.kind {
            ASTBinaryOperatorKind::LogicalAnd => {
                // Short-circuit: if left is false, don't evaluate right
                self.visit_expression(&expr.left);
                let left = match &self.last_value {
                    Some(v) => v.clone(),
                    None => return,
                };
                
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
                            if b == 0 {
                                self.add_error("Division by zero".to_string());
                                None
                            } else {
                                Some(Value::Integer(a / b))
                            }
                        },
                        (Value::Float(a), Value::Float(b)) => {
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
        };
    }
}