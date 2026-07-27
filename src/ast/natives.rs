//! # Native (built-in) function runtime
//!
//! [`call_builtin`] is the single source of truth for what every built-in
//! does at runtime — mirrors [`apply_binary`](crate::ast::types::apply_binary)'s
//! role for operators, shared by the tree-walking evaluator and the
//! bytecode VM so the two backends cannot drift on builtin semantics or
//! error messages.
//!
//! Arity is already checked by the Resolver for every program that reaches
//! here, so these implementations index arguments directly rather than
//! re-validating argument counts (`max`/`min` are the one exception: their
//! "at least one argument" rule is re-checked here too, matching how the
//! rest of the runtime stays defensive about variable-arity calls).

use crate::ast::resolver::BuiltinFn;
use crate::ast::types::Value;
use std::cmp::Ordering;

pub fn call_builtin(builtin: BuiltinFn, args: &[Value]) -> Result<Value, String> {
    use BuiltinFn::*;
    match builtin {
        Print => {
            let line = args.iter().map(Value::to_string).collect::<Vec<_>>().join(" ");
            println!("{}", line);
            Ok(Value::Unit)
        }
        Max => reduce_extreme("max", args, Ordering::Less),
        Min => reduce_extreme("min", args, Ordering::Greater),
        Int => to_int(&args[0]),
        Float => to_float(&args[0]),
        Str => Ok(Value::String(args[0].to_string())),
        Len => match &args[0] {
            Value::String(s) => Ok(Value::Integer(s.chars().count() as i64)),
            other => Err(format!("len() requires a String argument, got {:?}", other.get_type())),
        },
        Input => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| format!("input() failed: {}", e))?;
            Ok(Value::String(line.trim_end_matches(['\n', '\r']).to_string()))
        }
        Substr => native_substr(args),
        Find => native_find(args),
        Upper => as_string(&args[0], "upper").map(|s| Value::String(s.to_uppercase())),
        Lower => as_string(&args[0], "lower").map(|s| Value::String(s.to_lowercase())),
        Trim => as_string(&args[0], "trim").map(|s| Value::String(s.trim().to_string())),
        CharAt => native_char_at(args),
        Ord => native_ord(&args[0]),
        Chr => native_chr(&args[0]),
        Abs => native_abs(&args[0]),
        Sqrt => native_sqrt(&args[0]),
        Floor => native_round(&args[0], f64::floor, "floor"),
        Ceil => native_round(&args[0], f64::ceil, "ceil"),
        Round => native_round(&args[0], f64::round, "round"),
        Assert => native_assert(args),
        Clock => native_clock(),
    }
}

/// Shared reduction for `max`/`min`: keeps the current value unless the next
/// one compares as `keep_current` against it (`Less` for max, `Greater` for min).
fn reduce_extreme(name: &str, args: &[Value], keep_current: Ordering) -> Result<Value, String> {
    let Some((first, rest)) = args.split_first() else {
        return Err(format!("{}() requires at least one argument", name));
    };
    let mut current = first.clone();
    for v in rest {
        match current.compare(v) {
            Ok(ordering) if ordering == keep_current => current = v.clone(),
            Ok(_) => (),
            Err(e) => return Err(format!("{}() comparison error: {}", name, e)),
        }
    }
    Ok(current)
}

