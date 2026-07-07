//! # Bytecode
//!
//! Phase 2 of the compiler pipeline: lowers a Resolver-annotated AST into
//! [`chunk::Chunk`]s of bytecode. No execution happens in this module — that
//! is Phase 3's job. See `local-notes/phase2-bytecode-design.md` for the
//! full design.

pub mod opcode;
pub mod chunk;
pub mod compiler;
pub mod disassembler;
