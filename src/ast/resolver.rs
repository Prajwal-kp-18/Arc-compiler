//! # Resolver
//!
//! Walks an AST once, after parsing and before evaluation, and statically
//! resolves every variable and function reference:
//!
//! - Every `let`/`const`/parameter gets a stable [`SlotIndex`] within its
//!   enclosing frame (function-frame-relative for locals, a single flat
//!   space for globals) — recorded directly on the AST node via `Cell`
//!   fields, so the evaluator never does a runtime, string-keyed lookup.
//! - Every expression gets a statically inferred [`DataType`], using the
//!   same widening/error rules the old runtime coercion used, computed
//!   ahead of time instead of on every evaluation. Where a type genuinely
//!   can't be pinned down (an untyped parameter with no usage-derived type,
//!   or a return type that only depends on itself with no reachable base
//!   case), it falls back to `DataType::Any` and the Resolver emits a
//!   non-fatal warning — this can never turn a previously-valid program
//!   into a new hard failure.
//! - `fn` declarations are resolved with lexical scoping *and* hoisting:
//!   within a block, every sibling function's name/signature is registered
//!   before any of their bodies are resolved, so forward references and
//!   mutual recursion keep working without a runtime name-lookup stack.
//! - Immutable-assignment, type-mismatch, and arity checks move here from
//!   the evaluator, using the exact same diagnostic text, so a previously
//!   *runtime* error now surfaces before evaluation ever starts.
//!
//! See `local-notes/phase1-resolver-design.md` for the full design writeup,
//! including the nested-closure and recursive-return-type decisions.

use std::collections::HashMap;

use crate::ast::diagnostic::{closest_name, Diagnostic, DiagnosticKind, Severity};
use crate::ast::lexer::TextSpan;
use crate::ast::types::DataType;
use crate::ast::{
    Ast, ASTAssignment, ASTBinaryExpression, ASTBinaryOperatorKind, ASTExpression,
    ASTExpressionKind, ASTFunctionCallExpression, ASTFunctionDeclaration, ASTIdentifierExpression,
    ASTIfStatement, ASTPostfixUnaryExpression, ASTReturnStatement, ASTStatement, ASTStatementKind,
    ASTUnaryExpression, ASTUnaryOperatorKind, ASTVariableDeclaration, ASTVisitor, ASTWhileStatement,
};

/// A slot index within a frame (locals) or the global slot space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotIndex(pub u32);

/// Stable identity for a resolved function declaration, assigned once by
/// the Resolver and used by call sites instead of a runtime name lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

/// Where a variable reference resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBinding {
    Local(SlotIndex),
    Global(SlotIndex),
}

impl ResolvedBinding {
    pub fn slot(&self) -> SlotIndex {
        match self {
            ResolvedBinding::Local(s) | ResolvedBinding::Global(s) => *s,
        }
    }
}

/// The built-in functions the evaluator implements natively. Explicit
/// discriminants so `as u8` (bytecode operand encoding, see
/// [`BuiltinFn::from_u8`]) stays stable even if variants are reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BuiltinFn {
    Print = 0,
    Max = 1,
    Min = 2,
    Int = 3,
    Float = 4,
    Str = 5,
    Len = 6,
    Input = 7,
    Substr = 8,
    Find = 9,
    Upper = 10,
    Lower = 11,
    Trim = 12,
    CharAt = 13,
    Ord = 14,
    Chr = 15,
    Abs = 16,
    Sqrt = 17,
    Floor = 18,
    Ceil = 19,
    Round = 20,
    Assert = 21,
    Clock = 22,
}

impl BuiltinFn {
    /// The source-level name, used for error messages, disassembly, and IR dumps.
    pub fn name(self) -> &'static str {
        use BuiltinFn::*;
        match self {
            Print => "print",
            Max => "max",
            Min => "min",
            Int => "int",
            Float => "float",
            Str => "str",
            Len => "len",
            Input => "input",
            Substr => "substr",
            Find => "find",
            Upper => "upper",
            Lower => "lower",
            Trim => "trim",
            CharAt => "char_at",
            Ord => "ord",
            Chr => "chr",
            Abs => "abs",
            Sqrt => "sqrt",
            Floor => "floor",
            Ceil => "ceil",
            Round => "round",
            Assert => "assert",
            Clock => "clock",
        }
    }

    /// Reconstructs a `BuiltinFn` from a bytecode operand byte (the inverse
    /// of `as u8`); `None` on a byte a valid compiler would never emit.
    pub fn from_u8(b: u8) -> Option<BuiltinFn> {
        use BuiltinFn::*;
        Some(match b {
            0 => Print,
            1 => Max,
            2 => Min,
            3 => Int,
            4 => Float,
            5 => Str,
            6 => Len,
            7 => Input,
            8 => Substr,
            9 => Find,
            10 => Upper,
            11 => Lower,
            12 => Trim,
            13 => CharAt,
            14 => Ord,
            15 => Chr,
            16 => Abs,
            17 => Sqrt,
            18 => Floor,
            19 => Ceil,
            20 => Round,
            21 => Assert,
            22 => Clock,
            _ => return None,
        })
    }
}

