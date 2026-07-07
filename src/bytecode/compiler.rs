//! # Bytecode Compiler
//!
//! Lowers a Resolver-annotated AST into a [`CompiledProgram`] of [`Chunk`]s.
//! No execution happens here — see the Phase 2 design doc
//! (`local-notes/phase2-bytecode-design.md`) for the full rationale.
//!
//! Like the evaluator, this is infallible: it trusts the Resolver's
//! guarantees completely and reads `SlotIndex`/`FunctionId`/`ResolvedBinding`/
//! `ResolvedCall` straight off the AST's `Cell` fields instead of
//! re-deriving scope.

use crate::ast::resolver::{BuiltinFn, ResolvedBinding, ResolvedCall};
use crate::ast::types::Value;
use crate::ast::{
    Ast, ASTBinaryExpression, ASTBinaryOperatorKind, ASTExpression, ASTExpressionKind,
    ASTFunctionCallExpression, ASTFunctionDeclaration, ASTIfStatement, ASTPostfixUnaryExpression,
    ASTStatement, ASTStatementKind, ASTUnaryExpression, ASTUnaryOperatorKind,
};
use crate::bytecode::chunk::Chunk;
use crate::bytecode::opcode::OpCode;

/// The output of a compile pass: the top-level "script" chunk, plus one
/// compiled function per `FunctionId`, indexed by `FunctionId.0`.
pub struct CompiledProgram {
    pub script: Chunk,
    pub functions: Vec<CompiledFunction>,
    /// Local slots the *script* frame needs. Zero for directly-compiled
    /// programs (script variables are globals); non-zero when the IR
    /// pipeline spills the script's virtual registers to frame slots.
    pub script_frame_size: u32,
}

/// A function's chunk plus the metadata the VM needs to call it. No arity
/// field: the `Call` opcode carries its own argument count and the Resolver
/// already rejected arity mismatches statically.
pub struct CompiledFunction {
    pub name: String,
    pub frame_size: u32,
    pub chunk: Chunk,
}

impl CompiledFunction {
    fn placeholder() -> Self {
        Self { name: String::new(), frame_size: 0, chunk: Chunk::new() }
    }
}

pub struct BytecodeCompiler {
    functions: Vec<CompiledFunction>,
    /// In-progress chunks; the top is the one currently being written to.
    /// Depth > 1 only while compiling a function nested inside another.
    chunk_stack: Vec<Chunk>,
    /// Source byte-offset of the most recently seen token, reused for AST
    /// nodes that don't carry their own (e.g. number literals) so
    /// disassembly still points somewhere close to the right place.
    current_offset: u32,
}

impl BytecodeCompiler {
    pub fn new(function_count: u32) -> Self {
        Self {
            functions: (0..function_count).map(|_| CompiledFunction::placeholder()).collect(),
            chunk_stack: Vec::new(),
            current_offset: 0,
        }
    }

    pub fn compile(mut self, ast: &Ast) -> CompiledProgram {
        self.chunk_stack.push(Chunk::new());
        self.compile_body(&ast.statements);
        let script = self.chunk_stack.pop().expect("script chunk was just pushed");
        CompiledProgram { script, functions: self.functions, script_frame_size: 0 }
    }

    fn current(&mut self) -> &mut Chunk {
        self.chunk_stack.last_mut().expect("always compiling inside some chunk")
    }

    fn emit(&mut self, op: OpCode, offset: u32) {
        self.current().write_op(op, offset);
    }

    fn emit_constant(&mut self, value: Value, offset: u32) {
        let idx = self.current().add_constant(value);
        self.current().write_op(OpCode::Constant, offset);
        self.current().write_u16(idx, offset);
    }

    fn slot_u16(index: u32) -> u16 {
        u16::try_from(index).expect("slot index exceeded u16::MAX")
    }

    fn emit_get(&mut self, binding: ResolvedBinding, offset: u32) {
        let (op, slot) = match binding {
            ResolvedBinding::Local(s) => (OpCode::GetLocal, s.0),
            ResolvedBinding::Global(s) => (OpCode::GetGlobal, s.0),
        };
        self.current().write_op(op, offset);
        self.current().write_u16(Self::slot_u16(slot), offset);
    }

    fn emit_set(&mut self, binding: ResolvedBinding, offset: u32) {
        let (op, slot) = match binding {
            ResolvedBinding::Local(s) => (OpCode::SetLocal, s.0),
            ResolvedBinding::Global(s) => (OpCode::SetGlobal, s.0),
        };
        self.current().write_op(op, offset);
        self.current().write_u16(Self::slot_u16(slot), offset);
    }

