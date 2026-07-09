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
pub mod liveness;
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
    use super::{dump, liveness, lower, passes, to_bytecode};

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
            "let sum = 0; let i = 0; while i < 5 { sum = sum + i; i = i + 1; } sum;",
            "let x = 0; while false { x = 99; } x;",
            "let total = 0; for i in 0..5 { total = total + i; } total;",
            "fn first_over(limit) { let i = 0; while true { if i > limit { return i; } i = i + 1; } } first_over(3);",
        ];
        for source in programs {
            assert_optimized_parity(source);
        }
    }

    // -- loops (Phase 4.5 back-edge support) --------------------------------------

    #[test]
    fn test_constant_false_while_condition_eliminates_the_loop_body() {
        let mut program = lower_source(r#"while false { print("never"); } print("done");"#);
        passes::optimize(&mut program);
        // The body's print must be gone; only "done" survives.
        let text = dump::dump_program(&program);
        assert!(!text.contains("never"), "\n{}", text);
        assert!(text.contains("done"), "\n{}", text);
    }

    #[test]
    fn test_loop_back_edge_survives_optimization() {
        // A loop that isn't constant-foldable must still contain a jump
        // back to its own header after optimization runs.
        let mut program = lower_source("let i = 0; while i < 10 { i = i + 1; }");
        passes::optimize(&mut program);
        let text = dump::dump_program(&program);
        // At least one block must jump to a block with a lower id than itself.
        let has_back_edge = program
            .script
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.dead)
            .any(|(id, b)| matches!(&b.terminator, super::instr::Terminator::Jump(target) if (target.0 as usize) <= id));
        assert!(has_back_edge, "\n{}", text);
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

    // -- liveness analysis --------------------------------------------------

    #[test]
    fn test_liveness_dead_store_has_empty_live_after_and_live_store_is_live_after() {
        // `a`'s store is dead (never read again); `b`'s store is live (read by return).
        let program = lower_source("fn f(n: Int) { let a = 1; let b = 2; return b; } f(1);");
        let f = &program.functions[0];
        let live = liveness::analyze(f);
        let block = &f.blocks[0];
        let live_after = liveness::live_after_each(block, &live.live_out[0]);

        let store_slots: Vec<u32> = block
            .instrs
            .iter()
            .filter_map(|i| match &i.kind {
                InstrKind::StoreLocal { slot, .. } => Some(slot.0),
                _ => None,
            })
            .collect();
        assert_eq!(store_slots.len(), 2, "expected two stores (a, b)\n{}", dump::dump_program(&program));
        let (a_slot, b_slot) = (store_slots[0], store_slots[1]);

        let idx_of = |slot: u32| {
            block.instrs.iter().position(|i| matches!(&i.kind, InstrKind::StoreLocal { slot: s, .. } if s.0 == slot)).unwrap()
        };
        assert!(!live_after[idx_of(a_slot)].contains(&a_slot), "\n{}", dump::dump_program(&program));
        assert!(live_after[idx_of(b_slot)].contains(&b_slot), "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_liveness_live_out_is_union_of_branch_arms() {
        // `x` is only read in the else arm; it must still be live-out of
        // the header block (union of both successors' live-in).
        let program =
            lower_source("fn f(cond: Bool, n: Int) { let x = n; if cond { return 1; } else { return x; } } f(true, 5);");
        let f = &program.functions[0];
        let live = liveness::analyze(f);
        let x_slot = f.blocks[0]
            .instrs
            .iter()
            .find_map(|i| match &i.kind {
                InstrKind::StoreLocal { slot, .. } => Some(slot.0),
                _ => None,
            })
            .unwrap();
        assert!(live.live_out[0].contains(&x_slot), "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_liveness_loop_carried_store_stays_live_via_back_edge() {
        let program = lower_source("fn f(n: Int) { let i = 0; while i < n { i = i + 1; } return i; } f(3);");
        let f = &program.functions[0];
        let live = liveness::analyze(f);
        let i_slot = f.blocks[0]
            .instrs
            .iter()
            .find_map(|i| match &i.kind {
                InstrKind::StoreLocal { slot, .. } => Some(slot.0),
                _ => None,
            })
            .unwrap();
        // The loop body's `i = i + 1` store (not b0's initializer) must
        // still be live-after: the back-edge feeds the header's condition
        // read back into this block during the fixpoint.
        let (bid, idx) = f
            .blocks
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(bid, b)| {
                let idx = b
                    .instrs
                    .iter()
                    .position(|ins| matches!(&ins.kind, InstrKind::StoreLocal { slot, .. } if slot.0 == i_slot))?;
                Some((bid, idx))
            })
            .unwrap_or_else(|| panic!("expected a loop-body store to slot {}\n{}", i_slot, dump::dump_program(&program)));
        let live_after = liveness::live_after_each(&f.blocks[bid], &live.live_out[bid]);
        assert!(live_after[idx].contains(&i_slot), "\n{}", dump::dump_program(&program));
    }

    // -- dead store elimination --------------------------------------------------

    #[test]
    fn test_dse_removes_store_never_read_again() {
        let mut program = lower_source("fn f(n: Int) { let a = 1; let b = 2; return b; } f(1);");
        let f = &mut program.functions[0];
        passes::dse(f);
        let stores = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::StoreLocal { .. }))
            .count();
        assert_eq!(stores, 1, "expected only b's store to survive\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_dse_keeps_store_read_in_other_branch_arm() {
        let mut program =
            lower_source("fn f(cond: Bool, n: Int) { let x = n; if cond { return 1; } else { return x; } } f(true, 5);");
        let f = &mut program.functions[0];
        passes::dse(f);
        let stores = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::StoreLocal { .. }))
            .count();
        assert_eq!(stores, 1, "x's store is read in the else arm, must survive\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_dse_keeps_loop_carried_store() {
        let mut program = lower_source("fn f(n: Int) { let i = 0; while i < n { i = i + 1; } return i; } f(3);");
        let f = &mut program.functions[0];
        passes::dse(f);
        let stores = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::StoreLocal { .. }))
            .count();
        // The initializer `i = 0` and the body's `i = i + 1` both survive:
        // both are read (by the condition / by return).
        assert_eq!(stores, 2, "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_dse_never_touches_store_global() {
        // Top-level `let` is a global, not a local — conservatively always
        // live, DSE must not remove it even though `a` is never read again.
        let mut program = lower_source("let a = 1; let b = 2; b;");
        passes::dse(&mut program.script);
        let stores = count_instrs(&program, |k| matches!(k, InstrKind::StoreGlobal { .. }));
        assert_eq!(stores, 2, "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_optimize_pipeline_runs_dse() {
        let mut program = lower_source("fn f(n: Int) { let a = 1; let b = 2; return b; } f(1);");
        passes::optimize(&mut program);
        let f = &program.functions[0];
        let stores = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::StoreLocal { .. }))
            .count();
        assert_eq!(stores, 1, "only b's store should survive the full pipeline\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_dse_removes_dead_store_even_when_fed_by_erroring_op() {
        let mut program = lower_source("fn f(n: Int) { let x = 10 % n; return 1; } f(1);");
        passes::optimize(&mut program);
        let f = &program.functions[0];
        let stores = f
            .blocks
            .iter()
            .filter(|b| !b.dead)
            .flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i.kind, InstrKind::StoreLocal { .. }))
            .count();
        assert_eq!(stores, 0, "\n{}", dump::dump_program(&program));
        // But the modulo itself — which can raise "Modulo by zero" — must
        // survive; deleting it would silently drop a runtime error.
        let in_f = |k: &InstrKind| matches!(k, InstrKind::Binary { .. });
        assert_eq!(count_instrs(&program, in_f), 1, "\n{}", dump::dump_program(&program));
    }

    #[test]
    fn test_dse_preserves_runtime_error_from_dead_stores_source_expr() {
        // `x`'s store is dead (never read again), but `10 % n` can raise
        // "Modulo by zero" at n=0 — DSE must not silently swallow that
        // error by deleting the op along with the dead store.
        //
        // Compared against the *un-optimized* VM rather than the
        // tree-walker: the tree-walker wraps any erroring store RHS (`let`
        // initializer or assignment) in an extra diagnostic the bytecode/IR
        // paths never emit — a pre-existing divergence unrelated to DSE,
        // reproducible on plain `--backend=vm` with no IR pass involved.
        // This test isolates DSE's actual contract: it must not change the
        // optimized pipeline's own error behavior relative to unoptimized.
        let source = "fn f(n: Int) { let x = 10 % n; return 1; } f(0);";
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

        let unopt = lower::IrLowering::new(resolver.function_count()).lower(&ast);
        let unopt_compiled = to_bytecode::program_to_bytecode(&unopt);
        let mut unopt_vm = VM::new(&unopt_compiled, resolver.global_slot_count());
        unopt_vm.run();

        let mut opt = lower::IrLowering::new(resolver.function_count()).lower(&ast);
        passes::optimize(&mut opt);
        let opt_compiled = to_bytecode::program_to_bytecode(&opt);
        let mut opt_vm = VM::new(&opt_compiled, resolver.global_slot_count());
        opt_vm.run();

        let unopt_errors: Vec<_> = unopt_vm.errors.iter().map(|d| d.message.clone()).collect();
        let opt_errors: Vec<_> = opt_vm.errors.iter().map(|d| d.message.clone()).collect();
        assert_eq!(unopt_errors, opt_errors);
        assert!(!opt_errors.is_empty(), "expected the modulo-by-zero error to survive DSE");
    }

    // -- liveness dump --------------------------------------------------------

    #[test]
    fn test_dump_liveness_shows_dead_and_live_slots() {
        // Dump runs pre-DSE (on `lower_source`'s un-optimized output), so
        // both the dead store to `a` and the live store to `b` are present
        // for the printer to annotate.
        let program = lower_source("fn f(n: Int) { let a = 1; let b = 2; return b; } f(1);");
        let text = dump::dump_liveness(&program);
        assert!(text.contains("live-in"), "\n{}", text);
        assert!(text.contains("live-out"), "\n{}", text);
        assert!(text.contains("live-after"), "\n{}", text);
        // `a`'s store line must show an empty live-after set; `b`'s must
        // show its own slot live-after.
        let lines: Vec<&str> = text.lines().collect();
        let a_line = lines.iter().find(|l| l.contains("store_local 1")).unwrap();
        assert!(a_line.contains("live-after={}"), "\n{}", text);
        let b_line = lines.iter().find(|l| l.contains("store_local 2")).unwrap();
        assert!(b_line.contains("live-after={2}"), "\n{}", text);
    }

    #[test]
    fn test_dump_liveness_notes_frame_with_no_locals() {
        // Top-level `let` is a global, not a local — the script frame has
        // zero locals, and every liveness set is trivially empty. Without a
        // note, that reads as "the analysis is broken" rather than "there's
        // nothing to track here".
        let program = lower_source("let a = 1; a;");
        let text = dump::dump_liveness(&program);
        assert!(text.to_lowercase().contains("no locals"), "\n{}", text);
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
