//! # AST → IR Lowering
//!
//! Structurally the sibling of `bytecode/compiler.rs`: the same infallible
//! walk over the Resolver-annotated AST, emitting three-address instructions
//! into basic blocks instead of bytes into a chunk. Control flow (`if`,
//! `&&`, `||`) becomes explicit `Branch`/`Jump` edges; statement-level
//! results are simply unused registers (which is what gives DCE something
//! to delete).
//!
//! Block creation order guarantees every CFG edge targets a *higher* block
//! id — Arc has no loops, so CFGs are DAGs and the bytecode lowering can rely
//! on forward-only jumps. (`// CFG is a DAG today` — re-audit when `while`
//! lands.)

use crate::ast::resolver::ResolvedBinding;
use crate::ast::types::{DataType, Value};
use crate::ast::{
    Ast, ASTBinaryExpression, ASTBinaryOperatorKind, ASTExpression, ASTExpressionKind,
    ASTFunctionCallExpression, ASTFunctionDeclaration, ASTIfStatement, ASTStatement,
    ASTStatementKind, ASTUnaryOperatorKind,
};

use super::instr::{Block, BlockId, InstrKind, Instruction, IrFunction, IrProgram, Reg, Terminator};

/// One function's blocks while they're being built (terminators arrive when
/// a block is sealed).
struct FnBuilder {
    name: String,
    frame_size: u32,
    blocks: Vec<PendingBlock>,
    current: usize,
    next_reg: u32,
}

struct PendingBlock {
    instrs: Vec<Instruction>,
    terminator: Option<(Terminator, u32)>,
}

pub struct IrLowering {
    functions: Vec<Option<IrFunction>>,
    stack: Vec<FnBuilder>,
    current_offset: u32,
}

impl IrLowering {
    pub fn new(function_count: u32) -> Self {
        Self {
            functions: (0..function_count).map(|_| None).collect(),
            stack: Vec::new(),
            current_offset: 0,
        }
    }

    pub fn lower(mut self, ast: &Ast) -> IrProgram {
        self.push_function("script".to_string(), 0);
        self.lower_body(&ast.statements);
        let script = self.pop_function();
        IrProgram {
            script,
            functions: self
                .functions
                .into_iter()
                .map(|f| f.expect("Resolver guarantees every FunctionId has a declaration"))
                .collect(),
        }
    }

    // -- builder plumbing -----------------------------------------------------

    fn push_function(&mut self, name: String, frame_size: u32) {
        self.stack.push(FnBuilder {
            name,
            frame_size,
            blocks: vec![PendingBlock { instrs: Vec::new(), terminator: None }],
            current: 0,
            next_reg: 0,
        });
    }

    fn pop_function(&mut self) -> IrFunction {
        let mut builder = self.stack.pop().expect("function builder was pushed");
        // Any still-unsealed block at body end (detached unreachable code,
        // or a join both of whose arms returned) gets a synthetic
        // `return Unit` so the type is total — unreachable-block
        // elimination drops the dead ones.
        let offset = self.current_offset;
        for block in &mut builder.blocks {
            if block.terminator.is_none() {
                let unit = Reg(builder.next_reg);
                builder.next_reg += 1;
                block.instrs.push(Instruction { kind: InstrKind::Const { dst: unit, value: Value::Unit }, offset });
                block.terminator = Some((Terminator::Return(unit), offset));
            }
        }
        IrFunction {
            name: builder.name,
            frame_size: builder.frame_size,
            reg_count: builder.next_reg,
            blocks: builder
                .blocks
                .into_iter()
                .map(|b| {
                    let (terminator, term_offset) = b.terminator.expect("every block is sealed");
                    Block { instrs: b.instrs, terminator, term_offset, dead: false }
                })
                .collect(),
        }
    }

    fn builder(&mut self) -> &mut FnBuilder {
        self.stack.last_mut().expect("always lowering inside some function")
    }

    fn fresh_reg(&mut self) -> Reg {
        let b = self.builder();
        let r = Reg(b.next_reg);
        b.next_reg += 1;
        r
    }

    /// Creates a new (empty, unsealed) block and returns its id. Does not
    /// switch to it.
    fn new_block(&mut self) -> BlockId {
        let b = self.builder();
        b.blocks.push(PendingBlock { instrs: Vec::new(), terminator: None });
        BlockId((b.blocks.len() - 1) as u32)
    }

    fn switch_to(&mut self, block: BlockId) {
        self.builder().current = block.0 as usize;
    }

