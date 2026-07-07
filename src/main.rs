//! Arc Compiler - Supports REPL mode and file execution

mod ast;
mod bytecode;
mod ir;
use ast::lexer::Token;
use ast::lexer::TokenKind;
use ast::Ast;
use ast::parser::Parser;
use ast::resolver::Resolver;
use ast::evaluator::ASTEvaluator;
use ast::diagnostic::{Diagnostic, Severity};
use std::io::{self, Write};
use std::env;
use std::fs;

/// Which execution engine runs the program. Both must produce identical
/// output on the golden suite (see `tests/golden.rs`).
#[derive(Clone, Copy, PartialEq)]
enum Backend {
    TreeWalk,
    Vm,
}

/// Entry point - runs REPL, executes a file (`--backend=tree-walk|vm`,
/// optionally `--opt` for the optimizing IR pipeline), dumps its bytecode
/// (`--dump-bytecode`), or dumps its IR (`--dump-ir`, `--dump-ir=opt`)
fn main() {
    let mut backend: Option<Backend> = None;
    let mut opt = false;
    let mut dump_bytecode = false;
    let mut dump_ir: Option<bool> = None;
    let mut filename = None;

    for arg in env::args().skip(1) {
        if arg == "--dump-bytecode" {
            dump_bytecode = true;
        } else if arg == "--opt" {
            opt = true;
        } else if arg == "--dump-ir" {
            dump_ir = Some(false);
        } else if arg == "--dump-ir=opt" {
            dump_ir = Some(true);
        } else if let Some(name) = arg.strip_prefix("--backend=") {
            backend = match name {
                "tree-walk" => Some(Backend::TreeWalk),
                "vm" => Some(Backend::Vm),
                other => {
                    eprintln!("Unknown backend '{}' (expected 'tree-walk' or 'vm')", other);
                    return;
                }
            };
        } else {
            filename = Some(arg);
        }
    }

    if opt && backend == Some(Backend::TreeWalk) {
        eprintln!("--opt runs on the VM; it cannot be combined with --backend=tree-walk");
        return;
    }

    match filename {
        Some(filename) if dump_bytecode => dump_bytecode_for_file(&filename),
        Some(filename) if dump_ir.is_some() => dump_ir_for_file(&filename, dump_ir.unwrap()),
        Some(filename) if opt => execute_file_optimized(&filename),
        Some(filename) => execute_file(&filename, backend.unwrap_or(Backend::TreeWalk)),
        None => run_repl(),
    }
}

fn print_diagnostics(header: &str, diagnostics: &[&Diagnostic], source: &str) {
    if diagnostics.is_empty() {
        return;
    }
    eprintln!("\n=== {} ===", header);
    for diagnostic in diagnostics {
        eprintln!("{}", diagnostic.format_with_source(source));
    }
}

/// Lexes, parses, and resolves a whole file, printing parse/resolve
/// diagnostics along the way. Returns `None` if a blocking error occurred
/// (evaluation/compilation should not proceed); resolve warnings are
/// printed but don't block.
fn parse_and_resolve(filename: &str, contents: &str) -> Option<(Ast, Resolver)> {
    let mut lexer = ast::lexer::Lexer::new(contents);
    let mut tokens: Vec<Token> = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token);
    }

    let mut ast: Ast = Ast::new();
    let mut parser = Parser::new(tokens);

    loop {
        match parser.current().map(|token| &token.kind) {
            Some(TokenKind::EOF) | None => break,
            _ => {}
        }

        match parser.next_statement() {
            Some(statement) => ast.add_statement(statement),
            None => {
                if parser.diagnostics.is_empty() {
                    eprintln!("Parse error: Invalid syntax in file '{}'.", filename);
                } else {
                    let refs: Vec<&Diagnostic> = parser.diagnostics.iter().collect();
                    print_diagnostics("Parse Errors", &refs, contents);
                }
                return None;
            }
        }
    }

    if !parser.diagnostics.is_empty() {
        let refs: Vec<&Diagnostic> = parser.diagnostics.iter().collect();
        print_diagnostics("Parse Errors", &refs, contents);
        return None;
    }

    // Resolve: statically resolve every name/type before evaluation/compilation runs.
    let mut resolver = Resolver::new();
    resolver.resolve(&ast);

    let (errors, warnings): (Vec<&Diagnostic>, Vec<&Diagnostic>) =
        resolver.diagnostics.iter().partition(|d| d.severity == Severity::Error);
    print_diagnostics("Resolve Warnings", &warnings, contents);
    if resolver.has_errors() {
        print_diagnostics("Resolve Errors", &errors, contents);
        return None;
    }

    Some((ast, resolver))
}

/// Reads and executes Arc source file as a complete program, on the chosen
/// backend. Output (including runtime error rendering) must be identical
/// across backends for error-free programs.
fn execute_file(filename: &str, backend: Backend) {
    let contents = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            return;
        }
    };

    println!("=== Executing {} ===", filename);

    let Some((ast, resolver)) = parse_and_resolve(filename, &contents) else {
        return;
    };

    let errors: Vec<Diagnostic> = match backend {
        Backend::TreeWalk => {
            let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
            evaluator.execute(&ast);
            evaluator.errors
        }
        Backend::Vm => {
            let compiler = bytecode::compiler::BytecodeCompiler::new(resolver.function_count());
            let program = compiler.compile(&ast);
            let mut vm = bytecode::vm::VM::new(&program, resolver.global_slot_count());
            vm.run();
            vm.errors
        }
    };

    if !errors.is_empty() {
        let refs: Vec<&Diagnostic> = errors.iter().collect();
        print_diagnostics("Errors", &refs, &contents);
    }
}

