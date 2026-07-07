//! # IR
//!
//! Phase 4 of the compiler pipeline: a three-address-code intermediate
//! representation with basic blocks and explicit control flow, sitting
//! between the resolved AST and codegen. [`lower`] builds it, [`passes`]
//! optimizes it (constant folding, DCE, local CSE — all gated on preserving
//! behavior *including runtime errors*), [`to_bytecode`] lowers it back to
//! chunks so the existing VM executes it (`--opt`), and [`dump`] prints it
//! (`--dump-ir`, `--dump-ir=opt`). See `local-notes/phase4-ir-design.md`.

pub mod instr;
pub mod lower;
pub mod passes;
pub mod dump;
pub mod to_bytecode;

#[cfg(test)]
mod tests {
    use crate::ast::evaluator::ASTEvaluator;
    use crate::ast::lexer::Lexer;
    use crate::ast::parser::Parser;
    use crate::ast::resolver::Resolver;
    use crate::ast::types::Value;
    use crate::ast::Ast;
    use crate::bytecode::vm::VM;

    use super::instr::{InstrKind, IrProgram};
    use super::{dump, lower, passes, to_bytecode};

    fn lower_source(source: &str) -> IrProgram {
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

        lower::IrLowering::new(resolver.function_count()).lower(&ast)
    }

    /// Counts live instructions matching `pred` across the whole program.
    fn count_instrs(program: &IrProgram, pred: impl Fn(&InstrKind) -> bool) -> usize {
        std::iter::once(&program.script)
            .chain(program.functions.iter())
            .flat_map(|f| f.blocks.iter().filter(|b| !b.dead))
            .flat_map(|b| b.instrs.iter())
            .filter(|i| pred(&i.kind))
            .count()
    }

    /// Runs a source program through tree-walker and the *optimized* IR
    /// pipeline, asserting identical last_value and error messages.
    fn assert_optimized_parity(source: &str) {
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
        assert!(!resolver.has_errors());

        let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
        evaluator.execute(&ast);

        let mut program = lower::IrLowering::new(resolver.function_count()).lower(&ast);
        passes::optimize(&mut program);
        let compiled = to_bytecode::program_to_bytecode(&program);
        let mut vm = VM::new(&compiled, resolver.global_slot_count());
        vm.run();

        assert_eq!(evaluator.last_value, vm.last_value, "last_value diverged on: {}", source);
        let tree_errors: Vec<_> = evaluator.errors.iter().map(|d| d.message.clone()).collect();
        let vm_errors: Vec<_> = vm.errors.iter().map(|d| d.message.clone()).collect();
        assert_eq!(tree_errors, vm_errors, "errors diverged on: {}", source);
    }

    // -- identity + optimized pipeline parity --------------------------------

    #[test]
    fn test_optimized_pipeline_parity_on_golden_programs() {
        let programs = [
            "1 + 2 * 3;",
            "2 ** 10;",
            "let x = 1; x = x + 1; x;",
            "let x = 1.5; x = 2; x + 0.5;",
            "true && false || !true;",
            "let r = 0; if true { r = 1; } else { r = 2; } r;",
            "let r = 0; if false { r = 1; } else { r = 2; } r;",
            "let v = 5; { let v = 99; } v;",
            "fn add(a: Int, b: Int) { return a + b; } add(2, 3);",
            "fn fact(n) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } fact(10);",
            "fn is_even(n) { if n == 0 { return true; } else { return is_odd(n - 1); } } \
             fn is_odd(n) { if n == 0 { return false; } else { return is_even(n - 1); } } \
             is_even(10);",
            "let counter = 10; ++counter; counter++; --counter; counter--; counter;",
            "min(4, 2, 9, 1); max(4, 2, 9, 1);",
            "fn f() { 7; } f();",
            "fn f() { return 1; 2; } f();", // unreachable code after return
            "1 / 0;",
            "10 % 0;",
            r#"print(print("x"));"#,
        ];
        for source in programs {
            assert_optimized_parity(source);
        }
    }

    // -- constant folding ------------------------------------------------------