    fn emit(&mut self, kind: InstrKind) {
        let offset = self.current_offset;
        let b = self.builder();
        let current = b.current;
        debug_assert!(b.blocks[current].terminator.is_none(), "emitting into a sealed block");
        b.blocks[current].instrs.push(Instruction { kind, offset });
    }

    fn emit_const(&mut self, value: Value) -> Reg {
        let dst = self.fresh_reg();
        self.emit(InstrKind::Const { dst, value });
        dst
    }

    /// Seals the current block with `terminator`. If the block is already
    /// sealed (statements after a `return`), the terminator is dropped —
    /// that code is unreachable and lowering into a fresh floating block
    /// keeps it out of the CFG entirely.
    fn seal(&mut self, terminator: Terminator) {
        let offset = self.current_offset;
        let b = self.builder();
        let current = b.current;
        if b.blocks[current].terminator.is_none() {
            b.blocks[current].terminator = Some((terminator, offset));
        }
    }

    fn is_sealed(&mut self) -> bool {
        let b = self.builder();
        b.blocks[b.current].terminator.is_some()
    }

    // -- statements -------------------------------------------------------------

    /// Lowers a function/script body with the Phase 2 §4 implicit-return
    /// rule: a trailing expression statement's register is returned;
    /// anything else returns a `Unit` const.
    fn lower_body(&mut self, statements: &[ASTStatement]) {
        let Some((last, rest)) = statements.split_last() else {
            let unit = self.emit_const(Value::Unit);
            self.seal(Terminator::Return(unit));
            return;
        };

        for stmt in rest {
            self.lower_statement(stmt);
        }

        // A trailing statement after a `return` is unreachable: divert it
        // into a detached block, same as lower_statement does.
        if self.is_sealed() {
            let detached = self.new_block();
            self.switch_to(detached);
        }

        if let ASTStatementKind::Expression(expr) = &last.kind {
            let result = self.lower_expression(expr);
            self.seal(Terminator::Return(result));
        } else {
            self.lower_statement(last);
            if !self.is_sealed() {
                let unit = self.emit_const(Value::Unit);
                self.seal(Terminator::Return(unit));
            }
        }
    }

    fn lower_statements(&mut self, statements: &[ASTStatement]) {
        for stmt in statements {
            self.lower_statement(stmt);
        }
    }

    fn lower_statement(&mut self, stmt: &ASTStatement) {
        // Unreachable code after a `return` in the same body: lower it into
        // a detached block (no predecessors) rather than corrupting the
        // sealed one; unreachable-block elimination will drop it.
        if self.is_sealed() {
            let detached = self.new_block();
            self.switch_to(detached);
        }
        match &stmt.kind {
            ASTStatementKind::Expression(expr) => {
                self.lower_expression(expr); // result register goes unused
            }
            ASTStatementKind::VariableDeclaration(decl) => {
                let value = self.lower_expression(&decl.initializer);
                if let Some(token) = &decl.name_span {
                    self.current_offset = token.span.start as u32;
                }
                let binding = decl.binding.get().expect("Resolver guarantees every declaration is resolved");
                self.emit_store(binding, value);
            }
            ASTStatementKind::Assignment(assign) => {
                let mut value = self.lower_expression(&assign.value);
                if let Some(token) = &assign.name_span {
                    self.current_offset = token.span.start as u32;
                }
                if assign.needs_float_widen.get() {
                    let widened = self.fresh_reg();
                    self.emit(InstrKind::ToFloat { dst: widened, src: value });
                    value = widened;
                }
                let binding = assign.binding.get().expect("Resolver guarantees every assignment target is resolved");
                self.emit_store(binding, value);
            }
            ASTStatementKind::Block(statements) => self.lower_statements(statements),
            ASTStatementKind::IfStatement(if_stmt) => self.lower_if(if_stmt),
            ASTStatementKind::FunctionDeclaration(func) => self.lower_function(func),
            ASTStatementKind::ReturnStatement(ret) => {
                let value = self.lower_expression(&ret.value);
                self.seal(Terminator::Return(value));
            }
        }
    }

    fn lower_if(&mut self, if_stmt: &ASTIfStatement) {
        let cond = self.lower_expression(&if_stmt.condition);
        let then_block = self.new_block();
        let else_block = self.new_block();
        self.seal(Terminator::Branch { cond, then_block, else_block });

        self.switch_to(then_block);
        self.lower_statements(&if_stmt.then_branch);
        let then_end_sealed = self.is_sealed();
        let then_end = BlockId(self.builder().current as u32);

        self.switch_to(else_block);
        if let Some(else_branch) = &if_stmt.else_branch {
            self.lower_statements(else_branch);
        }
        let else_end_sealed = self.is_sealed();
        let else_end = BlockId(self.builder().current as u32);

        let join = self.new_block();
        if !then_end_sealed {
            self.switch_to(then_end);
            self.seal(Terminator::Jump(join));
        }
        if !else_end_sealed {
            self.switch_to(else_end);
            self.seal(Terminator::Jump(join));
        }
        self.switch_to(join);
    }