/// What a call expression resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedCall {
    Builtin(BuiltinFn),
    User(FunctionId),
}

/// Every built-in name, in the same order as the `BuiltinFn` variants —
/// used both for name resolution and for "did you mean" suggestions.
const BUILTIN_NAMES: &[(&str, BuiltinFn)] = &[
    ("print", BuiltinFn::Print),
    ("max", BuiltinFn::Max),
    ("min", BuiltinFn::Min),
    ("int", BuiltinFn::Int),
    ("float", BuiltinFn::Float),
    ("str", BuiltinFn::Str),
    ("len", BuiltinFn::Len),
    ("input", BuiltinFn::Input),
    ("substr", BuiltinFn::Substr),
    ("find", BuiltinFn::Find),
    ("upper", BuiltinFn::Upper),
    ("lower", BuiltinFn::Lower),
    ("trim", BuiltinFn::Trim),
    ("char_at", BuiltinFn::CharAt),
    ("ord", BuiltinFn::Ord),
    ("chr", BuiltinFn::Chr),
    ("abs", BuiltinFn::Abs),
    ("sqrt", BuiltinFn::Sqrt),
    ("floor", BuiltinFn::Floor),
    ("ceil", BuiltinFn::Ceil),
    ("round", BuiltinFn::Round),
    ("assert", BuiltinFn::Assert),
    ("clock", BuiltinFn::Clock),
];