    #[test]
    fn test_fold_collapses_constant_arithmetic() {
        let mut program = lower_source("1 + 2 * 3;");
        passes::optimize(&mut program);
        assert_eq!(count_instrs(&program, |k| matches!(k, InstrKind::Binary { .. })), 0);
        // The whole script is one Const 7 feeding the return.
        assert_eq!(program.instr_count(), 1, "\n{}", dump::dump_program(&program));
        assert!(count_instrs(&program, |k| matches!(k, InstrKind::Const { value: Value::Integer(7), .. })) == 1);
    }

    #[test]
    fn test_fold_preserves_division_by_zero() {
        let mut program = lower_source("1 / 0;");
        passes::optimize(&mut program);
        // The failing divide must survive to fail at runtime.
        assert_eq!(count_instrs(&program, |k| matches!(k, InstrKind::Binary { .. })), 1);
    }

    #[test]
    fn test_fold_preserves_int_pow_overflow() {
        let mut program = lower_source("10 ** 200;"); // i64::pow would panic
        passes::optimize(&mut program);
        assert_eq!(count_instrs(&program, |k| matches!(k, InstrKind::Binary { .. })), 1);
    }

    #[test]
    fn test_fold_constant_branch_kills_untaken_arm() {
        let mut program = lower_source(r#"if true { print("a"); } else { print("b"); }"#);
        passes::optimize(&mut program);
        // Only the taken arm's print survives.
        assert_eq!(count_instrs(&program, |k| matches!(k, InstrKind::CallBuiltin { .. })), 1);
        // And no branch remains anywhere.
        let text = dump::dump_program(&program);
        assert!(!text.contains("branch"), "\n{}", text);
    }

    // -- dead code elimination ---------------------------------------------------

    #[test]
    fn test_dce_removes_unused_pure_result() {
        let mut program = lower_source("fn f(a: Int) { a + 1; return 2; } f(1);");
        passes::optimize(&mut program);
        // `a + 1` is pure (typed Int, non-partial op) and unused -> gone,
        // along with its operand loads.
        let f = &program.functions[0];
        assert_eq!(
            f.blocks.iter().filter(|b| !b.dead).map(|b| b.instrs.len()).sum::<usize>(),
            1, // just `const 2`
            "\n{}",
            dump::dump_program(&program)
        );
    }

    #[test]
    fn test_dce_keeps_unused_but_erroring_op() {
        let mut program = lower_source("fn f(n: Int) { 10 % n; return 2; } f(1);");
        passes::optimize(&mut program);
        // `10 % n` can raise "Modulo by zero" — deleting it would change
        // behavior for f(0), so it stays.
        let in_f = |k: &InstrKind| matches!(k, InstrKind::Binary { .. });
        assert_eq!(count_instrs(&program, in_f), 1);
    }

    // -- common subexpression elimination ---------------------------------------------

    #[test]
    fn test_cse_dedupes_repeated_expression() {
        let mut program = lower_source("fn f(a: Int, b: Int) { return (a * b) + (a * b); } f(2, 3);");
        passes::optimize(&mut program);
        let f = &program.functions[0];
        let muls = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::Binary { op: crate::ast::ASTBinaryOperatorKind::Multiply, .. }))
            .count();
        assert_eq!(muls, 1, "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_cse_does_not_dedupe_across_store() {
        let mut program = lower_source("fn f(a: Int) { let x = a + 1; a = 5; let y = a + 1; return x + y; } f(1);");
        passes::optimize(&mut program);
        // `a + 1` before and after `a = 5` are different values: both adds
        // (or their folded equivalents) must produce distinct results.
        assert_optimized_parity("fn f(a: Int) { let x = a + 1; a = 5; let y = a + 1; return x + y; } f(1);");
    }

    // -- the exit-criterion metric ----------------------------------------------

    #[test]
    fn test_passes_reduce_instruction_count() {
        let source = "let a = 2 * 3 + 4; print(1 + 2 * 3, 2 ** 10, (4 - 2) * (4 - 2));";
        let before = lower_source(source);
        let mut after = lower_source(source);
        passes::optimize(&mut after);
        assert!(
            after.instr_count() < before.instr_count(),
            "expected a reduction, got {} -> {}\nbefore:\n{}\nafter:\n{}",
            before.instr_count(),
            after.instr_count(),
            dump::dump_program(&before),
            dump::dump_program(&after)
        );
    }
}