    fn lower_function(&mut self, func: &ASTFunctionDeclaration) {
        let id = func.function_id.get().expect("Resolver assigns every function an id");
        let frame_size = func.frame_size.get().expect("Resolver computes every function's frame size");
        // Parameter slots are 0..arity in order (asserted in the bytecode
        // compiler); IR functions receive arguments directly in those slots,
        // same contract as the VM.
        let saved_offset = self.current_offset;
        self.push_function(func.name.clone(), frame_size);
        self.lower_body(&func.body);
        let function = self.pop_function();
        self.functions[id.0 as usize] = Some(function);
        self.current_offset = saved_offset;
    }

    fn emit_store(&mut self, binding: ResolvedBinding, src: Reg) {
        match binding {
            ResolvedBinding::Local(slot) => self.emit(InstrKind::StoreLocal { slot, src }),
            ResolvedBinding::Global(slot) => self.emit(InstrKind::StoreGlobal { slot, src }),
        }
    }

    fn emit_load(&mut self, binding: ResolvedBinding) -> Reg {
        let dst = self.fresh_reg();
        match binding {
            ResolvedBinding::Local(slot) => self.emit(InstrKind::LoadLocal { dst, slot }),
            ResolvedBinding::Global(slot) => self.emit(InstrKind::LoadGlobal { dst, slot }),
        }
        dst
    }

    // -- expressions --------------------------------------------------------------

    fn lower_expression(&mut self, expr: &ASTExpression) -> Reg {
        match &expr.kind {
            ASTExpressionKind::Number(n) => self.emit_const(n.value.clone()),
            ASTExpressionKind::Paranthesized(p) => self.lower_expression(&p.expression),
            ASTExpressionKind::Identifier(ident) => {
                if let Some(token) = &ident.token {
                    self.current_offset = token.span.start as u32;
                }
                let binding = ident.binding.get().expect("Resolver guarantees every identifier is resolved");
                self.emit_load(binding)
            }
            ASTExpressionKind::Binary(bin) => self.lower_binary(bin),
            ASTExpressionKind::Unary(u) => {
                self.current_offset = u.operator.token.span.start as u32;
                match u.operator.kind {
                    ASTUnaryOperatorKind::Increment | ASTUnaryOperatorKind::Decrement => {
                        let is_inc = matches!(u.operator.kind, ASTUnaryOperatorKind::Increment);
                        self.lower_inc_dec(&u.operand, is_inc, false)
                    }
                    // Runtime passthrough — nothing to emit.
                    ASTUnaryOperatorKind::Plus => self.lower_expression(&u.operand),
                    ASTUnaryOperatorKind::Minus => {
                        let src = self.lower_expression(&u.operand);
                        let can_error = !is_static_numeric(&u.operand);
                        let dst = self.fresh_reg();
                        self.current_offset = u.operator.token.span.start as u32;
                        self.emit(InstrKind::Negate { dst, src, can_error });
                        dst
                    }
                    ASTUnaryOperatorKind::LogicalNot => {
                        let src = self.lower_expression(&u.operand);
                        let dst = self.fresh_reg();
                        self.current_offset = u.operator.token.span.start as u32;
                        self.emit(InstrKind::Not { dst, src });
                        dst
                    }
                }
            }
            ASTExpressionKind::PostfixUnary(p) => {
                self.current_offset = p.operator.token.span.start as u32;
                let is_inc = matches!(p.operator.kind, ASTUnaryOperatorKind::Increment);
                self.lower_inc_dec(&p.operand, is_inc, true)
            }
            ASTExpressionKind::FunctionCall(call) => self.lower_call(call),
        }
    }

