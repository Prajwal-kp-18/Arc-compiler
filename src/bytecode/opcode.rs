//! # OpCode
//!
//! The bytecode instruction set. Deliberately small and generic — every
//! arithmetic/comparison op dispatches dynamically over the same tagged
//! [`Value`](crate::ast::types::Value) enum the tree-walking evaluator uses.
//! Type-specialized instructions are a Phase 5/LLVM concern, not this one.
//!
//! Operand widths (bytes immediately following the opcode byte in a
//! [`Chunk`](crate::bytecode::chunk::Chunk)'s code):
//!
//! | OpCode | Operands |
//! |---|---|
//! | `Constant` | `u16` constant pool index |
//! | `GetLocal` / `SetLocal` / `GetGlobal` / `SetGlobal` | `u16` slot index |
//! | `Jump` / `JumpIfFalse` | `u16` forward offset (added to ip after the operand) |
//! | `Call` | `u16` function id, `u8` argument count |
//! | `Print` / `Max` / `Min` | `u8` argument count |
//! | everything else | none |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// Push `constants[operand]`.
    Constant = 0,
    /// Push `Value::Boolean(true)` / `Value::Boolean(false)` directly, no constant pool entry.
    True = 1,
    False = 2,
    /// Discard the top of the stack (a statement's leftover expression value).
    Pop = 3,
    GetLocal = 4,
    /// Store into a local slot; leaves the stored value on the stack
    /// (assignment-as-expression convention, used by inc/dec desugaring).
    SetLocal = 5,
    GetGlobal = 6,
    SetGlobal = 7,
    /// Convert the top of the stack from `Integer` to `Float` (a no-op if
    /// already `Float`). Emitted only where the Resolver statically decided
    /// an assignment needs int->float widening — the *decision* is made at
    /// compile time; this opcode just performs the runtime conversion.
    ToFloat = 8,
    Add = 9,
    Subtract = 10,
    Multiply = 11,
    Divide = 12,
    Modulo = 13,
    Power = 14,
    BitAnd = 15,
    BitOr = 16,
    BitXor = 17,
    ShiftLeft = 18,
    ShiftRight = 19,
    Equal = 20,
    NotEqual = 21,
    Less = 22,
    Greater = 23,
    LessEqual = 24,
    GreaterEqual = 25,
    /// Arithmetic negation (`-x`). Unary `+` is a runtime no-op and compiles
    /// to nothing at all, so it has no opcode.
    Negate = 26,
    /// Logical NOT, coercing the operand to boolean first (`!x`).
    Not = 27,
    /// Unconditional relative forward jump.
    Jump = 28,
    /// Peeks (does not pop) the top of the stack; jumps forward if falsy.
    JumpIfFalse = 29,
    /// Unconditional relative *backward* jump (loop back-edge): `u16`
    /// distance subtracted from ip. Used only for `while`/`for` loops —
    /// every other jump is forward-only.
    Loop = 35,
    /// Calls a user-defined function: `u16` FunctionId, `u8` argument count.
    /// Arguments are already on the stack, pushed left-to-right.
    Call = 30,
    /// Ends the current function/script chunk; the return value is
    /// whatever's on top of the stack.
    Return = 31,
    /// Built-in `print(...)`: `u8` argument count. Pops the arguments and
    /// pushes an inert placeholder (see design doc §5) since print has no
    /// real return value.
    Print = 32,
    Max = 33,
    Min = 34,
}

impl OpCode {
    pub fn from_byte(byte: u8) -> Option<OpCode> {
        use OpCode::*;
        Some(match byte {
            0 => Constant,
            1 => True,
            2 => False,
            3 => Pop,
            4 => GetLocal,
            5 => SetLocal,
            6 => GetGlobal,
            7 => SetGlobal,
            8 => ToFloat,
            9 => Add,
            10 => Subtract,
            11 => Multiply,
            12 => Divide,
            13 => Modulo,
            14 => Power,
            15 => BitAnd,
            16 => BitOr,
            17 => BitXor,
            18 => ShiftLeft,
            19 => ShiftRight,
            20 => Equal,
            21 => NotEqual,
            22 => Less,
            23 => Greater,
            24 => LessEqual,
            25 => GreaterEqual,
            26 => Negate,
            27 => Not,
            28 => Jump,
            29 => JumpIfFalse,
            35 => Loop,
            30 => Call,
            31 => Return,
            32 => Print,
            33 => Max,
            34 => Min,
            _ => return None,
        })
    }

    /// Human-readable mnemonic, used by the disassembler.
    pub fn mnemonic(self) -> &'static str {
        use OpCode::*;
        match self {
            Constant => "OP_CONSTANT",
            True => "OP_TRUE",
            False => "OP_FALSE",
            Pop => "OP_POP",
            GetLocal => "OP_GET_LOCAL",
            SetLocal => "OP_SET_LOCAL",
            GetGlobal => "OP_GET_GLOBAL",
            SetGlobal => "OP_SET_GLOBAL",
            ToFloat => "OP_TO_FLOAT",
            Add => "OP_ADD",
            Subtract => "OP_SUBTRACT",
            Multiply => "OP_MULTIPLY",
            Divide => "OP_DIVIDE",
            Modulo => "OP_MODULO",
            Power => "OP_POWER",
            BitAnd => "OP_BIT_AND",
            BitOr => "OP_BIT_OR",
            BitXor => "OP_BIT_XOR",
            ShiftLeft => "OP_SHIFT_LEFT",
            ShiftRight => "OP_SHIFT_RIGHT",
            Equal => "OP_EQUAL",
            NotEqual => "OP_NOT_EQUAL",
            Less => "OP_LESS",
            Greater => "OP_GREATER",
            LessEqual => "OP_LESS_EQUAL",
            GreaterEqual => "OP_GREATER_EQUAL",
            Negate => "OP_NEGATE",
            Not => "OP_NOT",
            Jump => "OP_JUMP",
            JumpIfFalse => "OP_JUMP_IF_FALSE",
            Loop => "OP_LOOP",
            Call => "OP_CALL",
            Return => "OP_RETURN",
            Print => "OP_PRINT",
            Max => "OP_MAX",
            Min => "OP_MIN",
        }
    }
}
