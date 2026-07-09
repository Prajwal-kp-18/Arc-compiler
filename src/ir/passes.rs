//! # Optimization Passes
//!
//! Constant folding, dead code elimination, local common-subexpression
//! elimination, and dead-store elimination (via [`liveness`]), run
//! per-function to a fixpoint. Every pass obeys one law:
//! **behavior is untouchable** — including *runtime errors*. A fold that
//! would error doesn't fold (`1 / 0` stays and fails at runtime, same line,
//! same message); DCE and CSE only touch instructions whose `can_error`
//! fact (computed at lowering from the Resolver's static types) proves they
//! cannot raise. The golden suite runs the whole pipeline under `--opt` and
//! must stay byte-identical.

use std::collections::HashMap;

use crate::ast::types::{apply_binary, Value};
use crate::ast::ASTBinaryOperatorKind;

use super::instr::{InstrKind, IrFunction, IrProgram, Reg, Terminator};
use super::liveness;

/// Runs the full pass pipeline (fold → DCE → CSE, to fixpoint) on every
/// function.
pub fn optimize(program: &mut IrProgram) {
    optimize_function(&mut program.script);
    for f in &mut program.functions {
        optimize_function(f);
    }
}

fn optimize_function(f: &mut IrFunction) {
    // Converges in one or two rounds in practice; the cap is a safety net
    // against a pass-interaction cycle, not a tuning knob.
    for _ in 0..10 {
        let mut changed = fold(f);
        changed |= dce(f);
        changed |= cse(f);
        changed |= dse(f);
        if !changed {
            return;
        }
    }
}

/// How many times each register is written across the whole function.
/// Almost always 1; `&&`/`||` join results are written in two arm blocks
/// and are excluded from const-tracking and CSE caching.
fn write_counts(f: &IrFunction) -> HashMap<Reg, u32> {
    let mut counts = HashMap::new();
    for block in f.blocks.iter().filter(|b| !b.dead) {
        for instr in &block.instrs {
            if let Some(dst) = instr.kind.dst() {
                *counts.entry(dst).or_insert(0) += 1;
            }
        }
    }
    counts
}

// -- constant folding ---------------------------------------------------------

pub fn fold(f: &mut IrFunction) -> bool {
    let counts = write_counts(f);
    let mut consts: HashMap<Reg, Value> = HashMap::new();
    let mut changed = false;

    // Pre-seed with every existing Const instruction, regardless of block —
    // a plain collection pass, order-independent, so it finds constants
    // inside a loop body exactly as readily as ones before the loop.
    for block in f.blocks.iter().filter(|b| !b.dead) {
        for instr in &block.instrs {
            if let InstrKind::Const { dst, value } = &instr.kind {
                if counts.get(dst) == Some(&1) {
                    consts.insert(*dst, value.clone());
                }
            }
        }
    }

    for block in f.blocks.iter_mut().filter(|b| !b.dead) {
        for instr in &mut block.instrs {
            let folded: Option<(Reg, Value)> = match &instr.kind {
                InstrKind::Binary { dst, op, lhs, rhs, .. } => {
                    match (consts.get(lhs), consts.get(rhs)) {
                        (Some(l), Some(r)) => fold_binary(*op, l, r).map(|v| (*dst, v)),
                        _ => None,
                    }
                }
                InstrKind::Negate { dst, src, .. } => match consts.get(src) {
                    Some(Value::Integer(i)) => Some((*dst, Value::Integer(-i))),
                    Some(Value::Float(fl)) => Some((*dst, Value::Float(-fl))),
                    _ => None, // non-numeric const: leave the runtime error in place
                },
                InstrKind::Not { dst, src } => {
                    consts.get(src).map(|v| (*dst, Value::Boolean(!v.to_boolean())))
                }
                InstrKind::ToBool { dst, src } => {
                    consts.get(src).map(|v| (*dst, Value::Boolean(v.to_boolean())))
                }
                InstrKind::ToFloat { dst, src } => consts.get(src).map(|v| {
                    let widened = match v {
                        Value::Integer(i) => Value::Float(*i as f64),
                        other => other.clone(), // runtime ToFloat passes non-Integers through
                    };
                    (*dst, widened)
                }),
                _ => None,
            };

            if let Some((dst, value)) = folded {
                if counts.get(&dst) == Some(&1) {
                    consts.insert(dst, value.clone());
                }
                instr.kind = InstrKind::Const { dst, value };
                changed = true;
            }
        }

        // A branch on a constant condition becomes a jump; the untaken
        // block loses a predecessor and DCE's reachability sweep drops it.
        if let Terminator::Branch { cond, then_block, else_block } = &block.terminator {
            if let Some(v) = consts.get(cond) {
                let target = if v.to_boolean() { *then_block } else { *else_block };
                block.terminator = Terminator::Jump(target);
                changed = true;
            }
        }
    }
    changed
}