    fn lower_binary(&mut self, bin: &ASTBinaryExpression) -> Reg {
        let op_offset = bin.operator.token.span.start as u32;
        match bin.operator.kind {
            // Short-circuit: the right operand only evaluates in its arm.
            // The result register is written in both arms (the IR's one
            // deliberate deviation from single-assignment) and holds a real
            // Boolean, matching both existing backends' coercion.
            ASTBinaryOperatorKind::LogicalAnd | ASTBinaryOperatorKind::LogicalOr => {
                let is_and = matches!(bin.operator.kind, ASTBinaryOperatorKind::LogicalAnd);
                let lhs = self.lower_expression(&bin.left);
                self.current_offset = op_offset;
                let result = self.fresh_reg();
                let rhs_block = self.new_block();
                let short_block = self.new_block();
                let (then_block, else_block) = if is_and {
                    (rhs_block, short_block) // truthy lhs -> evaluate rhs
                } else {
                    (short_block, rhs_block) // truthy lhs -> short-circuit true
                };
                self.seal(Terminator::Branch { cond: lhs, then_block, else_block });

                self.switch_to(rhs_block);
                let rhs = self.lower_expression(&bin.right);
                self.current_offset = op_offset;
                self.emit(InstrKind::ToBool { dst: result, src: rhs });
                let rhs_end = BlockId(self.builder().current as u32);

                self.switch_to(short_block);
                self.emit(InstrKind::Const { dst: result, value: Value::Boolean(!is_and) });

                let join = self.new_block();
                self.seal(Terminator::Jump(join));
                self.switch_to(rhs_end);
                self.seal(Terminator::Jump(join));
                self.switch_to(join);
                result
            }
            op => {
                let lhs = self.lower_expression(&bin.left);
                let rhs = self.lower_expression(&bin.right);
                let can_error = binary_can_error(op, &bin.left, &bin.right);
                let dst = self.fresh_reg();
                self.current_offset = op_offset;
                self.emit(InstrKind::Binary { dst, op, lhs, rhs, can_error });
                dst
            }
        }
    }

    /// Same desugaring as the bytecode compiler: load, `+`/`-` 1, store;
    /// prefix yields the new value, postfix the original.
    fn lower_inc_dec(&mut self, operand: &ASTExpression, is_inc: bool, is_postfix: bool) -> Reg {
        let ASTExpressionKind::Identifier(ident) = &operand.kind else {
            unreachable!("Resolver guarantees increment/decrement operand is an identifier");
        };
        let binding = ident.binding.get().expect("Resolver guarantees this identifier is resolved");

        let old = self.emit_load(binding);
        let one = self.emit_const(Value::Integer(1));
        let new = self.fresh_reg();
        let op = if is_inc { ASTBinaryOperatorKind::Plus } else { ASTBinaryOperatorKind::Minus };
        let can_error = !is_static_numeric(operand);
        self.emit(InstrKind::Binary { dst: new, op, lhs: old, rhs: one, can_error });
        self.emit_store(binding, new);
        if is_postfix {
            old
        } else {
            new
        }
    }

    fn lower_call(&mut self, call: &ASTFunctionCallExpression) -> Reg {
        if let Some(token) = &call.token {
            self.current_offset = token.span.start as u32;
        }
        let call_offset = self.current_offset;
        let args: Vec<Reg> = call.arguments.iter().map(|arg| self.lower_expression(arg)).collect();
        let dst = self.fresh_reg();
        self.current_offset = call_offset;
        match call.resolved_call.get().expect("Resolver guarantees every call is resolved") {
            crate::ast::resolver::ResolvedCall::User(id) => {
                self.emit(InstrKind::Call { dst, function: id.0, args });
            }
            crate::ast::resolver::ResolvedCall::Builtin(builtin) => {
                self.emit(InstrKind::CallBuiltin { dst, builtin, args });
            }
        }
        dst
    }
}

/// `true` iff the expression's Resolver-inferred type is statically numeric
/// — the operand-side half of the shared can-this-error predicate. An
/// unresolved or `Any` type means runtime coercion can fail.
fn is_static_numeric(expr: &ASTExpression) -> bool {
    matches!(expr.resolved_type.get(), Some(DataType::Integer) | Some(DataType::Float))
}

fn is_static_concrete(expr: &ASTExpression) -> bool {
    !matches!(expr.resolved_type.get(), None | Some(DataType::Any))
}

/// The shared purity fact (design doc §5.2): a binary op can raise a runtime
/// error iff it's inherently partial (`/`/`%` by zero, `**` overflow) or
/// either operand is dynamically typed. Statically-typed operands of any
/// other op already passed the Resolver's legality check and cannot fail.
fn binary_can_error(op: ASTBinaryOperatorKind, left: &ASTExpression, right: &ASTExpression) -> bool {
    use ASTBinaryOperatorKind::*;
    matches!(op, Divide | Modulo | Exponentiation)
        || !is_static_concrete(left)
        || !is_static_concrete(right)
}