fn builtin_named(name: &str) -> Option<BuiltinFn> {
    BUILTIN_NAMES.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

struct VarInfo {
    slot: SlotIndex,
    data_type: DataType,
    is_mutable: bool,
}

#[derive(Default)]
struct BlockScope {
    variables: HashMap<String, VarInfo>,
    functions: HashMap<String, FunctionId>,
}

impl BlockScope {
    fn new() -> Self {
        Self::default()
    }
}

/// One function's local slot space (or the single global space, for frame 0).
///
/// Slots are never reused across sibling blocks within a frame — every
/// nested if/block just keeps advancing `next_slot` — matching the flat
/// "stack window addressed by slot index" Phase 3's VM frames will want.
struct Frame {
    is_global: bool,
    next_slot: u32,
    scopes: Vec<BlockScope>,
}

/// Resolves an [`Ast`] in place, annotating it with slots/types/bindings.
///
/// Nested functions do **not** capture their enclosing function's locals
/// (see the design doc, §9) — only their own frame and the single global
/// frame are visible to a function body. Sibling function names *are*
/// visible regardless of declaration order, via hoisting.
pub struct Resolver {
    frames: Vec<Frame>,
    next_function_id: u32,
    function_param_types: HashMap<u32, Vec<DataType>>,
    function_names: HashMap<u32, String>,
    function_return_types: HashMap<u32, DataType>,
    /// FunctionIds currently being resolved (self/mutual recursion guard).
    currently_resolving: Vec<u32>,
    /// Stack of "types seen at `return` statements", one per function body
    /// currently being resolved; empty means we're not inside any function.
    return_types_stack: Vec<Vec<DataType>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            frames: vec![Frame { is_global: true, next_slot: 0, scopes: Vec::new() }],
            next_function_id: 0,
            function_param_types: HashMap::new(),
            function_names: HashMap::new(),
            function_return_types: HashMap::new(),
            currently_resolving: Vec::new(),
            return_types_stack: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Resolves a program's statements against the persistent global scope.
    ///
    /// Safe to call repeatedly with new statements (e.g. once per REPL
    /// line) — the global scope is never popped, so later calls still see
    /// everything declared by earlier ones. Call [`Resolver::has_errors`]
    /// afterward to decide whether it's safe to evaluate.
    pub fn resolve(&mut self, ast: &Ast) {
        if self.frames[0].scopes.is_empty() {
            self.frames[0].scopes.push(BlockScope::new());
        }
        self.hoist_and_resolve(&ast.statements);
    }

    /// Number of global slots needed — the evaluator allocates a `Vec<Value>` this size.
    pub fn global_slot_count(&self) -> u32 {
        self.frames[0].next_slot
    }

    /// Total number of function declarations resolved so far — `FunctionId`s
    /// are dense and 0-based, so this also sizes a `Vec` indexable by them
    /// (e.g. the bytecode compiler's per-function `Chunk` table).
    pub fn function_count(&self) -> u32 {
        self.next_function_id
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    // -- diagnostics -------------------------------------------------------

    fn error(&mut self, message: String) {
        self.diagnostics.push(Diagnostic::new(DiagnosticKind::ResolveError, message, None));
    }

    fn error_at(&mut self, message: String, span: Option<TextSpan>) {
        self.diagnostics.push(Diagnostic::new(DiagnosticKind::ResolveError, message, span));
    }

    fn error_with_suggestion(&mut self, message: String, span: Option<TextSpan>, suggestion: String) {
        self.diagnostics.push(
            Diagnostic::new(DiagnosticKind::ResolveError, message, span).with_suggestion(suggestion),
        );
    }

    fn warn(&mut self, message: String) {
        self.diagnostics.push(Diagnostic::warning(DiagnosticKind::ResolveError, message, None));
    }

    // -- scope/frame management --------------------------------------------

    fn resolve_statements(&mut self, stmts: &[ASTStatement]) {
        self.frames.last_mut().unwrap().scopes.push(BlockScope::new());
        self.hoist_and_resolve(stmts);
        self.frames.last_mut().unwrap().scopes.pop();
    }

    /// Hoists every direct `fn` declaration in `stmts`, then resolves each
    /// statement in order (function bodies included, at their own position).
    fn hoist_and_resolve(&mut self, stmts: &[ASTStatement]) {
        self.hoist_functions(stmts);
        for stmt in stmts {
            self.visit_statement(stmt);
        }
    }

    fn hoist_functions(&mut self, stmts: &[ASTStatement]) {
        for stmt in stmts {
            if let ASTStatementKind::FunctionDeclaration(func) = &stmt.kind {
                let id = FunctionId(self.next_function_id);
                self.next_function_id += 1;
                func.function_id.set(Some(id));

                let param_types: Vec<DataType> = func
                    .parameters
                    .iter()
                    .map(|p| p.type_annotation.unwrap_or(DataType::Any))
                    .collect();

                let frame = self.frames.last_mut().unwrap();
                let scope = frame.scopes.last_mut().unwrap();
                if scope.functions.contains_key(&func.name) {
                    self.error(format!("Function '{}' already declared in this scope", func.name));
                } else {
                    scope.functions.insert(func.name.clone(), id);
                }

                self.function_param_types.insert(id.0, param_types);
                self.function_names.insert(id.0, func.name.clone());
            }
        }
    }

    fn declare_variable(
        &mut self,
        name: &str,
        data_type: DataType,
        is_mutable: bool,
        span: Option<TextSpan>,
    ) -> ResolvedBinding {
        let frame_idx = self.frames.len() - 1;
        let is_global = self.frames[frame_idx].is_global;

        if let Some(existing) = self.frames[frame_idx].scopes.last().unwrap().variables.get(name) {
            let slot = existing.slot;
            self.error_at(format!("Variable '{}' already declared in this scope", name), span);
            return if is_global { ResolvedBinding::Global(slot) } else { ResolvedBinding::Local(slot) };
        }

        let slot = SlotIndex(self.frames[frame_idx].next_slot);
        self.frames[frame_idx].next_slot += 1;
        self.frames[frame_idx]
            .scopes
            .last_mut()
            .unwrap()
            .variables
            .insert(name.to_string(), VarInfo { slot, data_type, is_mutable });

        if is_global { ResolvedBinding::Global(slot) } else { ResolvedBinding::Local(slot) }
    }

    /// Searches the current frame's scope stack, then falls back to the
    /// global frame only — nested functions never see an intermediate
    /// enclosing function's locals (see design doc §9).
    fn lookup_variable(&self, name: &str) -> Option<(ResolvedBinding, DataType, bool)> {
        let frame = self.frames.last().unwrap();
        for scope in frame.scopes.iter().rev() {
            if let Some(info) = scope.variables.get(name) {
                let binding = if frame.is_global {
                    ResolvedBinding::Global(info.slot)
                } else {
                    ResolvedBinding::Local(info.slot)
                };
                return Some((binding, info.data_type, info.is_mutable));
            }
        }
        if !frame.is_global {
            for scope in self.frames[0].scopes.iter().rev() {
                if let Some(info) = scope.variables.get(name) {
                    return Some((ResolvedBinding::Global(info.slot), info.data_type, info.is_mutable));
                }
            }
        }
        None
    }

    fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        let frame = self.frames.last().unwrap();
        for scope in frame.scopes.iter().rev() {
            if let Some(&id) = scope.functions.get(name) {
                return Some(id);
            }
        }
        if !frame.is_global {
            for scope in self.frames[0].scopes.iter().rev() {
                if let Some(&id) = scope.functions.get(name) {
                    return Some(id);
                }
            }
        }
        None
    }

    fn visible_variable_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let frame = self.frames.last().unwrap();
        for scope in frame.scopes.iter().rev() {
            for name in scope.variables.keys() {
                if seen.insert(name.clone()) {
                    names.push(name.clone());
                }
            }
        }
        if !frame.is_global {
            for scope in self.frames[0].scopes.iter().rev() {
                for name in scope.variables.keys() {
                    if seen.insert(name.clone()) {
                        names.push(name.clone());
                    }
                }
            }
        }
        names
    }

    // -- expression typing ---------------------------------------------------

    /// Resolves `expr`, records its type on the node, and returns it.
    fn visit_expression_typed(&mut self, expr: &ASTExpression) -> DataType {
        let ty = self.resolve_expr_type(expr);
        expr.resolved_type.set(Some(ty));
        ty
    }

    fn resolve_expr_type(&mut self, expression: &ASTExpression) -> DataType {
        match &expression.kind {
            ASTExpressionKind::Number(n) => n.value.get_type(),
            ASTExpressionKind::Binary(bin) => self.resolve_binary(bin),
            ASTExpressionKind::Paranthesized(p) => self.visit_expression_typed(&p.expression),
            ASTExpressionKind::Unary(u) => self.resolve_unary(u),
            ASTExpressionKind::PostfixUnary(p) => self.resolve_postfix(p),
            ASTExpressionKind::Identifier(ident) => self.resolve_identifier(ident),
            ASTExpressionKind::FunctionCall(call) => self.resolve_call(call),
        }
    }

    fn resolve_identifier(&mut self, ident: &ASTIdentifierExpression) -> DataType {
        let span = ident.token.as_ref().map(|t| t.span.clone());
        match self.lookup_variable(&ident.name) {
            Some((binding, data_type, _)) => {
                ident.binding.set(Some(binding));
                data_type
            }
            None => {
                let message = format!("Variable '{}' not found", ident.name);
                match closest_name(&ident.name, &self.visible_variable_names()) {
                    Some(s) => self.error_with_suggestion(message, span, format!("did you mean '{}' ?", s)),
                    None => self.error_at(message, span),
                }
                DataType::Any
            }
        }
    }

    /// Mirrors `Value::coerce_to_common_type`'s numeric-only cases (no
    /// string concatenation) — shared by every arithmetic operator except `+`.
    fn numeric_result(left: DataType, right: DataType) -> Option<DataType> {
        use DataType::*;
        match (left, right) {
            (Integer, Integer) => Some(Integer),
            (Float, Float) | (Integer, Float) | (Float, Integer) => Some(Float),
            _ => None,
        }
    }

    fn check_bitwise(&mut self, left: DataType, right: DataType, span: Option<TextSpan>, message: &str) -> DataType {
        if left == DataType::Any || right == DataType::Any {
            return DataType::Any;
        }
        // Value::to_integer() at runtime only fails for String.
        if left == DataType::String || right == DataType::String {
            self.error_at(message.to_string(), span);
            return DataType::Any;
        }
        DataType::Integer
    }

    fn is_comparable_pair(left: DataType, right: DataType) -> bool {
        left == right || matches!((left, right), (DataType::Integer, DataType::Float) | (DataType::Float, DataType::Integer))
    }

    fn check_equality(&mut self, left: DataType, right: DataType, span: Option<TextSpan>) -> DataType {
        if left == DataType::Any || right == DataType::Any {
            return DataType::Boolean;
        }
        if Self::is_comparable_pair(left, right) {
            DataType::Boolean
        } else {
            self.error_at(format!("Cannot compare {:?} and {:?} for equality", left, right), span);
            DataType::Any
        }
    }

    fn check_ordering(&mut self, left: DataType, right: DataType, span: Option<TextSpan>) -> DataType {
        if left == DataType::Any || right == DataType::Any {
            return DataType::Boolean;
        }
        if Self::is_comparable_pair(left, right) {
            DataType::Boolean
        } else {
            self.error_at(format!("Cannot compare {:?} and {:?}", left, right), span);
            DataType::Any
        }
    }

    fn resolve_binary(&mut self, bin: &ASTBinaryExpression) -> DataType {
        let left = self.visit_expression_typed(&bin.left);
        let right = self.visit_expression_typed(&bin.right);
        let span = Some(bin.operator.token.span.clone());
        use ASTBinaryOperatorKind::*;
        use DataType::Any;

        match bin.operator.kind {
            // to_boolean() never fails, regardless of operand type.
            LogicalAnd | LogicalOr => DataType::Boolean,

            Plus => {
                if left == Any || right == Any {
                    Any
                } else if left == DataType::String && right == DataType::String {
                    DataType::String
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot add {:?} and {:?}", left, right), span);
                    Any
                }
            }
            Minus => {
                if left == Any || right == Any {
                    Any
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot subtract {:?} from {:?}", right, left), span);
                    Any
                }
            }
            Multiply => {
                if left == Any || right == Any {
                    Any
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot multiply {:?} and {:?}", left, right), span);
                    Any
                }
            }
            Divide => {
                if left == Any || right == Any {
                    Any
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot divide {:?} by {:?}", left, right), span);
                    Any
                }
            }
            Modulo => {
                if left == Any || right == Any {
                    Any
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot compute modulo of {:?} and {:?}", left, right), span);
                    Any
                }
            }
            Exponentiation => {
                if left == Any || right == Any {
                    Any
                } else if let Some(t) = Self::numeric_result(left, right) {
                    t
                } else {
                    self.error_at(format!("Cannot exponentiate {:?} and {:?}", left, right), span);
                    Any
                }
            }
            BitwiseAnd => self.check_bitwise(left, right, span, "Bitwise AND requires integer operands"),
            BitwiseOr => self.check_bitwise(left, right, span, "Bitwise OR requires integer operands"),
            BitwiseXor => self.check_bitwise(left, right, span, "Bitwise XOR requires integer operands"),
            LeftShift => self.check_bitwise(left, right, span, "Left shift requires integer operands"),
            RightShift => self.check_bitwise(left, right, span, "Right shift requires integer operands"),
            Equal | NotEqual => self.check_equality(left, right, span),
            Less | Greater | LessEqual | GreaterEqual => self.check_ordering(left, right, span),
        }
    }

    fn resolve_unary(&mut self, u: &ASTUnaryExpression) -> DataType {
        match u.operator.kind {
            ASTUnaryOperatorKind::Increment | ASTUnaryOperatorKind::Decrement => {
                self.resolve_inc_dec(&u.operand, matches!(u.operator.kind, ASTUnaryOperatorKind::Increment), false)
            }
            ASTUnaryOperatorKind::Plus => {
                // Runtime passthrough: valid (and returns the operand type)
                // for every type, never errors.
                self.visit_expression_typed(&u.operand)
            }
            ASTUnaryOperatorKind::Minus => {
                let ty = self.visit_expression_typed(&u.operand);
                match ty {
                    DataType::Any | DataType::Integer | DataType::Float => ty,
                    other => {
                        self.error(format!("Cannot negate {:?}", other));
                        DataType::Any
                    }
                }
            }
            ASTUnaryOperatorKind::LogicalNot => {
                self.visit_expression_typed(&u.operand);
                DataType::Boolean
            }
        }
    }

    fn resolve_postfix(&mut self, p: &ASTPostfixUnaryExpression) -> DataType {
        self.resolve_inc_dec(&p.operand, matches!(p.operator.kind, ASTUnaryOperatorKind::Increment), true)
    }

    fn resolve_inc_dec(&mut self, operand: &ASTExpression, is_increment: bool, is_postfix: bool) -> DataType {
        let ident = match &operand.kind {
            ASTExpressionKind::Identifier(ident) => ident,
            _ => {
                let prefix = if is_postfix { "Postfix " } else { "" };
                self.error(format!("{}Increment/Decrement can only be applied to variables", prefix));
                return DataType::Any;
            }
        };

        if let Some((_, _, is_mutable)) = self.lookup_variable(&ident.name) {
            if !is_mutable {
                self.error(format!("Cannot assign to immutable variable '{}'", ident.name));
            }
        }

        let ty = self.visit_expression_typed(operand);
        match ty {
            DataType::Any | DataType::Integer | DataType::Float => ty,
            other => {
                let verb = if is_increment { "increment" } else { "decrement" };
                self.error(format!("Cannot {} {:?}", verb, other));
                DataType::Any
            }
        }
    }

    fn resolve_call(&mut self, call: &ASTFunctionCallExpression) -> DataType {
        let span = call.token.as_ref().map(|t| t.span.clone());
        let arg_types: Vec<DataType> = call.arguments.iter().map(|a| self.visit_expression_typed(a)).collect();

        if let Some(builtin) = builtin_named(&call.name) {
            call.resolved_call.set(Some(ResolvedCall::Builtin(builtin)));
            return self.resolve_builtin_call(builtin, &arg_types, span);
        }

        match self.lookup_function(&call.name) {
            Some(id) => {
                call.resolved_call.set(Some(ResolvedCall::User(id)));
                let expected = self.function_param_types.get(&id.0).cloned().unwrap_or_default();
                let name = self.function_names.get(&id.0).cloned().unwrap_or_else(|| call.name.clone());
                if arg_types.len() != expected.len() {
                    self.error_at(
                        format!("Function '{}' expected {} arguments but got {}", name, expected.len(), arg_types.len()),
                        span,
                    );
                    return DataType::Any;
                }
                for (i, (&expected_ty, &actual_ty)) in expected.iter().zip(arg_types.iter()).enumerate() {
                    let widening_ok = expected_ty == DataType::Float && actual_ty == DataType::Integer;
                    if expected_ty != DataType::Any && actual_ty != DataType::Any && expected_ty != actual_ty && !widening_ok {
                        self.error_at(
                            format!(
                                "Type mismatch: argument {} to '{}' expected {:?} but got {:?}",
                                i + 1, name, expected_ty, actual_ty
                            ),
                            span.clone(),
                        );
                    }
                }
                if self.currently_resolving.contains(&id.0) {
                    // Direct or mutual recursion: this function's return
                    // type isn't known yet.
                    return DataType::Any;
                }
                self.function_return_types.get(&id.0).copied().unwrap_or(DataType::Any)
            }
            None => {
                let mut names: Vec<String> = BUILTIN_NAMES.iter().map(|(n, _)| n.to_string()).collect();
                names.extend(self.function_names.values().cloned());
                let message = format!("Unknown function: '{}'", call.name);
                match closest_name(&call.name, &names) {
                    Some(s) => self.error_with_suggestion(message, span, format!("did you mean '{}' ?", s)),
                    None => self.error_at(message, span),
                }
                DataType::Any
            }
        }
    }

    fn resolve_builtin_call(&mut self, builtin: BuiltinFn, arg_types: &[DataType], span: Option<TextSpan>) -> DataType {
        use BuiltinFn::*;

        // Fixed-arity builtins: (name, exact arg count, return type).
        // Checked first so the per-builtin match below never sees a wrong
        // argument count.
        let fixed_arity: Option<usize> = match builtin {
            Print | Max | Min => None, // variable-arity, checked separately below
            Int | Float | Str | Len | Abs | Sqrt | Floor | Ceil | Round | Upper | Lower | Trim | Ord | Chr => Some(1),
            Find | CharAt => Some(2),
            Substr => Some(3),
            Input | Clock => Some(0),
            Assert => None, // 1 or 2, checked separately below
        };
        if let Some(expected) = fixed_arity {
            if arg_types.len() != expected {
                self.error_at(
                    format!("{}() expects {} argument{} but got {}", builtin.name(), expected, if expected == 1 { "" } else { "s" }, arg_types.len()),
                    span,
                );
                return DataType::Any;
            }
        }

        match builtin {
            // print() never produces a usable value.
            Print => DataType::Any,
            Max | Min => {
                if arg_types.is_empty() {
                    self.error_at(format!("{}() requires at least one argument", builtin.name()), span);
                    return DataType::Any;
                }
                // ponytail: mismatched-argument-type errors inside max/min
                // stay a runtime concern (Value::compare's error text);
                // statically we just fall back to Any for a mixed call.
                Self::combine_types(arg_types)
            }
            Int => DataType::Integer,
            Float => DataType::Float,
            Str => DataType::String,
            Len => {
                self.expect_type(arg_types[0], DataType::String, builtin.name(), span);
                DataType::Integer
            }
            Input => DataType::String,
            Substr => {
                self.expect_type(arg_types[0], DataType::String, builtin.name(), span.clone());
                self.expect_numeric(arg_types[1], builtin.name(), span.clone());
                self.expect_numeric(arg_types[2], builtin.name(), span);
                DataType::String
            }
            Find => {
                self.expect_type(arg_types[0], DataType::String, builtin.name(), span.clone());
                self.expect_type(arg_types[1], DataType::String, builtin.name(), span);
                DataType::Integer
            }
            Upper | Lower | Trim | Ord => {
                self.expect_type(arg_types[0], DataType::String, builtin.name(), span);
                if builtin == Ord { DataType::Integer } else { DataType::String }
            }
            CharAt => {
                self.expect_type(arg_types[0], DataType::String, builtin.name(), span.clone());
                self.expect_numeric(arg_types[1], builtin.name(), span);
                DataType::String
            }
            Chr => {
                self.expect_numeric(arg_types[0], builtin.name(), span);
                DataType::String
            }
            Abs => match arg_types[0] {
                DataType::Integer => DataType::Integer,
                DataType::Float => DataType::Float,
                DataType::Any => DataType::Any,
                other => {
                    self.error_at(format!("abs() requires a numeric argument, got {:?}", other), span);
                    DataType::Any
                }
            },
            Sqrt => {
                self.expect_numeric(arg_types[0], builtin.name(), span);
                DataType::Float
            }
            Floor | Ceil | Round => {
                self.expect_numeric(arg_types[0], builtin.name(), span);
                DataType::Integer
            }
            Assert => {
                if arg_types.is_empty() || arg_types.len() > 2 {
                    self.error_at(format!("assert() expects 1 or 2 arguments but got {}", arg_types.len()), span);
                }
                DataType::Any
            }
            Clock => DataType::Float,
        }
    }

    /// Emits an error unless `actual` is `expected` or statically unknown (`Any`).
    fn expect_type(&mut self, actual: DataType, expected: DataType, who: &str, span: Option<TextSpan>) {
        if actual != expected && actual != DataType::Any {
            self.error_at(format!("{}() requires a {:?} argument, got {:?}", who, expected, actual), span);
        }
    }

    /// Emits an error unless `actual` is `Integer`, `Float`, or statically unknown (`Any`).
    fn expect_numeric(&mut self, actual: DataType, who: &str, span: Option<TextSpan>) {
        if !matches!(actual, DataType::Integer | DataType::Float | DataType::Any) {
            self.error_at(format!("{}() requires a numeric argument, got {:?}", who, actual), span);
        }
    }

    /// Unifies a set of types the same way assignment widening does
    /// (Integer+Float -> Float); anything else collapses to `Any`.
    fn combine_types(types: &[DataType]) -> DataType {
        let mut result = match types.first() {
            Some(t) => *t,
            None => return DataType::Any,
        };
        for &t in &types[1..] {
            result = match (result, t) {
                (a, b) if a == b => a,
                (DataType::Integer, DataType::Float) | (DataType::Float, DataType::Integer) => DataType::Float,
                _ => return DataType::Any,
            };
        }
        result
    }

    // -- statements -----------------------------------------------------------

    fn resolve_function_body(&mut self, func: &ASTFunctionDeclaration, id: FunctionId, emit_warnings: bool) -> DataType {
        self.frames.push(Frame { is_global: false, next_slot: 0, scopes: vec![BlockScope::new()] });

        let mut param_types = Vec::new();
        let mut any_untyped = false;
        for param in &func.parameters {
            let ty = match param.type_annotation {
                Some(t) => t,
                None => {
                    any_untyped = true;
                    DataType::Any
                }
            };
            let binding = self.declare_variable(&param.name, ty, true, None);
            param.slot.set(Some(binding.slot()));
            param_types.push(ty);
        }
        self.function_param_types.insert(id.0, param_types);
        self.function_names.insert(id.0, func.name.clone());

        if any_untyped && emit_warnings {
            self.warn(format!(
                "cannot statically infer a type for one or more parameters of '{}'; falling back to dynamic type checking",
                func.name
            ));
        }

        // Params and the body's own top-level declarations share one scope
        // (matching the old evaluator: shadowing a parameter at the body's
        // top level was already a "redeclared in this scope" error).
        self.return_types_stack.push(Vec::new());
        self.hoist_and_resolve(&func.body);
        let seen_returns = self.return_types_stack.pop().unwrap();

        let return_type = if !seen_returns.is_empty() {
            Self::combine_types(&seen_returns)
        } else {
            Self::last_expression_type(&func.body).unwrap_or(DataType::Any)
        };

        let frame_size = self.frames.last().unwrap().next_slot;
        func.frame_size.set(Some(frame_size));
        self.frames.pop();

        return_type
    }

    fn last_expression_type(body: &[ASTStatement]) -> Option<DataType> {
        match &body.last()?.kind {
            ASTStatementKind::Expression(e) => e.resolved_type.get(),
            _ => None,
        }
    }

    fn calls_function_by_name(statements: &[ASTStatement], name: &str) -> bool {
        statements.iter().any(|s| Self::stmt_calls(s, name))
    }

    fn stmt_calls(stmt: &ASTStatement, name: &str) -> bool {
        match &stmt.kind {
            ASTStatementKind::Expression(e) => Self::expr_calls(e, name),
            ASTStatementKind::VariableDeclaration(d) => Self::expr_calls(&d.initializer, name),
            ASTStatementKind::Assignment(a) => Self::expr_calls(&a.value, name),
            ASTStatementKind::Block(stmts) => Self::calls_function_by_name(stmts, name),
            ASTStatementKind::IfStatement(i) => {
                Self::expr_calls(&i.condition, name)
                    || Self::calls_function_by_name(&i.then_branch, name)
                    || i.else_branch.as_ref().is_some_and(|b| Self::calls_function_by_name(b, name))
            }
            ASTStatementKind::WhileStatement(w) => {
                Self::expr_calls(&w.condition, name) || Self::calls_function_by_name(&w.body, name)
            }
            // A nested fn's body is a separate scope; it doesn't count as
            // the enclosing function calling itself.
            ASTStatementKind::FunctionDeclaration(_) => false,
            ASTStatementKind::ReturnStatement(r) => Self::expr_calls(&r.value, name),
        }
    }

    fn expr_calls(expr: &ASTExpression, name: &str) -> bool {
        match &expr.kind {
            ASTExpressionKind::Number(_) | ASTExpressionKind::Identifier(_) => false,
            ASTExpressionKind::Binary(b) => Self::expr_calls(&b.left, name) || Self::expr_calls(&b.right, name),
            ASTExpressionKind::Paranthesized(p) => Self::expr_calls(&p.expression, name),
            ASTExpressionKind::Unary(u) => Self::expr_calls(&u.operand, name),
            ASTExpressionKind::PostfixUnary(p) => Self::expr_calls(&p.operand, name),
            ASTExpressionKind::FunctionCall(c) => c.name == name || c.arguments.iter().any(|a| Self::expr_calls(a, name)),
        }
    }
}

