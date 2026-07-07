//! # IR → Bytecode Lowering
//!
//! Makes the IR *runnable*: lowers an (optimized) [`IrProgram`] to the same
//! `Chunk`s the Phase 2 compiler emits, so the existing VM executes it and
//! the golden suite gates every optimization pass. Wired up behind `--opt`.
//!
//! Register scheme: spill-everything. Each virtual register gets a frame
//! slot above the function's real locals (`slot = frame_size + reg`), every
//! instruction loads its operands from slots and stores its result back, and
//! the operand stack is empty at every instruction boundary. Deliberately
//! naive — correctness is the requirement; the VM doesn't care, and Phase 5
//! consumes the IR directly, not this bytecode.
//!
//! Jumps: block creation order in `lower.rs` guarantees every CFG edge
//! targets a higher block id, and blocks are emitted in id order, so the
//! forward-only `Jump`/`JumpIfFalse` opcodes suffice. // CFG is a DAG today

use crate::ast::resolver::{BuiltinFn, SlotIndex};
use crate::bytecode::chunk::Chunk;
use crate::bytecode::compiler::{CompiledFunction, CompiledProgram};
use crate::bytecode::opcode::OpCode;

use super::instr::{Block, BlockId, InstrKind, IrFunction, IrProgram, Reg, Terminator};

pub fn program_to_bytecode(program: &IrProgram) -> CompiledProgram {
    let script = function_to_chunk(&program.script);
    let functions = program
        .functions
        .iter()
        .map(|f| CompiledFunction {
            name: f.name.clone(),
            frame_size: f.frame_size + f.reg_count, // locals + spilled registers
            chunk: function_to_chunk(f),
        })
        .collect();
    CompiledProgram {
        script,
        functions,
        // The script has no Resolver locals (its variables are globals), but
        // its spilled registers need a frame too.
        script_frame_size: program.script.reg_count,
    }
}

struct ChunkBuilder<'a> {
    f: &'a IrFunction,
    chunk: Chunk,
    /// Bytecode offset where each block starts (filled as blocks are emitted).
    block_starts: Vec<Option<usize>>,
    /// (operand position, target block) pairs to patch once starts are known.
    pending_jumps: Vec<(usize, BlockId)>,
}

fn function_to_chunk(f: &IrFunction) -> Chunk {
    let mut b = ChunkBuilder {
        f,
        chunk: Chunk::new(),
        block_starts: vec![None; f.blocks.len()],
        pending_jumps: Vec::new(),
    };

    for (id, block) in f.blocks.iter().enumerate() {
        if block.dead {
            continue;
        }
        b.block_starts[id] = Some(b.chunk.code.len());
        b.emit_block(block);
    }

    for (operand_at, target) in std::mem::take(&mut b.pending_jumps) {
        let target_start = b.block_starts[target.0 as usize]
            .expect("live blocks never jump to dead blocks");
        debug_assert!(target_start >= operand_at + 2, "CFG edges are forward-only (DAG)");
        b.chunk.patch_jump_to(operand_at, target_start);
    }

    b.chunk
}

