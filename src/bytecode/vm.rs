//! # Virtual Machine
//!
//! Phase 3: a stack-based VM executing the [`CompiledProgram`] chunks that
//! Phase 2 produces — the second, independently-verified execution backend
//! next to the tree-walking evaluator. Selected via `--backend=vm` on the
//! CLI; the golden suite (see `tests/golden.rs`) must produce identical
//! output on both backends.
//!
//! ## Execution model
//!
//! One shared operand stack; one [`Frame`] per active call holding its own
//! `ip` and local slots (addressed by the Resolver's Phase 1 slot indices —
//! no name lookups at runtime). Calls are iterative, so Arc recursion never
//! consumes host stack; depth is still capped at the same limit as the
//! tree-walker so both backends report the same stack-overflow error.
//!
//! ## Semantics sharing
//!
//! All binary-operator arithmetic/comparison goes through
//! [`apply_binary`](crate::ast::types::apply_binary), the same function the
//! evaluator uses — the two backends cannot drift on operator semantics or
//! error messages by construction.
//!
//! ## Error handling
//!
//! ponytail: unlike the tree-walker (which records an error and keeps
//! executing subsequent statements), the VM halts on the first runtime
//! error — continuing with a corrupted operand stack has no meaningful
//! semantics. Error-free programs (the golden suite) are unaffected; on a
//! failing program both backends report the same first error. Diagnostics
//! carry a source span recovered from the chunk's per-byte offset table.

use crate::ast::diagnostic::{Diagnostic, DiagnosticKind};
use crate::ast::evaluator::ASTEvaluator;
use crate::ast::lexer::TextSpan;
use crate::ast::types::{apply_binary, Value};
use crate::ast::ASTBinaryOperatorKind;
use crate::bytecode::chunk::Chunk;
use crate::bytecode::compiler::CompiledProgram;
use crate::bytecode::opcode::OpCode;

/// One active call: which function's chunk is executing (`None` = the
/// top-level script), where in it, and the call's local slots.
struct Frame {
    function: Option<usize>,
    ip: usize,
    locals: Vec<Value>,
}

/// A runtime error: message plus the source byte-offset of the faulting
/// instruction (from the chunk's offset table).
type RuntimeError = (String, u32);

pub struct VM<'a> {
    program: &'a CompiledProgram,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    globals: Vec<Value>,
    /// The script's final value (its implicit-return result), `None` if the
    /// script didn't end in an expression statement — mirroring the
    /// evaluator's `last_value` so tests can compare the two directly.
    pub last_value: Option<Value>,
    pub errors: Vec<Diagnostic>,
}

impl<'a> VM<'a> {
    pub fn new(program: &'a CompiledProgram, global_slot_count: u32) -> Self {
        Self {
            program,
            stack: Vec::new(),
            frames: Vec::new(),
            // Same inert filler the evaluator uses for not-yet-assigned slots.
            globals: vec![Value::Integer(0); global_slot_count as usize],
            last_value: None,
            errors: Vec::new(),
        }
    }

    /// Executes the whole program. Runtime errors land in
    /// [`VM::errors`]; the script's implicit result in [`VM::last_value`].
    pub fn run(&mut self) {
        // Script-frame locals exist only for IR-pipeline programs (spilled
        // registers); zero-sized for directly-compiled bytecode.
        let script_locals = vec![Value::Integer(0); self.program.script_frame_size as usize];
        self.frames.push(Frame { function: None, ip: 0, locals: script_locals });
        if let Err((message, offset)) = self.execute() {
            let span = TextSpan::new(offset as usize, offset as usize + 1, String::new());
            self.errors.push(Diagnostic::new(DiagnosticKind::RuntimeError, message, Some(span)));
        }
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("compiler guarantees stack balance")
    }

    fn peek(&self) -> &Value {
        self.stack.last().expect("compiler guarantees stack balance")
    }

    /// Pops the top `argc` values (in push order) after rejecting `Unit`
    /// arguments — the VM equivalent of the evaluator failing to evaluate an
    /// argument (a call to a function that returned no value).
    fn pop_args(&mut self, argc: usize, callee: &str, offset: u32) -> Result<Vec<Value>, RuntimeError> {
        let args = self.stack.split_off(self.stack.len() - argc);
        if args.iter().any(|a| matches!(a, Value::Unit)) {
            return Err((format!("Failed to evaluate argument to '{}'", callee), offset));
        }
        Ok(args)
    }