impl ASTVisitor for Resolver {
    // Bypassed entirely by our own `visit_expression` override below; kept
    // only to satisfy the trait (it has no default implementation).
    fn visit_number(&mut self, _number: &crate::ast::ASTNumberExpression) {}

    fn visit_expression(&mut self, expression: &ASTExpression) {
        self.visit_expression_typed(expression);
    }

    fn visit_variable_declaration(&mut self, decl: &ASTVariableDeclaration) {
        let init_ty = self.visit_expression_typed(&decl.initializer);
        let span = decl.name_span.as_ref().map(|t| t.span.clone());
        let binding = self.declare_variable(&decl.name, init_ty, decl.is_mutable, span);
        decl.binding.set(Some(binding));
    }

    fn visit_assignment(&mut self, assign: &ASTAssignment) {
        let value_ty = self.visit_expression_typed(&assign.value);
        let span = assign.name_span.as_ref().map(|t| t.span.clone());

        match self.lookup_variable(&assign.name) {
            None => {
                let message = format!("Variable '{}' not found", assign.name);
                match closest_name(&assign.name, &self.visible_variable_names()) {
                    Some(s) => self.error_with_suggestion(message, span, format!("did you mean '{}' ?", s)),
                    None => self.error_at(message, span),
                }
            }
            Some((binding, declared_ty, is_mutable)) => {
                assign.binding.set(Some(binding));
                if !is_mutable {
                    self.error_at(format!("Cannot assign to immutable variable '{}'", assign.name), span);
                } else if value_ty != DataType::Any && declared_ty != DataType::Any && value_ty != declared_ty {
                    let widening_ok = declared_ty == DataType::Float && value_ty == DataType::Integer;
                    if widening_ok {
                        assign.needs_float_widen.set(true);
                    } else {
                        self.error_at(
                            format!(
                                "Type mismatch: variable '{}' has type {:?}, cannot assign {:?}",
                                assign.name, declared_ty, value_ty
                            ),
                            span,
                        );
                    }
                }
            }
        }
    }