    /// Pushed where there's genuinely no value to give: an implicit return
    /// with no trailing expression statement (design doc §4). `Unit` (not
    /// the Phase 2 `Integer(0)` placeholder) so misusing the "result" is a
    /// runtime error in the VM, matching the tree-walker's behavior.
    fn emit_placeholder(&mut self, offset: u32) {
        self.emit_constant(Value::Unit, offset);
    }

    // -- statements -----------------------------------------------------------

    /// Compiles a function/script body: every statement is stack-neutral
    /// except (optionally) the last. If the last statement is an expression
    /// statement, its value becomes the implicit return (no trailing `POP`);
    /// otherwise a placeholder is pushed. Either way, always ends in
    /// `OP_RETURN` — every chunk needs one, explicit `return` or not (see
    /// design doc §4).
    fn compile_body(&mut self, statements: &[ASTStatement]) {
        let Some((last, rest)) = statements.split_last() else {
            let offset = self.current_offset;
            self.emit_placeholder(offset);
            self.emit(OpCode::Return, offset);
            return;
        };

        for stmt in rest {
            self.compile_statement(stmt);
        }

        if let ASTStatementKind::Expression(expr) = &last.kind {
            self.compile_expression(expr);
        } else {
            self.compile_statement(last);
            self.emit_placeholder(self.current_offset);
        }
        self.emit(OpCode::Return, self.current_offset);
    }

    fn compile_statements(&mut self, statements: &[ASTStatement]) {
        for stmt in statements {
            self.compile_statement(stmt);
        }
    }

    fn compile_statement(&mut self, stmt: &ASTStatement) {
        match &stmt.kind {
            ASTStatementKind::Expression(expr) => {
                self.compile_expression(expr);
                self.emit(OpCode::Pop, self.current_offset);
            }
            ASTStatementKind::VariableDeclaration(decl) => {
                self.compile_expression(&decl.initializer);
                if let Some(token) = &decl.name_span {
                    self.current_offset = token.span.start as u32;
                }
                let binding = decl.binding.get().expect("Resolver guarantees every declaration is resolved");
                self.emit_set(binding, self.current_offset);
                self.emit(OpCode::Pop, self.current_offset);
            }
            ASTStatementKind::Assignment(assign) => {
                self.compile_expression(&assign.value);
                if let Some(token) = &assign.name_span {
                    self.current_offset = token.span.start as u32;
                }
                if assign.needs_float_widen.get() {
                    self.emit(OpCode::ToFloat, self.current_offset);
                }
                let binding = assign.binding.get().expect("Resolver guarantees every assignment target is resolved");
                self.emit_set(binding, self.current_offset);
                self.emit(OpCode::Pop, self.current_offset);
            }
            ASTStatementKind::Block(statements) => self.compile_statements(statements),
            ASTStatementKind::IfStatement(if_stmt) => self.compile_if(if_stmt),
            ASTStatementKind::FunctionDeclaration(func) => self.compile_function(func),
            ASTStatementKind::ReturnStatement(ret) => {
                self.compile_expression(&ret.value);
                self.emit(OpCode::Return, self.current_offset);
            }
        }
    }

    fn compile_if(&mut self, if_stmt: &ASTIfStatement) {
        self.compile_expression(&if_stmt.condition);
        let offset = self.current_offset;

        let then_jump = self.current().emit_jump(OpCode::JumpIfFalse, offset);
        self.emit(OpCode::Pop, offset);
        self.compile_statements(&if_stmt.then_branch);
        let else_jump = self.current().emit_jump(OpCode::Jump, offset);

        self.current().patch_jump(then_jump);
        self.emit(OpCode::Pop, offset);
        if let Some(else_branch) = &if_stmt.else_branch {
            self.compile_statements(else_branch);
        }

        self.current().patch_jump(else_jump);
    }

    fn compile_function(&mut self, func: &ASTFunctionDeclaration) {
        let id = func.function_id.get().expect("Resolver assigns every function an id");

        // The VM pops arguments straight into frame slots 0..arity — valid
        // because the Resolver declares parameters first in a fresh frame,
        // so their slots are always 0, 1, 2, ... in order.
        for (i, param) in func.parameters.iter().enumerate() {
            let slot = param.slot.get().expect("Resolver assigns every parameter a slot");
            debug_assert_eq!(slot.0 as usize, i, "parameter slots must be sequential from 0");
        }

        self.chunk_stack.push(Chunk::new());
        self.compile_body(&func.body);
        let chunk = self.chunk_stack.pop().expect("function chunk was just pushed");
        self.functions[id.0 as usize] = CompiledFunction {
            name: func.name.clone(),
            frame_size: func.frame_size.get().expect("Resolver computes every function's frame size"),
            chunk,
        };
    }

