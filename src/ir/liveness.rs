//! # Slot Liveness Analysis
//!
//! Backward dataflow over local frame slots (`LoadLocal`/`StoreLocal`): a
//! slot is live at a program point if its current value may be read before
//! being overwritten along some path to function exit. Globals are out of
//! scope — top-level `let` resolves to `ResolvedBinding::Global`, and
//! `StoreGlobal` is conservatively always live (see [`super::passes::dse`]).
//! Feeds dead-store elimination and `--dump-liveness`.
//! See `local-notes/phase4.6-liveness-design.md`.

use std::collections::HashSet;

use super::instr::{Block, BlockId, InstrKind, IrFunction, Terminator};

/// Live-in/live-out slot sets per block, indexed by `BlockId.0 as usize`.
/// Dead blocks keep empty sets.
pub struct Liveness {
    pub live_in: Vec<HashSet<u32>>,
    pub live_out: Vec<HashSet<u32>>,
}

/// Runs the analysis to a fixpoint over `f`'s CFG (loop back-edges included
/// via ordinary successor edges, so loop-carried slots converge correctly).
pub fn analyze(f: &IrFunction) -> Liveness {
    let n = f.blocks.len();
    let mut live_in: Vec<HashSet<u32>> = vec![HashSet::new(); n];
    let mut live_out: Vec<HashSet<u32>> = vec![HashSet::new(); n];

    loop {
        let mut changed = false;
        for id in (0..n).rev() {
            if f.blocks[id].dead {
                continue;
            }
            let mut out = HashSet::new();
            for succ in successors(&f.blocks[id]) {
                out.extend(live_in[succ.0 as usize].iter().copied());
            }
            let inp = transfer(&f.blocks[id], &out);
            if out != live_out[id] {
                live_out[id] = out;
                changed = true;
            }
            if inp != live_in[id] {
                live_in[id] = inp;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    Liveness { live_in, live_out }
}

/// Per-instruction live-after sets within `block`, index-aligned with
/// `block.instrs`, walked backward from `live_out`. Liveness is a
/// block-boundary fact; DSE and the dump both need it *within* a block too.
pub fn live_after_each(block: &Block, live_out: &HashSet<u32>) -> Vec<HashSet<u32>> {
    let mut result = vec![HashSet::new(); block.instrs.len()];
    let mut live = live_out.clone();
    for (idx, instr) in block.instrs.iter().enumerate().rev() {
        result[idx] = live.clone();
        apply_transfer(&instr.kind, &mut live);
    }
    result
}

fn successors(block: &Block) -> Vec<BlockId> {
    match &block.terminator {
        Terminator::Jump(b) => vec![*b],
        Terminator::Branch { then_block, else_block, .. } => vec![*then_block, *else_block],
        Terminator::Return(_) => vec![],
    }
}

fn transfer(block: &Block, live_out: &HashSet<u32>) -> HashSet<u32> {
    let mut live = live_out.clone();
    for instr in block.instrs.iter().rev() {
        apply_transfer(&instr.kind, &mut live);
    }
    live
}

/// `StoreLocal` kills its slot (def); `LoadLocal` generates its slot (use).
/// Every other instruction kind — register ops, globals, calls — is
/// transparent to slot liveness.
fn apply_transfer(kind: &InstrKind, live: &mut HashSet<u32>) {
    match kind {
        InstrKind::StoreLocal { slot, .. } => {
            live.remove(&slot.0);
        }
        InstrKind::LoadLocal { slot, .. } => {
            live.insert(slot.0);
        }
        _ => {}
    }
}
