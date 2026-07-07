//! # Chunk
//!
//! A unit of compiled bytecode: raw instruction bytes, a constant pool, and
//! a per-byte source-offset table for error reporting. One `Chunk` per
//! function, plus one for the top-level "script" (see
//! [`compiler`](crate::bytecode::compiler)).

use crate::ast::types::Value;
use crate::bytecode::opcode::OpCode;

#[derive(Debug, Default)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<Value>,
    /// The source byte-offset each `code` byte originated from — one entry
    /// per byte, so a disassembler or future error reporter can point back
    /// into the source without needing a separate range table.
    pub offsets: Vec<u32>,
}

impl Chunk {
    pub fn new() -> Self {
        Self::default()
    }

    fn write_byte(&mut self, byte: u8, source_offset: u32) {
        self.code.push(byte);
        self.offsets.push(source_offset);
    }

    /// Writes an opcode and returns the offset it was written at (useful as
    /// a jump-patch target).
    pub fn write_op(&mut self, op: OpCode, source_offset: u32) -> usize {
        let at = self.code.len();
        self.write_byte(op as u8, source_offset);
        at
    }

    pub fn write_u8(&mut self, value: u8, source_offset: u32) {
        self.write_byte(value, source_offset);
    }

    pub fn write_u16(&mut self, value: u16, source_offset: u32) {
        let bytes = value.to_be_bytes();
        self.write_byte(bytes[0], source_offset);
        self.write_byte(bytes[1], source_offset);
    }

    pub fn read_u16(&self, at: usize) -> u16 {
        u16::from_be_bytes([self.code[at], self.code[at + 1]])
    }

    /// Adds a value to the constant pool and returns its index.
    ///
    /// Panics if the pool exceeds `u16::MAX` entries — no program anywhere
    /// near that size exists today; this is a hard limit inherent to the
    /// `u16` operand width, not a recoverable error.
    pub fn add_constant(&mut self, value: Value) -> u16 {
        self.constants.push(value);
        u16::try_from(self.constants.len() - 1).expect("constant pool exceeded u16::MAX entries")
    }

    /// Emits `op` with a placeholder `u16` operand and returns the offset of
    /// that operand, to be patched later via [`Chunk::patch_jump`] once the
    /// jump target is known.
    pub fn emit_jump(&mut self, op: OpCode, source_offset: u32) -> usize {
        self.write_op(op, source_offset);
        let operand_at = self.code.len();
        self.write_u16(0xFFFF, source_offset);
        operand_at
    }

    /// Patches a jump emitted at `operand_at` (as returned by
    /// [`Chunk::emit_jump`]) to land at the current end of the chunk.
    pub fn patch_jump(&mut self, operand_at: usize) {
        self.patch_jump_to(operand_at, self.code.len());
    }

    /// Patches a jump emitted at `operand_at` to land at `target` (a code
    /// offset at or after the operand — jumps are forward-only).
    pub fn patch_jump_to(&mut self, operand_at: usize, target: usize) {
        let distance = target - (operand_at + 2);
        let bytes = u16::try_from(distance).expect("jump distance exceeded u16::MAX bytes").to_be_bytes();
        self.code[operand_at] = bytes[0];
        self.code[operand_at + 1] = bytes[1];
    }
}
