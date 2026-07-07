//! # Bytecode
//!
//! Phases 2 & 3 of the compiler pipeline: lowers a Resolver-annotated AST
//! into [`chunk::Chunk`]s of bytecode ([`compiler`]) and executes them on a
//! stack-based virtual machine ([`vm`]). See
//! `local-notes/phase2-bytecode-design.md` and
//! `local-notes/phase3-vm-design.md` for the full designs.

pub mod opcode;
pub mod chunk;
pub mod compiler;
pub mod disassembler;
pub mod vm;
