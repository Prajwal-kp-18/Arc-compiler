//! Golden-suite backend parity: every program here must produce
//! byte-identical stdout/stderr whether executed by the tree-walking
//! evaluator or the bytecode VM — the Phase 3 exit criterion, checked
//! end-to-end through the real CLI binary.

use std::fs;
use std::process::Command;

const PROGRAMS: &[(&str, &str)] = &[
    ("arithmetic", "print(1 + 2 * 3); print(2 ** 10); print(7 / 2); print(10 % 3); print(1 + 2.5);"),
    ("strings", r#"print("Hello, " + "World!"); print("a" < "b"); print("x" == "x");"#),
    ("bitwise", "print(12 & 10, 12 | 10, 12 ^ 10, 8 << 2, 32 >> 2);"),
    ("logic", "print(true && false); print(true || false); print(!true); print(false && print(\"skipped\"));"),
    (
        "variables",
        "let x = 10; const PI = 3.14159; x = x + 5; print(x, PI); let y = 1.5; y = 2; print(y + 0.5);",
    ),
    (
        "scoping",
        r#"let v = 5; { let v = 99; print("inner:", v); } print("outer:", v);"#,
    ),
    (
        "if_else",
        r#"let s = 10; if s == 10 { print("perfect"); } else { print("try again"); } if s > 100 { print("huge"); } else { print("small"); }"#,
    ),
    (
        "functions",
        "fn add(a: Int, b: Int) { return a + b; } fn sq(n: Int) { return n * n; } print(add(2, 3), sq(7), add(sq(2), sq(3)));",
    ),
    (
        "recursion",
        "fn fact(n: Int) { if n <= 1 { return 1; } else { return n * fact(n - 1); } } print(fact(10));",
    ),
    (
        "mutual_recursion",
        "fn is_even(n: Int) { if n == 0 { return true; } else { return is_odd(n - 1); } } \
         fn is_odd(n: Int) { if n == 0 { return false; } else { return is_even(n - 1); } } \
         print(is_even(10), is_odd(10));",
    ),
    (
        "inc_dec",
        "let c = 10; print(++c); print(c++); print(c); print(--c); print(c--); print(c);",
    ),
    ("builtins", "print(min(4, 2, 9, 1)); print(max(4, 2, 9, 1)); print(min(3.5, 2, 4.1)); print(max(\"a\", \"b\"));"),
    (
        "implicit_return",
        "fn f() { 7; } print(f());",
    ),
];

fn run(exe: &str, file: &std::path::Path, backend: &str) -> (String, String, bool) {
    let output = Command::new(exe)
        .arg(file)
        .arg(format!("--backend={}", backend))
        .output()
        .expect("failed to launch arc binary");
    (
        String::from_utf8(output.stdout).expect("stdout not utf-8"),
        String::from_utf8(output.stderr).expect("stderr not utf-8"),
        output.status.success(),
    )
}

#[test]
fn golden_suite_is_identical_on_both_backends() {
    let exe = env!("CARGO_BIN_EXE_arc");
    let dir = std::env::temp_dir().join("arc-golden-suite");
    fs::create_dir_all(&dir).expect("failed to create temp dir");

    for (name, source) in PROGRAMS {
        let file = dir.join(format!("{}.arc", name));
        fs::write(&file, source).expect("failed to write program");

        let (tree_out, tree_err, tree_ok) = run(exe, &file, "tree-walk");
        let (vm_out, vm_err, vm_ok) = run(exe, &file, "vm");

        assert!(tree_ok && vm_ok, "{}: non-zero exit (tree: {}, vm: {})", name, tree_ok, vm_ok);
        assert_eq!(tree_out, vm_out, "{}: stdout diverged", name);
        assert_eq!(tree_err, vm_err, "{}: stderr diverged", name);
        // A golden program actually prints something — guards against both
        // backends "agreeing" by silently doing nothing.
        assert!(tree_out.lines().count() > 1, "{}: produced no output", name);
    }
}

/// The repo's demo program exercises every documented feature at once; it
/// must also be backend-agnostic.
#[test]
fn demo_arc_is_identical_on_both_backends() {
    let exe = env!("CARGO_BIN_EXE_arc");
    let demo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo.arc");

    let (tree_out, tree_err, tree_ok) = run(exe, &demo, "tree-walk");
    let (vm_out, vm_err, vm_ok) = run(exe, &demo, "vm");

    assert!(tree_ok && vm_ok);
    assert_eq!(tree_out, vm_out, "demo.arc: stdout diverged");
    assert_eq!(tree_err, vm_err, "demo.arc: stderr diverged");
    assert!(tree_out.contains("Demo Complete"));
}