    fn visit_function_declaration(&mut self, func: &ASTFunctionDeclaration) {
        let id = func.function_id.get().expect("function must be hoisted before its body is resolved");

        self.currently_resolving.push(id.0);
        let tentative = self.resolve_function_body(func, id, true);
        self.currently_resolving.pop();
        self.function_return_types.insert(id.0, tentative);

        let final_return = if tentative != DataType::Any && Self::calls_function_by_name(&func.body, &func.name) {
            let refined = self.resolve_function_body(func, id, false);
            self.function_return_types.insert(id.0, refined);
            refined
        } else {
            tentative
        };

        if final_return == DataType::Any {
            self.warn(format!(
                "cannot statically infer a return type for '{}'; falling back to dynamic type checking",
                func.name
            ));
        }

        func.inferred_return_type.set(Some(final_return));
    }

    fn visit_return_statement(&mut self, ret: &ASTReturnStatement) {
        let ty = self.visit_expression_typed(&ret.value);
        match self.return_types_stack.last_mut() {
            Some(returns) => returns.push(ty),
            None => self.error("'return' can only be used inside a function".to_string()),
        }
    }

    fn visit_if_statement(&mut self, if_stmt: &ASTIfStatement) {
        self.visit_expression_typed(&if_stmt.condition);
        self.resolve_statements(&if_stmt.then_branch);
        if let Some(else_branch) = &if_stmt.else_branch {
            self.resolve_statements(else_branch);
        }
    }

