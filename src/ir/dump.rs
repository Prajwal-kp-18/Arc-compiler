//! # IR Dump
//!
//! Text pretty-printer for [`IrProgram`] — the disassembler's sibling,
//! wired up behind `--dump-ir` (post-lowering) and `--dump-ir=opt`
//! (post-passes). The before/after diff of these two is the Phase 4 exit
//! criterion's demo artifact. [`dump_liveness`] is the same idea for Phase
//! 4.6's slot liveness analysis, behind `--dump-liveness`.

use std::collections::HashSet;

use crate::ast::resolver::BuiltinFn;
use crate::ast::ASTBinaryOperatorKind;

use super::instr::{InstrKind, IrFunction, IrProgram, Terminator};
use super::liveness;

pub fn dump_program(program: &IrProgram) -> String {
    let mut out = dump_function(&program.script, "script");
    for (id, function) in program.functions.iter().enumerate() {
        out.push('\n');
        out.push_str(&dump_function(function, &format!("fn#{} {}", id, function.name)));
    }
    out
}

pub fn dump_function(f: &IrFunction, name: &str) -> String {
    let mut out = format!("== {} (frame_size={}, {} instrs) ==\n", name, f.frame_size, f.instr_count());
    for (id, block) in f.blocks.iter().enumerate() {
        if block.dead {
            continue;
        }
        out.push_str(&format!("b{}:\n", id));
        for instr in &block.instrs {
            out.push_str(&format!("    {}\n", render(&instr.kind)));
        }
        let term = match &block.terminator {
            Terminator::Jump(b) => format!("jump b{}", b.0),
            Terminator::Branch { cond, then_block, else_block } => {
                format!("branch r{} ? b{} : b{}", cond.0, then_block.0, else_block.0)
            }
            Terminator::Return(r) => format!("return r{}", r.0),
        };
        out.push_str(&format!("    {}\n", term));
    }
    out
}

/// Prints each function's CFG annotated with slot liveness: every block's
/// live-in/live-out sets and every instruction's live-after set. `--dump-ir`
/// output is untouched by this — a separate printer behind `--dump-liveness`.
pub fn dump_liveness(program: &IrProgram) -> String {
    let mut out = dump_function_liveness(&program.script, "script");
    for (id, function) in program.functions.iter().enumerate() {
        out.push('\n');
        out.push_str(&dump_function_liveness(function, &format!("fn#{} {}", id, function.name)));
    }
    out
}

fn dump_function_liveness(f: &IrFunction, name: &str) -> String {
    let live = liveness::analyze(f);
    let mut out = format!("== {} (frame_size={}, {} instrs) ==\n", name, f.frame_size, f.instr_count());
    if f.frame_size == 0 {
        out.push_str("-- no locals in this frame; liveness only tracks LoadLocal/StoreLocal, not globals --\n");
    }
    for (id, block) in f.blocks.iter().enumerate() {
        if block.dead {
            continue;
        }
        out.push_str(&format!("b{}: live-in={}\n", id, fmt_slots(&live.live_in[id])));
        let live_after = liveness::live_after_each(block, &live.live_out[id]);
        for (instr, after) in block.instrs.iter().zip(live_after.iter()) {
            out.push_str(&format!("    {}    ; live-after={}\n", render(&instr.kind), fmt_slots(after)));
        }
        let term = match &block.terminator {
            Terminator::Jump(b) => format!("jump b{}", b.0),
            Terminator::Branch { cond, then_block, else_block } => {
                format!("branch r{} ? b{} : b{}", cond.0, then_block.0, else_block.0)
            }
            Terminator::Return(r) => format!("return r{}", r.0),
        };
        out.push_str(&format!("    {}\n", term));
        out.push_str(&format!("b{}: live-out={}\n", id, fmt_slots(&live.live_out[id])));
    }
    out
}

fn fmt_slots(slots: &HashSet<u32>) -> String {
    let mut sorted: Vec<u32> = slots.iter().copied().collect();
    sorted.sort_unstable();
    format!("{{{}}}", sorted.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","))
}

fn render(kind: &InstrKind) -> String {
    use InstrKind::*;
    match kind {
        Const { dst, value } => format!("r{} = const {:?}", dst.0, value),
        Binary { dst, op, lhs, rhs, can_error } => format!(
            "r{} = {} r{}, r{}{}",
            dst.0,
            mnemonic(op),
            lhs.0,
            rhs.0,
            if *can_error { " !" } else { "" }
        ),
        Negate { dst, src, can_error } => {
            format!("r{} = neg r{}{}", dst.0, src.0, if *can_error { " !" } else { "" })
        }
        Not { dst, src } => format!("r{} = not r{}", dst.0, src.0),
        ToBool { dst, src } => format!("r{} = to_bool r{}", dst.0, src.0),
        ToFloat { dst, src } => format!("r{} = to_float r{}", dst.0, src.0),
        LoadLocal { dst, slot } => format!("r{} = load_local {}", dst.0, slot.0),
        StoreLocal { slot, src } => format!("store_local {} = r{}", slot.0, src.0),
        LoadGlobal { dst, slot } => format!("r{} = load_global {}", dst.0, slot.0),
        StoreGlobal { slot, src } => format!("store_global {} = r{}", slot.0, src.0),
        Call { dst, function, args } => format!("r{} = call fn#{}({})", dst.0, function, regs(args)),
        CallBuiltin { dst, builtin, args } => {
            let name = match builtin {
                BuiltinFn::Print => "print",
                BuiltinFn::Max => "max",
                BuiltinFn::Min => "min",
            };
            format!("r{} = call {}({})", dst.0, name, regs(args))
        }
    }
}

fn regs(args: &[super::instr::Reg]) -> String {
    args.iter().map(|r| format!("r{}", r.0)).collect::<Vec<_>>().join(", ")
}

fn mnemonic(op: &ASTBinaryOperatorKind) -> &'static str {
    use ASTBinaryOperatorKind::*;
    match op {
        Plus => "add",
        Minus => "sub",
        Multiply => "mul",
        Divide => "div",
        Modulo => "mod",
        Exponentiation => "pow",
        BitwiseAnd => "band",
        BitwiseOr => "bor",
        BitwiseXor => "bxor",
        LeftShift => "shl",
        RightShift => "shr",
        Equal => "eq",
        NotEqual => "ne",
        Less => "lt",
        Greater => "gt",
        LessEqual => "le",
        GreaterEqual => "ge",
        LogicalAnd | LogicalOr => unreachable!("logical operators lower to control flow"),
    }
}