/// Folds one binary op over constants via the same `apply_binary` both
/// backends execute — a folded result can never differ from the runtime
/// result. Returns `None` (no fold) where runtime behavior must be
/// preserved: ops that would error, and `Int ** Int` cases where the
/// runtime's unchecked `i64::pow` would panic (folding those would move the
/// panic to compile time).
fn fold_binary(op: ASTBinaryOperatorKind, l: &Value, r: &Value) -> Option<Value> {
    if op == ASTBinaryOperatorKind::Exponentiation {
        if let (Value::Integer(a), Value::Integer(b)) = (l, r) {
            if *b >= 0 {
                let exp = u32::try_from(*b).ok()?;
                return a.checked_pow(exp).map(Value::Integer);
            }
            // negative exponent: falls through to apply_binary's Float path
        }
    }
    apply_binary(&op, l, r).ok()
}

// -- dead code elimination ------------------------------------------------------

pub fn dce(f: &mut IrFunction) -> bool {
    let mut changed = mark_unreachable_blocks(f);

    // Deleting an instruction can orphan its operands' definitions, so
    // sweep to a local fixpoint.
    loop {
        let mut used: std::collections::HashSet<Reg> = std::collections::HashSet::new();
        for block in f.blocks.iter_mut().filter(|b| !b.dead) {
            for instr in &mut block.instrs {
                instr.kind.for_each_use(&mut |r| {
                    used.insert(*r);
                });
            }
            match &block.terminator {
                Terminator::Branch { cond, .. } => {
                    used.insert(*cond);
                }
                Terminator::Return(r) => {
                    used.insert(*r);
                }
                Terminator::Jump(_) => {}
            }
        }

        let mut swept = false;
        for block in f.blocks.iter_mut().filter(|b| !b.dead) {
            let before = block.instrs.len();
            block.instrs.retain(|instr| {
                let dead = instr.kind.is_pure() && instr.kind.dst().is_some_and(|d| !used.contains(&d));
                !dead
            });
            swept |= block.instrs.len() != before;
        }
        if !swept {
            break;
        }
        changed = true;
    }
    changed
}

fn mark_unreachable_blocks(f: &mut IrFunction) -> bool {
    let mut reachable = vec![false; f.blocks.len()];
    let mut worklist = vec![0usize];
    while let Some(id) = worklist.pop() {
        if reachable[id] {
            continue;
        }
        reachable[id] = true;
        match &f.blocks[id].terminator {
            Terminator::Jump(b) => worklist.push(b.0 as usize),
            Terminator::Branch { then_block, else_block, .. } => {
                worklist.push(then_block.0 as usize);
                worklist.push(else_block.0 as usize);
            }
            Terminator::Return(_) => {}
        }
    }

    let mut changed = false;
    for (id, block) in f.blocks.iter_mut().enumerate() {
        if !reachable[id] && !block.dead {
            block.dead = true;
            changed = true;
        }
    }
    changed
}

// -- local common subexpression elimination ----------------------------------------

/// Value-numbering key for a repeatable computation. `Const` is not CSE'd
/// (`Value` isn't hashable; fold + DCE already collapse constant chains).
#[derive(PartialEq, Eq, Hash)]
enum Key {
    Binary(ASTBinaryOperatorKind, Reg, Reg),
    Negate(Reg),
    Not(Reg),
    ToBool(Reg),
    ToFloat(Reg),
    LoadLocal(u32),
    LoadGlobal(u32),
}