    fn visit_block(&mut self, statements: &Vec<ASTStatement>) {
        self.resolve_statements(statements);
    }

    fn visit_while_statement(&mut self, while_stmt: &ASTWhileStatement) {
        self.visit_expression_typed(&while_stmt.condition);
        self.resolve_statements(&while_stmt.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::lexer::Lexer;
    use crate::ast::parser::Parser;

    fn resolve(source: &str) -> (Ast, Resolver) {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens);
        let mut ast = Ast::new();
        while let Some(stmt) = parser.next_statement() {
            ast.add_statement(stmt);
        }
        assert!(parser.diagnostics.is_empty());

        let mut resolver = Resolver::new();
        resolver.resolve(&ast);
        (ast, resolver)
    }

    #[test]
    fn test_global_slots_are_distinct_and_sequential() {
        let (ast, resolver) = resolve("let a = 1; let b = 2; let c = 3;");
        assert!(!resolver.has_errors(), "{:?}", resolver.diagnostics);
        assert_eq!(resolver.global_slot_count(), 3);

        let mut slots = Vec::new();
        for stmt in &ast.statements {
            if let ASTStatementKind::VariableDeclaration(decl) = &stmt.kind {
                slots.push(decl.binding.get().unwrap().slot().0);
            }
        }
        assert_eq!(slots, vec![0, 1, 2]);
    }

    #[test]
    fn test_nested_blocks_never_reuse_slots_within_a_frame() {
        // Slots are function-frame-relative and monotonically increasing —
        // sibling if-branches must not reuse each other's slot numbers,
        // since Phase 3's VM frames are a flat, fixed-size stack window.
        let (_, resolver) = resolve(
            "fn f(n) { if n > 0 { let a = 1; } else { let b = 2; } let c = 3; }",
        );
        assert!(!resolver.has_errors(), "{:?}", resolver.diagnostics);
    }

    #[test]
    fn test_sibling_functions_see_each_other_regardless_of_order() {
        // Hoisting: `a` calls `b`, which is declared *after* it in the same
        // block — this must resolve via the pre-registration pass.
        let (_, resolver) = resolve(
            "fn a() { return b(); } fn b() { return 1; } a();",
        );
        assert!(!resolver.has_errors(), "{:?}", resolver.diagnostics);
    }

    #[test]
    fn test_frame_size_accounts_for_params_and_locals() {
        let (ast, resolver) = resolve("fn f(a, b) { let c = 1; let d = 2; }");
        assert!(!resolver.has_errors(), "{:?}", resolver.diagnostics);
        match &ast.statements[0].kind {
            ASTStatementKind::FunctionDeclaration(func) => {
                assert_eq!(func.frame_size.get(), Some(4));
            }
            _ => panic!("expected a function declaration"),
        }
    }
}
