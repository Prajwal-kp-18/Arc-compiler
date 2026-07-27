//! # Disassembler
//!
//! A pretty-printer for [`Chunk`]s — the tool for eyeballing generated
//! bytecode while hand-tracing programs, and for debugging the VM once
//! Phase 3 exists. Wired up behind `arc --dump-bytecode file.arc`.

use crate::ast::resolver::BuiltinFn;
use crate::bytecode::chunk::Chunk;
use crate::bytecode::compiler::CompiledProgram;
use crate::bytecode::opcode::OpCode;

/// Disassembles every chunk in a compiled program: the script, then each
/// function in `FunctionId` order.
pub fn disassemble_program(program: &CompiledProgram) -> String {
    let mut out = disassemble_chunk(&program.script, "script");
    for (id, function) in program.functions.iter().enumerate() {
        out.push('\n');
        out.push_str(&disassemble_chunk(&function.chunk, &format!("fn#{} {}", id, function.name)));
    }
    out
}

pub fn disassemble_chunk(chunk: &Chunk, name: &str) -> String {
    let mut out = format!("== {} ==\n", name);
    let mut offset = 0;
    while offset < chunk.code.len() {
        offset = disassemble_instruction(chunk, offset, &mut out);
    }
    out
}

/// Disassembles one instruction at `offset`, appending its line to `out`,
/// and returns the offset of the next instruction.
fn disassemble_instruction(chunk: &Chunk, offset: usize, out: &mut String) -> usize {
    let src_offset = chunk.offsets[offset];
    let op = OpCode::from_byte(chunk.code[offset]).expect("invalid opcode byte in chunk");
    let prefix = format!("{:04}  src:{:<5}", offset, src_offset);

    match op {
        OpCode::Constant => {
            let idx = chunk.read_u16(offset + 1);
            out.push_str(&format!("{} {:<16} {:4} '{:?}'\n", prefix, op.mnemonic(), idx, chunk.constants[idx as usize]));
            offset + 3
        }
        OpCode::GetLocal | OpCode::SetLocal | OpCode::GetGlobal | OpCode::SetGlobal => {
            let slot = chunk.read_u16(offset + 1);
            out.push_str(&format!("{} {:<16} {:4}\n", prefix, op.mnemonic(), slot));
            offset + 3
        }
        OpCode::Jump | OpCode::JumpIfFalse => {
            let jump = chunk.read_u16(offset + 1);
            let target = offset + 3 + jump as usize;
            out.push_str(&format!("{} {:<16} {:4} -> {:04}\n", prefix, op.mnemonic(), jump, target));
            offset + 3
        }
        OpCode::Loop => {
            let distance = chunk.read_u16(offset + 1);
            let target = offset + 3 - distance as usize;
            out.push_str(&format!("{} {:<16} {:4} -> {:04}\n", prefix, op.mnemonic(), distance, target));
            offset + 3
        }
        OpCode::Call => {
            let function_id = chunk.read_u16(offset + 1);
            let argc = chunk.code[offset + 3];
            out.push_str(&format!("{} {:<16} fn#{} argc={}\n", prefix, op.mnemonic(), function_id, argc));
            offset + 4
        }
        OpCode::CallBuiltin => {
            let builtin_id = chunk.code[offset + 1];
            let argc = chunk.code[offset + 2];
            let name = BuiltinFn::from_u8(builtin_id).map(BuiltinFn::name).unwrap_or("?");
            out.push_str(&format!("{} {:<16} {:<8} argc={}\n", prefix, op.mnemonic(), name, argc));
            offset + 3
        }
        // No operand.
        OpCode::True
        | OpCode::False
        | OpCode::Pop
        | OpCode::ToFloat
        | OpCode::Add
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
        | OpCode::GreaterEqual
        | OpCode::Negate
        | OpCode::Not
        | OpCode::Return => {
            out.push_str(&format!("{} {}\n", prefix, op.mnemonic()));
            offset + 1
        }
    }
}