fn to_int(v: &Value) -> Result<Value, String> {
    match v {
        Value::Integer(i) => Ok(Value::Integer(*i)),
        Value::Float(f) => Ok(Value::Integer(*f as i64)),
        Value::Boolean(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
        Value::String(s) => s.trim().parse::<i64>().map(Value::Integer).map_err(|_| format!("Cannot convert '{}' to Int", s)),
        Value::Unit => Err("Cannot convert Unit to Int".to_string()),
    }
}

fn to_float(v: &Value) -> Result<Value, String> {
    match v {
        Value::Integer(i) => Ok(Value::Float(*i as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Boolean(b) => Ok(Value::Float(if *b { 1.0 } else { 0.0 })),
        Value::String(s) => s.trim().parse::<f64>().map(Value::Float).map_err(|_| format!("Cannot convert '{}' to Float", s)),
        Value::Unit => Err("Cannot convert Unit to Float".to_string()),
    }
}

fn as_string(v: &Value, who: &str) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        other => Err(format!("{}() requires a String argument, got {:?}", who, other.get_type())),
    }
}

/// Lenient integer coercion for numeric-ish arguments (indices, counts):
/// accepts Integer/Float/Boolean, same as `Value::to_integer`.
fn as_int(v: &Value, who: &str) -> Result<i64, String> {
    v.to_integer().map_err(|_| format!("{}() requires an Integer argument", who))
}

fn as_f64(v: &Value, who: &str) -> Result<f64, String> {
    match v {
        Value::Integer(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(format!("{}() requires a numeric argument, got {:?}", who, other.get_type())),
    }
}

fn native_substr(args: &[Value]) -> Result<Value, String> {
    let s = as_string(&args[0], "substr")?;
    let start = as_int(&args[1], "substr")?;
    let len = as_int(&args[2], "substr")?;
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len() as i64;
    let start_idx = start.clamp(0, n) as usize;
    let take = len.max(0) as usize;
    Ok(Value::String(chars[start_idx..].iter().take(take).collect()))
}

fn native_find(args: &[Value]) -> Result<Value, String> {
    let s = as_string(&args[0], "find")?;
    let sub = as_string(&args[1], "find")?;
    match s.find(&sub) {
        Some(byte_idx) => Ok(Value::Integer(s[..byte_idx].chars().count() as i64)),
        None => Ok(Value::Integer(-1)),
    }
}

fn native_char_at(args: &[Value]) -> Result<Value, String> {
    let s = as_string(&args[0], "char_at")?;
    let idx = as_int(&args[1], "char_at")?;
    if idx < 0 {
        return Err("char_at() index out of range".to_string());
    }
    s.chars().nth(idx as usize).map(|c| Value::String(c.to_string())).ok_or_else(|| "char_at() index out of range".to_string())
}

fn native_ord(v: &Value) -> Result<Value, String> {
    let s = as_string(v, "ord")?;
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(Value::Integer(c as i64)),
        _ => Err("ord() requires a single-character string".to_string()),
    }
}

fn native_chr(v: &Value) -> Result<Value, String> {
    let i = as_int(v, "chr")?;
    let cp = u32::try_from(i).map_err(|_| format!("chr() invalid code point: {}", i))?;
    char::from_u32(cp).map(|c| Value::String(c.to_string())).ok_or_else(|| format!("chr() invalid code point: {}", i))
}

fn native_abs(v: &Value) -> Result<Value, String> {
    match v {
        Value::Integer(i) => Ok(Value::Integer(i.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(format!("abs() requires a numeric argument, got {:?}", other.get_type())),
    }
}

fn native_sqrt(v: &Value) -> Result<Value, String> {
    let f = as_f64(v, "sqrt")?;
    if f < 0.0 {
        return Err("Cannot take sqrt of a negative number".to_string());
    }
    Ok(Value::Float(f.sqrt()))
}

fn native_round(v: &Value, round_fn: fn(f64) -> f64, who: &str) -> Result<Value, String> {
    match v {
        Value::Integer(i) => Ok(Value::Integer(*i)),
        Value::Float(f) => Ok(Value::Integer(round_fn(*f) as i64)),
        other => Err(format!("{}() requires a numeric argument, got {:?}", who, other.get_type())),
    }
}

fn native_assert(args: &[Value]) -> Result<Value, String> {
    if args[0].to_boolean() {
        return Ok(Value::Unit);
    }
    match args.get(1) {
        Some(msg) => Err(msg.to_string()),
        None => Err("assertion failed".to_string()),
    }
}

fn native_clock() -> Result<Value, String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    Ok(Value::Float(now.as_secs_f64()))
}