/// Executes a file through the optimizing IR pipeline:
/// AST → IR → fold/DCE/CSE → bytecode → VM. Must produce output identical
/// to both plain backends (gated by `tests/golden.rs`).
fn execute_file_optimized(filename: &str) {
    let contents = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            return;
        }
    };

    println!("=== Executing {} ===", filename);

    let Some((ast, resolver)) = parse_and_resolve(filename, &contents) else {
        return;
    };

    let mut program = ir::lower::IrLowering::new(resolver.function_count()).lower(&ast);
    ir::passes::optimize(&mut program);
    let compiled = ir::to_bytecode::program_to_bytecode(&program);
    let mut vm = bytecode::vm::VM::new(&compiled, resolver.global_slot_count());
    vm.run();

    if !vm.errors.is_empty() {
        let refs: Vec<&Diagnostic> = vm.errors.iter().collect();
        print_diagnostics("Errors", &refs, &contents);
    }
}

/// Compiles a file to IR and prints it — post-lowering by default, or after
/// the optimization pipeline with `--dump-ir=opt`. The diff between the two
/// is the "show your work" artifact for the optimization passes.
fn dump_ir_for_file(filename: &str, optimized: bool) {
    let contents = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            return;
        }
    };

    let Some((ast, resolver)) = parse_and_resolve(filename, &contents) else {
        return;
    };

    let mut program = ir::lower::IrLowering::new(resolver.function_count()).lower(&ast);
    if optimized {
        ir::passes::optimize(&mut program);
    }
    print!("{}", ir::dump::dump_program(&program));
}

/// Compiles a file to bytecode and prints its disassembly, without
/// executing it (use `--backend=vm` to run the bytecode).
fn dump_bytecode_for_file(filename: &str) {
    let contents = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            return;
        }
    };

    let Some((ast, resolver)) = parse_and_resolve(filename, &contents) else {
        return;
    };

    let compiler = bytecode::compiler::BytecodeCompiler::new(resolver.function_count());
    let program = compiler.compile(&ast);
    print!("{}", bytecode::disassembler::disassemble_program(&program));
}

/// Interactive Read-Eval-Print Loop for testing expressions
fn run_repl() {
    println!("=== Arc Compiler REPL ===");
    println!("Type expressions to evaluate them. Type 'exit' or 'quit' to exit.\n");
    println!("Examples:");
    println!("  let x = 10");
    println!("  x + 5");
    println!("  print(x)");
    println!("  // This is a comment");
    println!("  const pi = 3.14\n");

    let mut resolver = Resolver::new();
    let mut evaluator = ASTEvaluator::new(resolver.global_slot_count());
    let stdin = io::stdin();

    loop {
        print!(">> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match stdin.read_line(&mut input) {
            Ok(_) => {
                let input = input.trim();

                // Exit commands
                if input == "exit" || input == "quit" {
                    println!("Session ended. Goodbye.");
                    break;
                }

                // Skip empty lines
                if input.is_empty() {
                    continue;
                }

                // Tokenize
                let mut lexer = ast::lexer::Lexer::new(input);
                let mut tokens: Vec<Token> = Vec::new();
                while let Some(token) = lexer.next_token() {
                    tokens.push(token);
                }

                // Parse
                let mut ast: Ast = Ast::new();
                let mut parser = Parser::new(tokens);

                match parser.next_statement() {
                    Some(statement) => {
                        ast.add_statement(statement);

                        if !parser.diagnostics.is_empty() {
                            println!("Parse error:");
                            for diagnostic in &parser.diagnostics {
                                println!("  {}", diagnostic.format_with_source(input));
                            }
                        } else {
                            // Resolve this line against the persistent global scope.
                            let diag_start = resolver.diagnostics.len();
                            resolver.resolve(&ast);
                            let new_diagnostics = &resolver.diagnostics[diag_start..];
                            let has_errors = new_diagnostics.iter().any(|d| d.severity == Severity::Error);

                            for diagnostic in new_diagnostics {
                                println!("  {}", diagnostic.format_with_source(input));
                            }

                            if !has_errors {
                                evaluator.ensure_global_capacity(resolver.global_slot_count());

                                let error_count_before = evaluator.errors.len();
                                evaluator.execute(&ast);
                                let error_count_after = evaluator.errors.len();

                                if error_count_after > error_count_before {
                                    println!("Error:");
                                    for i in error_count_before..error_count_after {
                                        println!("  {}", evaluator.errors[i].format_with_source(input));
                                    }
                                } else {
                                    match &evaluator.last_value {
                                        Some(value) => {
                                            println!("{:?} : {:?}", value, value.get_type());
                                        }
                                        None => {
                                            // Statement executed without producing a value
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        println!("Parse error: Invalid syntax");
                    }
                }
            }
            Err(error) => {
                println!("Error reading input: {}", error);
                break;
            }
        }
        println!();
    }
}