    // -- expressions ------------------------------------------------------------

    fn compile_expression(&mut self, expr: &ASTExpression) {
        match &expr.kind {
            ASTExpressionKind::Number(n) => {
                let offset = self.current_offset;
                match n.value {
                    Value::Boolean(true) => self.emit(OpCode::True, offset),
                    Value::Boolean(false) => self.emit(OpCode::False, offset),
                    ref v => self.emit_constant(v.clone(), offset),
                }
            }
            ASTExpressionKind::Binary(bin) => self.compile_binary(bin),
            ASTExpressionKind::Paranthesized(p) => self.compile_expression(&p.expression),
            ASTExpressionKind::Unary(u) => self.compile_unary(u),
            ASTExpressionKind::PostfixUnary(p) => self.compile_postfix(p),
            ASTExpressionKind::Identifier(ident) => {
                if let Some(token) = &ident.token {
                    self.current_offset = token.span.start as u32;
                }
                let binding = ident.binding.get().expect("Resolver guarantees every identifier is resolved");
                self.emit_get(binding, self.current_offset);
            }
            ASTExpressionKind::FunctionCall(call) => self.compile_call(call),
        }
    }

    fn binary_opcode(kind: &ASTBinaryOperatorKind) -> OpCode {
        use ASTBinaryOperatorKind::*;
        match kind {
            Plus => OpCode::Add,
            Minus => OpCode::Subtract,
            Multiply => OpCode::Multiply,
            Divide => OpCode::Divide,
            Modulo => OpCode::Modulo,
            Exponentiation => OpCode::Power,
            BitwiseAnd => OpCode::BitAnd,
            BitwiseOr => OpCode::BitOr,
            BitwiseXor => OpCode::BitXor,
            LeftShift => OpCode::ShiftLeft,
            RightShift => OpCode::ShiftRight,
            Equal => OpCode::Equal,
            NotEqual => OpCode::NotEqual,
            Less => OpCode::Less,
            Greater => OpCode::Greater,
            LessEqual => OpCode::LessEqual,
            GreaterEqual => OpCode::GreaterEqual,
            LogicalAnd | LogicalOr => unreachable!("handled directly in compile_binary"),
        }
    }

    /// `&&`/`||` compile to jumps + double-`NOT` rather than dedicated
    /// opcodes: Arc's logical operators always coerce their result to a real
    /// `Boolean` (unlike Lox-style "return whichever operand"), and `NOT NOT`
    /// performs exactly that coercion on whichever value survives on the
    /// stack. See design doc §3.
    fn compile_binary(&mut self, bin: &ASTBinaryExpression) {
        self.current_offset = bin.operator.token.span.start as u32;
        let offset = self.current_offset;

        match bin.operator.kind {
            ASTBinaryOperatorKind::LogicalAnd => {
                self.compile_expression(&bin.left);
                let end_jump = self.current().emit_jump(OpCode::JumpIfFalse, offset);
                self.emit(OpCode::Pop, offset);
                self.compile_expression(&bin.right);
                self.current().patch_jump(end_jump);
                self.emit(OpCode::Not, offset);
                self.emit(OpCode::Not, offset);
            }
            ASTBinaryOperatorKind::LogicalOr => {
                self.compile_expression(&bin.left);
                let else_jump = self.current().emit_jump(OpCode::JumpIfFalse, offset);
                let end_jump = self.current().emit_jump(OpCode::Jump, offset);
                self.current().patch_jump(else_jump);
                self.emit(OpCode::Pop, offset);
                self.compile_expression(&bin.right);
                self.current().patch_jump(end_jump);
                self.emit(OpCode::Not, offset);
                self.emit(OpCode::Not, offset);
            }
            ref kind => {
                self.compile_expression(&bin.left);
                self.compile_expression(&bin.right);
                self.emit(Self::binary_opcode(kind), offset);
            }
        }
    }

    fn compile_unary(&mut self, u: &ASTUnaryExpression) {
        self.current_offset = u.operator.token.span.start as u32;
        let offset = self.current_offset;
        match u.operator.kind {
            ASTUnaryOperatorKind::Increment | ASTUnaryOperatorKind::Decrement => {
                self.compile_inc_dec(&u.operand, matches!(u.operator.kind, ASTUnaryOperatorKind::Increment), false, offset);
            }
            // Runtime passthrough: valid for every type, never errors, so
            // there's nothing to emit at all.
            ASTUnaryOperatorKind::Plus => self.compile_expression(&u.operand),
            ASTUnaryOperatorKind::Minus => {
                self.compile_expression(&u.operand);
                self.emit(OpCode::Negate, offset);
            }
            ASTUnaryOperatorKind::LogicalNot => {
                self.compile_expression(&u.operand);
                self.emit(OpCode::Not, offset);
            }
        }
    }

