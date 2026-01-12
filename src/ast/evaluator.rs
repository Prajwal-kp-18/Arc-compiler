use crate::ast::{ASTVisitor, ASTBinaryExpression, ASTNumberExpression, ASTBinaryOperatorKind, ASTUnaryExpression, ASTUnaryOperatorKind};


pub struct ASTEvaluator {
    pub last_value: Option<i64>,
}

impl ASTEvaluator {
    pub fn new() -> Self {
        Self { last_value: None }
    }
}

impl ASTVisitor for ASTEvaluator {
    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.last_value = Some(number.number);
    }

    fn visit_binary_expression(&mut self, expr: &ASTBinaryExpression) {
        self.visit_expression(&expr.left);
        let left = self.last_value.unwrap();
        self.visit_expression(&expr.right);
        let right = self.last_value.unwrap();

        self.last_value = match expr.operator.kind {
            ASTBinaryOperatorKind::Plus => Some(left + right),
            ASTBinaryOperatorKind::Minus => Some(left - right),
            ASTBinaryOperatorKind::Multiply => Some(left * right),
            ASTBinaryOperatorKind::Divide => Some(left / right),
            ASTBinaryOperatorKind::Modulo => Some(left % right),
            ASTBinaryOperatorKind::Exponentiation => Some(left.pow(right as u32)),
            ASTBinaryOperatorKind::BitwiseAnd => Some(left & right),
            ASTBinaryOperatorKind::BitwiseOr => Some(left | right),
            ASTBinaryOperatorKind::BitwiseXor => Some(left ^ right),
            ASTBinaryOperatorKind::LeftShift => Some(left << right),
            ASTBinaryOperatorKind::RightShift => Some(left >> right),
        };
    }

    fn visit_unary_expression(&mut self, unary_expr: &ASTUnaryExpression) {
        self.visit_expression(&unary_expr.operand);
        let operand = self.last_value.unwrap();
        
        self.last_value = match unary_expr.operator.kind {
            ASTUnaryOperatorKind::Plus => Some(operand),
            ASTUnaryOperatorKind::Minus => Some(-operand),
        };
    }
}