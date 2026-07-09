// Arc Compiler - Liveness Analysis & Dead-Store Elimination Demo
//
// Liveness only tracks LOCAL slots (function bodies) - top-level `let` is
// a global, always conservatively live, and never touched by DSE. So every
// scenario below lives inside a function.
//
// Inspect it with:
//   cargo run -- examples/liveness_demo.arc --dump-liveness   (per-instruction live-after sets)
//   cargo run -- examples/liveness_demo.arc --dump-ir=opt     (dead stores gone after DSE)
//   cargo run -- examples/liveness_demo.arc --opt             (run it through the optimized pipeline)

print("=== Liveness Analysis Demo ===")

// 1) Dead store: `waste` is computed and stored but never read again.
//    --dump-liveness shows its store with live-after={}; --dump-ir=opt
//    shows both the store and the multiply that fed it are gone.
fn straight_line(n: Int) {
    let waste = n * 99;
    let result = n + 1;
    return result;
}
print("straight_line(5) =", straight_line(5));

// 2) Killed store: `x` is assigned twice before ever being read. The first
//    assignment is dead (overwritten before use); only the second survives.
fn reassignment(n: Int) {
    let x = n;
    x = n * 2;
    return x;
}
print("reassignment(5) =", reassignment(5));

// 3) Branch-arm liveness: `shared` is read only in the else arm, so it must
//    be live-out of the header block - the union of both arms' live-in
//    sets, not just whichever arm happens to run.
fn branch_liveness(cond: Bool, n: Int) {
    let shared = n * 3;
    if cond {
        return 1;
    } else {
        return shared;
    }
}
print("branch_liveness(true, 5) =", branch_liveness(true, 5));
print("branch_liveness(false, 5) =", branch_liveness(false, 5));

// 4) Loop-carried liveness: `total`'s store inside the loop body is read
//    again by the next iteration via the back-edge, so it stays live
//    across every pass of the fixpoint - never eliminated, even though a
//    single straight-line scan could never prove that on its own.
fn loop_liveness(n: Int) {
    let total = 0;
    let i = 0;
    while i < n {
        total = total + i;
        i = i + 1;
    }
    return total;
}
print("loop_liveness(5) =", loop_liveness(5));

print("=== Demo Complete ===")