    fn compile_postfix(&mut self, p: &ASTPostfixUnaryExpression) {
        self.current_offset = p.operator.token.span.start as u32;
        let offset = self.current_offset;
        let is_increment = matches!(p.operator.kind, ASTUnaryOperatorKind::Increment);
        self.compile_inc_dec(&p.operand, is_increment, true, offset);
    }

    /// Desugars `++x`/`--x`/`x++`/`x--` into `GET`/`CONSTANT 1`/`ADD`-or-
    /// `SUBTRACT`/`SET` (plus a `POP` for postfix, to discard the new value
    /// and keep the old one), reusing the generic runtime arithmetic instead
    /// of dedicated opcodes.
    ///
    /// ponytail: this means a String/Boolean value flowing through an
    /// untyped (`Any`) variable that gets incremented will error as "Cannot
    /// add" rather than "Cannot increment" once a VM exists. Every *typed*
    /// case is already rejected at resolve time, so only that narrow
    /// dynamic-fallback sliver is affected — an accepted trade-off, not an
    /// oversight (design doc §3).
    fn compile_inc_dec(&mut self, operand: &ASTExpression, is_increment: bool, is_postfix: bool, offset: u32) {
        let ASTExpressionKind::Identifier(ident) = &operand.kind else {
            unreachable!("Resolver guarantees increment/decrement operand is an identifier");
        };
        let binding = ident.binding.get().expect("Resolver guarantees this identifier is resolved");

        if is_postfix {
            self.emit_get(binding, offset); // old value: kept as the final result
        }
        self.emit_get(binding, offset);
        self.emit_constant(Value::Integer(1), offset);
        self.emit(if is_increment { OpCode::Add } else { OpCode::Subtract }, offset);
        self.emit_set(binding, offset); // leaves the new value on the stack
        if is_postfix {
            self.emit(OpCode::Pop, offset); // discard new value; old value remains
        }
    }

