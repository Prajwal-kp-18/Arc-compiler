//! # IR Types
//!
//! Three-address code over virtual registers, organized into basic blocks
//! with explicit terminators — the representation the Phase 4 optimization
//! passes operate on. Not SSA: registers are *almost* single-assignment
//! (lowering writes each register once per block; only `&&`/`||` join
//! results are written in two different blocks), and locals/globals stay
//! slot-addressed memory operations rather than being register-promoted.
//! See `local-notes/phase4-ir-design.md`.

use crate::ast::resolver::{BuiltinFn, SlotIndex};
use crate::ast::types::Value;
use crate::ast::ASTBinaryOperatorKind;

/// A virtual register. Infinite supply, function-local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u32);

/// A basic-block id — an index into [`IrFunction::blocks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// One IR operation plus the source byte-offset it originated from (same
/// convention as [`Chunk::offsets`](crate::bytecode::chunk::Chunk::offsets),
/// so bytecode lowering can rebuild the offset table and runtime
/// diagnostics survive optimization).
#[derive(Debug, Clone)]
pub struct Instruction {
    pub kind: InstrKind,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub enum InstrKind {
    Const { dst: Reg, value: Value },
    /// A non-logical binary operation (`&&`/`||` never reach the IR — they
    /// lower to control flow). `can_error` is the shared purity fact every
    /// pass keys off: `true` if this op could raise a runtime error
    /// (division/modulo by zero, `**` overflow, or any operand typed `Any`),
    /// in which case folding skips error results and DCE/CSE leave it alone.
    Binary { dst: Reg, op: ASTBinaryOperatorKind, lhs: Reg, rhs: Reg, can_error: bool },
    /// Arithmetic negation. Errors at runtime on non-numeric values, so it
    /// carries its own `can_error` (true iff the operand's static type is
    /// `Any`).
    Negate { dst: Reg, src: Reg, can_error: bool },
    /// Logical NOT with boolean coercion (`!x`). Never errors.
    Not { dst: Reg, src: Reg },
    /// Boolean coercion (`Value::to_boolean`), used by `&&`/`||` lowering.
    /// Never errors.
    ToBool { dst: Reg, src: Reg },
    /// Resolver-decided int→float widening. Never errors.
    ToFloat { dst: Reg, src: Reg },
    LoadLocal { dst: Reg, slot: SlotIndex },
    StoreLocal { slot: SlotIndex, src: Reg },
    LoadGlobal { dst: Reg, slot: SlotIndex },
    StoreGlobal { slot: SlotIndex, src: Reg },
    Call { dst: Reg, function: u32, args: Vec<Reg> },
    CallBuiltin { dst: Reg, builtin: BuiltinFn, args: Vec<Reg> },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Jump(BlockId),
    Branch { cond: Reg, then_block: BlockId, else_block: BlockId },
    Return(Reg),
}

#[derive(Debug)]
pub struct Block {
    pub instrs: Vec<Instruction>,
    pub terminator: Terminator,
    pub term_offset: u32,
    /// Set by unreachable-block elimination. Dead blocks stay in the vec so
    /// `BlockId`s remain stable; dump and bytecode lowering skip them.
    pub dead: bool,
}

pub struct IrFunction {
    pub name: String,
    /// Local slots (parameters + declared variables) — the Resolver's
    /// per-function frame size. Registers live *above* these when the
    /// bytecode lowering spills them to slots.
    pub frame_size: u32,
    pub reg_count: u32,
    /// Entry block is always `blocks[0]`.
    pub blocks: Vec<Block>,
}

pub struct IrProgram {
    pub script: IrFunction,
    /// Indexed by `FunctionId.0`.
    pub functions: Vec<IrFunction>,
}

impl InstrKind {
    /// The register this instruction writes, if any.
    pub fn dst(&self) -> Option<Reg> {
        use InstrKind::*;
        match self {
            Const { dst, .. }
            | Binary { dst, .. }
            | Negate { dst, .. }
            | Not { dst, .. }
            | ToBool { dst, .. }
            | ToFloat { dst, .. }
            | LoadLocal { dst, .. }
            | LoadGlobal { dst, .. }
            | Call { dst, .. }
            | CallBuiltin { dst, .. } => Some(*dst),
            StoreLocal { .. } | StoreGlobal { .. } => None,
        }
    }

    /// Visits every register this instruction *reads* (mutably, so passes
    /// can rewrite operands in place).
    pub fn for_each_use(&mut self, f: &mut dyn FnMut(&mut Reg)) {
        use InstrKind::*;
        match self {
            Const { .. } | LoadLocal { .. } | LoadGlobal { .. } => {}
            Binary { lhs, rhs, .. } => {
                f(lhs);
                f(rhs);
            }
            Negate { src, .. } | Not { src, .. } | ToBool { src, .. } | ToFloat { src, .. } => f(src),
            StoreLocal { src, .. } | StoreGlobal { src, .. } => f(src),
            Call { args, .. } | CallBuiltin { args, .. } => {
                for arg in args {
                    f(arg);
                }
            }
        }
    }

    /// Whether the instruction is removable when its result is unused:
    /// no side effects *and* provably unable to raise a runtime error.
    /// Deleting a potential runtime error is a behavior change, so
    /// `Binary`/`Negate` consult the `can_error` fact computed at lowering.
    pub fn is_pure(&self) -> bool {
        use InstrKind::*;
        match self {
            Const { .. } | Not { .. } | ToBool { .. } | ToFloat { .. } | LoadLocal { .. } | LoadGlobal { .. } => true,
            Binary { can_error, .. } | Negate { can_error, .. } => !can_error,
            StoreLocal { .. } | StoreGlobal { .. } | Call { .. } | CallBuiltin { .. } => false,
        }
    }
}

impl IrFunction {
    /// Live (non-dead-block) instruction count — the metric the exit
    /// criterion's "concrete reduction" is measured in. Terminators aren't
    /// counted; passes don't remove control flow except with dead blocks.
    pub fn instr_count(&self) -> usize {
        self.blocks.iter().filter(|b| !b.dead).map(|b| b.instrs.len()).sum()
    }
}

impl IrProgram {
    pub fn instr_count(&self) -> usize {
        self.script.instr_count() + self.functions.iter().map(IrFunction::instr_count).sum::<usize>()
    }
}