impl ChunkBuilder<'_> {
    /// The frame slot a virtual register is spilled to.
    fn reg_slot(&self, reg: Reg) -> u16 {
        u16::try_from(self.f.frame_size + reg.0).expect("spilled register slot exceeded u16::MAX")
    }

    fn slot_u16(slot: SlotIndex) -> u16 {
        u16::try_from(slot.0).expect("slot index exceeded u16::MAX")
    }

    /// Push a register's value onto the operand stack.
    fn push_reg(&mut self, reg: Reg, offset: u32) {
        let slot = self.reg_slot(reg);
        self.chunk.write_op(OpCode::GetLocal, offset);
        self.chunk.write_u16(slot, offset);
    }

    /// Pop the top of the operand stack into a register.
    fn pop_into_reg(&mut self, reg: Reg, offset: u32) {
        let slot = self.reg_slot(reg);
        self.chunk.write_op(OpCode::SetLocal, offset); // SetLocal peeks…
        self.chunk.write_u16(slot, offset);
        self.chunk.write_op(OpCode::Pop, offset); // …so pop the leftover
    }

    fn emit_jump_to(&mut self, op: OpCode, target: BlockId, offset: u32) {
        let operand_at = self.chunk.emit_jump(op, offset);
        self.pending_jumps.push((operand_at, target));
    }

    fn emit_block(&mut self, block: &Block) {
        for instr in &block.instrs {
            self.emit_instr(&instr.kind, instr.offset);
        }
        let offset = block.term_offset;
        match &block.terminator {
            Terminator::Jump(target) => self.emit_jump_to(OpCode::Jump, *target, offset),
            Terminator::Branch { cond, then_block, else_block } => {
                // JumpIfFalse peeks, so both paths pop the condition copy.
                self.push_reg(*cond, offset);
                let to_else = self.chunk.emit_jump(OpCode::JumpIfFalse, offset);
                self.chunk.write_op(OpCode::Pop, offset);
                self.emit_jump_to(OpCode::Jump, *then_block, offset);
                let else_entry = self.chunk.code.len();
                self.chunk.patch_jump_to(to_else, else_entry);
                self.chunk.write_op(OpCode::Pop, offset);
                self.emit_jump_to(OpCode::Jump, *else_block, offset);
            }
            Terminator::Return(reg) => {
                self.push_reg(*reg, offset);
                self.chunk.write_op(OpCode::Return, offset);
            }
        }
    }

    fn emit_instr(&mut self, kind: &InstrKind, offset: u32) {
        use InstrKind::*;
        match kind {
            Const { dst, value } => {
                let idx = self.chunk.add_constant(value.clone());
                self.chunk.write_op(OpCode::Constant, offset);
                self.chunk.write_u16(idx, offset);
                self.pop_into_reg(*dst, offset);
            }
            Binary { dst, op, lhs, rhs, .. } => {
                self.push_reg(*lhs, offset);
                self.push_reg(*rhs, offset);
                self.chunk.write_op(binary_opcode(op), offset);
                self.pop_into_reg(*dst, offset);
            }
            Negate { dst, src, .. } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::Negate, offset);
                self.pop_into_reg(*dst, offset);
            }
            Not { dst, src } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::Not, offset);
                self.pop_into_reg(*dst, offset);
            }
            // to_bool == not(not(x)) — reuses the existing opcodes.
            ToBool { dst, src } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::Not, offset);
                self.chunk.write_op(OpCode::Not, offset);
                self.pop_into_reg(*dst, offset);
            }
            ToFloat { dst, src } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::ToFloat, offset);
                self.pop_into_reg(*dst, offset);
            }
            LoadLocal { dst, slot } => {
                self.chunk.write_op(OpCode::GetLocal, offset);
                self.chunk.write_u16(Self::slot_u16(*slot), offset);
                self.pop_into_reg(*dst, offset);
            }
            StoreLocal { slot, src } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::SetLocal, offset);
                self.chunk.write_u16(Self::slot_u16(*slot), offset);
                self.chunk.write_op(OpCode::Pop, offset);
            }
            LoadGlobal { dst, slot } => {
                self.chunk.write_op(OpCode::GetGlobal, offset);
                self.chunk.write_u16(Self::slot_u16(*slot), offset);
                self.pop_into_reg(*dst, offset);
            }
            StoreGlobal { slot, src } => {
                self.push_reg(*src, offset);
                self.chunk.write_op(OpCode::SetGlobal, offset);
                self.chunk.write_u16(Self::slot_u16(*slot), offset);
                self.chunk.write_op(OpCode::Pop, offset);
            }
            Call { dst, function, args } => {
                for arg in args {
                    self.push_reg(*arg, offset);
                }
                self.chunk.write_op(OpCode::Call, offset);
                self.chunk.write_u16(u16::try_from(*function).expect("function id exceeded u16::MAX"), offset);
                self.chunk.write_u8(u8::try_from(args.len()).expect("more than 255 arguments"), offset);
                self.pop_into_reg(*dst, offset);
            }
            CallBuiltin { dst, builtin, args } => {
                for arg in args {
                    self.push_reg(*arg, offset);
                }
                let op = match builtin {
                    BuiltinFn::Print => OpCode::Print,
                    BuiltinFn::Max => OpCode::Max,
                    BuiltinFn::Min => OpCode::Min,
                };
                self.chunk.write_op(op, offset);
                self.chunk.write_u8(u8::try_from(args.len()).expect("more than 255 arguments"), offset);
                self.pop_into_reg(*dst, offset);
            }
        }
    }
}

fn binary_opcode(kind: &crate::ast::ASTBinaryOperatorKind) -> OpCode {
    use crate::ast::ASTBinaryOperatorKind::*;
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
        LogicalAnd | LogicalOr => unreachable!("logical operators lower to control flow"),
    }
}