    fn compile_call(&mut self, call: &ASTFunctionCallExpression) {
        if let Some(token) = &call.token {
            self.current_offset = token.span.start as u32;
        }
        let offset = self.current_offset;

        for arg in &call.arguments {
            self.compile_expression(arg);
        }
        let argc = u8::try_from(call.arguments.len()).expect("more than 255 arguments in a single call");

        match call.resolved_call.get().expect("Resolver guarantees every call is resolved") {
            ResolvedCall::User(id) => {
                self.emit(OpCode::Call, offset);
                self.current().write_u16(Self::slot_u16(id.0), offset);
                self.current().write_u8(argc, offset);
            }
            ResolvedCall::Builtin(builtin) => {
                let op = match builtin {
                    BuiltinFn::Print => OpCode::Print,
                    BuiltinFn::Max => OpCode::Max,
                    BuiltinFn::Min => OpCode::Min,
                };
                self.emit(op, offset);
                self.current().write_u8(argc, offset);
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
    use crate::bytecode::disassembler::{disassemble_chunk, disassemble_program};

    /// Lexes, parses, resolves, and compiles a program, panicking on any
    /// parse/resolve error (these tests are about the bytecode compiler,
    /// not upstream diagnostics).
    fn compile(source: &str) -> CompiledProgram {
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
        assert!(
            parser.diagnostics.is_empty(),
            "parse errors: {:?}",
            parser.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );

        let mut resolver = Resolver::new();
        resolver.resolve(&ast);
        assert!(
            !resolver.has_errors(),
            "resolve errors: {:?}",
            resolver.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );

        BytecodeCompiler::new(resolver.function_count()).compile(&ast)
    }

    fn op_lines(text: &str) -> Vec<&str> {
        text.lines().filter(|l| l.contains("OP_")).collect()
    }

    #[test]
    fn test_arithmetic_compiles_and_disassembles() {
        let program = compile("1 + 2 * 3;");
        let text = disassemble_chunk(&program.script, "script");
        assert!(text.contains("OP_CONSTANT"));
        assert!(text.contains("OP_MULTIPLY"));
        assert!(text.contains("OP_ADD"));
        assert!(text.contains("OP_RETURN"));
        // Last statement is an expression statement -> falls through to
        // RETURN with no trailing POP (design doc §4).
        assert!(!text.contains("OP_POP"));
    }

    #[test]
    fn test_if_else_compiles_with_jumps() {
        let program = compile(r#"let x = 1; if x > 0 { print("pos"); } else { print("neg"); }"#);
        let text = disassemble_chunk(&program.script, "script");
        assert!(text.contains("OP_JUMP_IF_FALSE"));
        assert!(text.contains("OP_JUMP "));
        assert!(text.contains("OP_GREATER"));
        assert!(text.contains("OP_PRINT"));
    }

    #[test]
    fn test_function_call_compiles_to_its_own_chunk() {
        let program = compile("fn add(a, b) { return a + b; } add(2, 3);");
        assert_eq!(program.functions.len(), 1);

        let script_text = disassemble_chunk(&program.script, "script");
        assert!(script_text.contains("OP_CALL"));
        assert!(script_text.contains("fn#0"));

        let fn_text = disassemble_chunk(&program.functions[0].chunk, "fn#0");
        assert!(fn_text.contains("OP_ADD"));
        assert!(fn_text.contains("OP_RETURN"));
    }

    #[test]
    fn test_recursive_function_calls_itself_by_id() {
        let program = compile("fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(5);");
        let fn_text = disassemble_chunk(&program.functions[0].chunk, "fn#0");
        assert!(fn_text.contains("OP_CALL"));
        assert!(fn_text.contains("fn#0")); // calls itself
        assert!(fn_text.contains("OP_MULTIPLY"));
    }

    #[test]
    fn test_int_to_float_widening_emits_to_float() {
        let program = compile("let x = 1.5; x = 2;");
        let text = disassemble_chunk(&program.script, "script");
        assert!(text.contains("OP_TO_FLOAT"));
    }

    #[test]
    fn test_golden_suite_compiles_without_panicking() {
        let programs = [
            "1 + 2 * 3;",
            "let x = 1; x = x + 1; x;",
            "let x = 1.5; x = 2; x + 0.5;",
            "true && false || !true;",
            r#"let x = 1; if x > 0 { print("pos"); } else { print("neg"); }"#,
            "fn add(a, b) { return a + b; } add(2, 3);",
            "fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(10);",
            "fn is_even(n) { if n == 0 { return true; } else { return is_odd(n - 1); } } \
             fn is_odd(n) { if n == 0 { return false; } else { return is_even(n - 1); } } \
             is_even(10);",
            "let counter = 10; ++counter; counter++; --counter; counter--;",
            "print(min(4, 2, 9, 1)); print(max(4, 2, 9, 1));",
        ];
        for source in programs {
            let program = compile(source);
            let text = disassemble_program(&program);
            assert!(text.contains("OP_RETURN"), "source: {}", source);
        }
    }

    #[test]
    fn test_print_of_print_stack_effect() {
        // The narrowest case Arc's grammar allows for a call's result being
        // consumed by another call: print(print("x")). The outer PRINT must
        // consume exactly the inner call's placeholder result — no extra
        // push should appear between the two PRINT instructions (design doc §5).
        let program = compile(r#"print(print("x"));"#);
        let text = disassemble_chunk(&program.script, "script");
        let lines = op_lines(&text);
        assert_eq!(lines.len(), 4, "{:#?}", lines);
        assert!(lines[0].contains("OP_CONSTANT"));
        assert!(lines[1].contains("OP_PRINT"));
        assert!(lines[2].contains("OP_PRINT"));
        assert!(lines[3].contains("OP_RETURN"));
    }

    #[test]
    fn test_implicit_return_from_trailing_expression_has_no_trailing_pop() {
        let program = compile("fn f() { 7; }");
        let text = disassemble_chunk(&program.functions[0].chunk, "fn#0");
        let lines = op_lines(&text);
        assert_eq!(lines.len(), 2, "{:#?}", lines);
        assert!(lines[0].contains("OP_CONSTANT"));
        assert!(lines[1].contains("OP_RETURN"));
    }

    #[test]
    fn test_implicit_return_from_trailing_declaration_pushes_placeholder() {
        let program = compile("fn f() { let x = 42; }");
        let text = disassemble_chunk(&program.functions[0].chunk, "fn#0");
        let lines = op_lines(&text);
        // CONSTANT 42, SET_LOCAL, POP (statement discard), CONSTANT 0 (placeholder), RETURN
        assert_eq!(lines.len(), 5, "{:#?}", lines);
        assert!(lines[3].contains("OP_CONSTANT"));
        assert!(lines[4].contains("OP_RETURN"));
    }
}