pub fn cse(f: &mut IrFunction) -> bool {
    let counts = write_counts(f);
    // dup register -> the earlier register holding the same value. Filled
    // per block (`available` below is local to each block: CSE itself is
    // block-local), applied to operands on the fly as blocks are walked in
    // id order. Safe even with `while`'s back-edge: every register still has
    // exactly one static def site (the loop header's condition is one
    // instruction, re-executed at runtime, not redefined per iteration), so
    // substitutions never point at a not-yet-processed block.
    let mut subst: HashMap<Reg, Reg> = HashMap::new();
    let mut changed = false;

    for block in f.blocks.iter_mut().filter(|b| !b.dead) {
        let mut available: HashMap<Key, Reg> = HashMap::new();

        block.instrs.retain_mut(|instr| {
            // Canonicalize operands through earlier substitutions first so
            // later duplicates match on canonical registers.
            instr.kind.for_each_use(&mut |r| {
                while let Some(rep) = subst.get(r) {
                    *r = *rep;
                }
            });

            let key = match &instr.kind {
                // Only provably non-erroring ops are CSE-able: merging two
                // *failing* ops would change how many errors the
                // tree-walker reports (design doc §5.3).
                InstrKind::Binary { op, lhs, rhs, can_error: false, .. } => Some(Key::Binary(*op, *lhs, *rhs)),
                InstrKind::Negate { src, can_error: false, .. } => Some(Key::Negate(*src)),
                InstrKind::Not { src, .. } => Some(Key::Not(*src)),
                InstrKind::ToBool { src, .. } => Some(Key::ToBool(*src)),
                InstrKind::ToFloat { src, .. } => Some(Key::ToFloat(*src)),
                InstrKind::LoadLocal { slot, .. } => Some(Key::LoadLocal(slot.0)),
                InstrKind::LoadGlobal { slot, .. } => Some(Key::LoadGlobal(slot.0)),
                _ => None,
            };

            // Invalidate stale loads before (possibly) caching this instr.
            match &instr.kind {
                InstrKind::StoreLocal { slot, .. } => {
                    available.remove(&Key::LoadLocal(slot.0));
                }
                InstrKind::StoreGlobal { slot, .. } => {
                    available.remove(&Key::LoadGlobal(slot.0));
                }
                // Calls may write globals; one conservative rule beats
                // three clever ones — drop every cached load.
                InstrKind::Call { .. } | InstrKind::CallBuiltin { .. } => {
                    available.retain(|k, _| !matches!(k, Key::LoadLocal(_) | Key::LoadGlobal(_)));
                }
                _ => {}
            }

            let (Some(key), Some(dst)) = (key, instr.kind.dst()) else {
                return true;
            };
            // Multi-write registers (&&/|| results) are never cached or
            // replaced — their value depends on which arm executed.
            if counts.get(&dst) != Some(&1) {
                return true;
            }

            if let Some(&earlier) = available.get(&key) {
                subst.insert(dst, earlier);
                changed = true;
                false // drop the duplicate computation
            } else {
                available.insert(key, dst);
                true
            }
        });

        // The block's terminator may read a substituted register.
        match &mut block.terminator {
            Terminator::Branch { cond, .. } => canonicalize(cond, &subst),
            Terminator::Return(r) => canonicalize(r, &subst),
            Terminator::Jump(_) => {}
        }
    }

    // Substitutions from one block can be used in later blocks (a dup in b1
    // whose dst is read in b3) — rewrite everything once at the end too.
    if changed {
        for block in f.blocks.iter_mut().filter(|b| !b.dead) {
            for instr in &mut block.instrs {
                instr.kind.for_each_use(&mut |r| canonicalize_mut(r, &subst));
            }
            match &mut block.terminator {
                Terminator::Branch { cond, .. } => canonicalize(cond, &subst),
                Terminator::Return(r) => canonicalize(r, &subst),
                Terminator::Jump(_) => {}
            }
        }
    }
    changed
}

fn canonicalize(r: &mut Reg, subst: &HashMap<Reg, Reg>) {
    canonicalize_mut(r, subst);
}

fn canonicalize_mut(r: &mut Reg, subst: &HashMap<Reg, Reg>) {
    while let Some(rep) = subst.get(r) {
        *r = *rep;
    }
}

// -- dead store elimination ---------------------------------------------------

/// Removes `StoreLocal`s whose slot is never read again on any path to
/// function exit (per [`liveness::analyze`]). `StoreGlobal` is never
/// touched — globals are conservatively always live. Only the store itself
/// is deleted; if that orphans its `src`'s defining instruction, the next
/// `dce` round reclaims it (and only if pure — an erroring op stays, so a
/// runtime error a dead store's value would have raised is preserved).
pub fn dse(f: &mut IrFunction) -> bool {
    let live = liveness::analyze(f);
    let mut changed = false;

    for (id, block) in f.blocks.iter_mut().enumerate() {
        if block.dead {
            continue;
        }
        let live_after = liveness::live_after_each(block, &live.live_out[id]);
        let before = block.instrs.len();
        let mut idx = 0;
        block.instrs.retain(|instr| {
            let dead = matches!(&instr.kind, InstrKind::StoreLocal { slot, .. } if !live_after[idx].contains(&slot.0));
            idx += 1;
            !dead
        });
        changed |= block.instrs.len() != before;
    }
    changed
}