    fn execute(&mut self) -> Result<(), RuntimeError> {
        let program = self.program;
        loop {
            let frame_idx = self.frames.len() - 1;
            let (function, at) = {
                let f = &self.frames[frame_idx];
                (f.function, f.ip)
            };
            let chunk: &Chunk = match function {
                None => &program.script,
                Some(i) => &program.functions[i].chunk,
            };
            let op = OpCode::from_byte(chunk.code[at]).expect("compiler emits only valid opcodes");
            let offset = chunk.offsets[at];

            match op {
                OpCode::Constant => {
                    let idx = chunk.read_u16(at + 1);
                    self.stack.push(chunk.constants[idx as usize].clone());
                    self.frames[frame_idx].ip = at + 3;
                }
                OpCode::True => {
                    self.stack.push(Value::Boolean(true));
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::False => {
                    self.stack.push(Value::Boolean(false));
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::Pop => {
                    self.pop();
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::GetLocal => {
                    let slot = chunk.read_u16(at + 1) as usize;
                    let value = self.frames[frame_idx].locals[slot].clone();
                    self.stack.push(value);
                    self.frames[frame_idx].ip = at + 3;
                }
                OpCode::SetLocal => {
                    let slot = chunk.read_u16(at + 1) as usize;
                    // Peek, not pop: assignment-as-expression leaves the value.
                    self.frames[frame_idx].locals[slot] = self.peek().clone();
                    self.frames[frame_idx].ip = at + 3;
                }
                OpCode::GetGlobal => {
                    let slot = chunk.read_u16(at + 1) as usize;
                    self.stack.push(self.globals[slot].clone());
                    self.frames[frame_idx].ip = at + 3;
                }
                OpCode::SetGlobal => {
                    let slot = chunk.read_u16(at + 1) as usize;
                    self.globals[slot] = self.peek().clone();
                    self.frames[frame_idx].ip = at + 3;
                }
                OpCode::ToFloat => {
                    if let Value::Integer(i) = self.peek() {
                        let widened = Value::Float(*i as f64);
                        *self.stack.last_mut().unwrap() = widened;
                    }
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::Add
                | OpCode::Subtract
                | OpCode::Multiply
                | OpCode::Divide
                | OpCode::Modulo
                | OpCode::Power
                | OpCode::BitAnd
                | OpCode::BitOr
                | OpCode::BitXor
                | OpCode::ShiftLeft
                | OpCode::ShiftRight
                | OpCode::Equal
                | OpCode::NotEqual
                | OpCode::Less
                | OpCode::Greater
                | OpCode::LessEqual
                | OpCode::GreaterEqual => {
                    let right = self.pop();
                    let left = self.pop();
                    let result = apply_binary(&Self::binary_kind(op), &left, &right)
                        .map_err(|e| (e, offset))?;
                    self.stack.push(result);
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::Negate => {
                    let value = self.pop();
                    let negated = match value {
                        Value::Integer(i) => Value::Integer(-i),
                        Value::Float(f) => Value::Float(-f),
                        other => return Err((format!("Cannot negate {:?}", other.get_type()), offset)),
                    };
                    self.stack.push(negated);
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::Not => {
                    let value = self.pop();
                    self.stack.push(Value::Boolean(!value.to_boolean()));
                    self.frames[frame_idx].ip = at + 1;
                }
                OpCode::Jump => {
                    let jump = chunk.read_u16(at + 1) as usize;
                    self.frames[frame_idx].ip = at + 3 + jump;
                }
                OpCode::JumpIfFalse => {
                    let jump = chunk.read_u16(at + 1) as usize;
                    // Peeks (does not pop); the compiler emits explicit POPs.
                    if self.peek().to_boolean() {
                        self.frames[frame_idx].ip = at + 3;
                    } else {
                        self.frames[frame_idx].ip = at + 3 + jump;
                    }
                }
                OpCode::Call => {
                    let function_id = chunk.read_u16(at + 1) as usize;
                    let argc = chunk.code[at + 3] as usize;
                    let callee = &program.functions[function_id];

                    // Same limit and message as the tree-walker (its frame
                    // count excludes the script, so compare against ours - 1).
                    if self.frames.len() - 1 >= ASTEvaluator::MAX_CALL_DEPTH {
                        return Err((
                            format!(
                                "Stack overflow: maximum recursion depth ({}) exceeded while calling '{}'",
                                ASTEvaluator::MAX_CALL_DEPTH,
                                callee.name
                            ),
                            offset,
                        ));
                    }

                    let args = self.pop_args(argc, &callee.name, offset)?;
                    // Same inert filler the evaluator uses for locals that
                    // haven't been assigned yet.
                    let mut locals = vec![Value::Integer(0); callee.frame_size as usize];
                    // Parameters occupy slots 0..arity in order (asserted at
                    // compile time), so arguments drop straight in.
                    for (slot, value) in args.into_iter().enumerate() {
                        locals[slot] = value;
                    }

                    self.frames[frame_idx].ip = at + 4; // return address
                    self.frames.push(Frame { function: Some(function_id), ip: 0, locals });
                }
                OpCode::Return => {
                    let value = self.pop();
                    self.frames.pop();
                    if self.frames.is_empty() {
                        // Script finished: Unit means "didn't end in an
                        // expression statement", i.e. the evaluator's None.
                        self.last_value = match value {
                            Value::Unit => None,
                            v => Some(v),
                        };
                        return Ok(());
                    }
                    // Leave the return value for the caller.
                    self.stack.push(value);
                }
                OpCode::Print => {
                    let argc = chunk.code[at + 1] as usize;
                    let args = self.pop_args(argc, "print", offset)?;
                    let line = args.iter().map(Value::to_string).collect::<Vec<_>>().join(" ");
                    println!("{}", line);
                    self.stack.push(Value::Unit); // print has no value
                    self.frames[frame_idx].ip = at + 2;
                }
                OpCode::Max | OpCode::Min => {
                    let name = if op == OpCode::Max { "max" } else { "min" };
                    let argc = chunk.code[at + 1] as usize;
                    let args = self.pop_args(argc, name, offset)?;
                    let Some((first, rest)) = args.split_first() else {
                        return Err((format!("{}() requires at least one argument", name), offset));
                    };
                    let keep_current = if op == OpCode::Max {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                    let mut current = first.clone();
                    for v in rest {
                        match current.compare(v) {
                            Ok(ordering) if ordering == keep_current => current = v.clone(),
                            Ok(_) => (),
                            Err(e) => return Err((format!("{}() comparison error: {}", name, e), offset)),
                        }
                    }
                    self.stack.push(current);
                    self.frames[frame_idx].ip = at + 2;
                }
            }
        }
    }

    /// Maps an arithmetic/comparison opcode back to the operator kind
    /// [`apply_binary`] speaks (the inverse of the compiler's lowering).
    fn binary_kind(op: OpCode) -> ASTBinaryOperatorKind {
        use ASTBinaryOperatorKind::*;
        match op {
            OpCode::Add => Plus,
            OpCode::Subtract => Minus,
            OpCode::Multiply => Multiply,
            OpCode::Divide => Divide,
            OpCode::Modulo => Modulo,
            OpCode::Power => Exponentiation,
            OpCode::BitAnd => BitwiseAnd,
            OpCode::BitOr => BitwiseOr,
            OpCode::BitXor => BitwiseXor,
            OpCode::ShiftLeft => LeftShift,
            OpCode::ShiftRight => RightShift,
            OpCode::Equal => Equal,
            OpCode::NotEqual => NotEqual,
            OpCode::Less => Less,
            OpCode::Greater => Greater,
            OpCode::LessEqual => LessEqual,
            OpCode::GreaterEqual => GreaterEqual,
            _ => unreachable!("not a binary opcode: {:?}", op),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::lexer::Lexer;
    use crate::ast::parser::Parser;
    use crate::ast::resolver::Resolver;
    use crate::ast::Ast;
    use crate::bytecode::compiler::BytecodeCompiler;

    struct Outcome {
        last_value: Option<Value>,
        errors: Vec<String>,
    }

    /// Runs `source` on both backends and returns (tree-walker, VM)
    /// outcomes. Panics on parse/resolve errors — these tests are about
    /// execution parity, not upstream diagnostics.
    fn run_both(source: &str) -> (Outcome, Outcome) {
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
        assert!(parser.diagnostics.is_empty(), "parse errors: {:?}", parser.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());

        let mut resolver = Resolver::new();
        resolver.resolve(&ast);
        assert!(!resolver.has_errors(), "resolve errors: {:?}", resolver.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());

        let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
        evaluator.execute(&ast);
        let tree = Outcome {
            last_value: evaluator.last_value.clone(),
            errors: evaluator.errors.iter().map(|d| d.message.clone()).collect(),
        };

        let program = BytecodeCompiler::new(resolver.function_count()).compile(&ast);
        let mut vm = VM::new(&program, resolver.global_slot_count());
        vm.run();
        let vm_outcome = Outcome {
            last_value: vm.last_value.clone(),
            errors: vm.errors.iter().map(|d| d.message.clone()).collect(),
        };

        (tree, vm_outcome)
    }

    fn assert_parity(source: &str) {
        let (tree, vm) = run_both(source);
        assert_eq!(tree.last_value, vm.last_value, "last_value diverged on: {}", source);
        assert_eq!(tree.errors, vm.errors, "errors diverged on: {}", source);
    }

    #[test]
    fn test_golden_suite_parity() {
        // Every program the Phase 2 golden compile suite used, now checked
        // for identical results across both execution backends.
        let programs = [
            "1 + 2 * 3;",
            "2 ** 10;",
            "10 % 3;",
            "7 / 2;",
            "7.5 / 2.5;",
            "1 + 2.5;",
            "\"con\" + \"cat\";",
            "12 & 10; 12 | 10; 12 ^ 10; 8 << 2; 32 >> 2;",
            "-5; +5; !true; !0;",
            "let x = 1; x = x + 1; x;",
            "let x = 1.5; x = 2; x + 0.5;",
            "true && false || !true;",
            "1 < 2 && 2 <= 2 && 3 > 2 && 3 >= 3 && 1 == 1 && 1 != 2;",
            "10 == 10.0;",
            "let r = 0; if true { r = 1; } else { r = 2; } r;",
            "let r = 0; if false { r = 1; } else { r = 2; } r;",
            "let v = 5; { let v = 99; } v;",
            "fn add(a, b) { return a + b; } add(2, 3);",
            "fn add(a: Int, b: Int) { return a + b; } add(2, 3);",
            "fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(10);",
            "fn is_even(n) { if n == 0 { return true; } else { return is_odd(n - 1); } } \
             fn is_odd(n) { if n == 0 { return false; } else { return is_even(n - 1); } } \
             is_even(10);",
            "let counter = 10; ++counter; counter++; --counter; counter--;",
            "let c = 10; ++c; c;",
            "let c = 10; c++; c;",
            "min(4, 2, 9, 1); max(4, 2, 9, 1);",
            "min(3.5, 2, 4.1);",
            "fn f() { 7; } f();",
            "fn sq(n: Int) { return n * n; } sq(2) + sq(3);",
            "let g = 41; fn get() { return g + 1; } get();",
        ];
        for source in programs {
            assert_parity(source);
        }
    }

    #[test]
    fn test_runtime_error_parity() {
        // Single-statement failures: both backends report the same one
        // error. (Multi-statement error recovery intentionally differs: the
        // tree-walker keeps going, the VM halts — see module docs.)
        for source in [
            "1 / 0;",
            "1.0 / 0.0;",
            "10 % 0;",
            r#"fn f() { let x = 1; } print(f());"#,
            r#"print(print("x"));"#,
        ] {
            assert_parity(source);
        }
    }

    #[test]
    fn test_stack_overflow_parity() {
        let (tree, vm) = run_both("fn boom(n) { return boom(n + 1); } boom(0);");
        assert_eq!(tree.errors, vm.errors);
        assert_eq!(vm.errors.len(), 1);
        assert!(vm.errors[0].contains("Stack overflow: maximum recursion depth"));
    }

    #[test]
    fn test_vm_error_carries_source_span() {
        let source = "let a = 1;\nlet b = 0;\na / b;";
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
        let mut resolver = Resolver::new();
        resolver.resolve(&ast);
        let program = BytecodeCompiler::new(resolver.function_count()).compile(&ast);
        let mut vm = VM::new(&program, resolver.global_slot_count());
        vm.run();

        assert_eq!(vm.errors.len(), 1);
        let rendered = vm.errors[0].format_with_source(source);
        assert!(rendered.contains("Division by zero"), "{}", rendered);
        // The faulting `/` is on line 3 — the offset table must point there.
        assert!(rendered.contains("3"), "expected a line-3 location in: {}", rendered);
    }
}
