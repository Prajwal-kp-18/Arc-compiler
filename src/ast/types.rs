//! # Type System
//!
//! Defines the runtime [`Value`] enum and the static [`DataType`] enum used
//! by the symbol table, as well as the coercion and comparison logic that
//! operates on values.
//!
//! ## Coercion rules
//!
//! | Left | Right | Result |
//! |------|-------|--------|
//! | Integer | Integer | Integer |
//! | Float | Float | Float |
//! | Integer | Float | Float (integer widened) |
//! | String | any | String (other side converted via `Display`) |
//! | Boolean | Boolean | Boolean |
//!
//! ## Truthiness
//!
//! | Value | Truthy? |
//! |-------|---------|
//! | `false` / `0` / `0.0` / `""` | no |
//! | everything else | yes |

use std::fmt;

/// The static type of a variable or expression, as computed by the
/// [`Resolver`](crate::ast::resolver::Resolver).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Integer,
    Float,
    Boolean,
    String,
    /// Deliberately dynamic: the Resolver could not statically pin down a
    /// concrete type (e.g. an untyped parameter with no usage-derived type),
    /// so this value is checked at runtime instead, same as before Phase 1.
    Any,
}

/// A runtime value produced by the evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 64-bit signed integer.
    Integer(i64),
    /// IEEE 754 double-precision float.
    Float(f64),
    Boolean(bool),
    String(String),
}

impl Value {
    /// Returns the [`DataType`] corresponding to this value's variant.
    pub fn get_type(&self) -> DataType {
        match self {
            Value::Integer(_) => DataType::Integer,
            Value::Float(_) => DataType::Float,
            Value::Boolean(_) => DataType::Boolean,
            Value::String(_) => DataType::String,
        }
    }

    /// Coerces `left` and `right` to a common type for binary operations.
    ///
    /// Returns `Err` if no implicit conversion exists between the two types.
    pub fn coerce_to_common_type(left: &Value, right: &Value) -> Result<(Value, Value), String> {
        match (left, right) {
            // Same types - no coercion needed
            (Value::Integer(l), Value::Integer(r)) => Ok((Value::Integer(*l), Value::Integer(*r))),
            (Value::Float(l), Value::Float(r)) => Ok((Value::Float(*l), Value::Float(*r))),
            (Value::Boolean(l), Value::Boolean(r)) => Ok((Value::Boolean(*l), Value::Boolean(*r))),
            (Value::String(l), Value::String(r)) => Ok((Value::String(l.clone()), Value::String(r.clone()))),
            // Widen integer to float
            (Value::Integer(i), Value::Float(f)) => Ok((Value::Float(*i as f64), Value::Float(*f))),
            (Value::Float(f), Value::Integer(i)) => Ok((Value::Float(*f), Value::Float(*i as f64))),
            // String absorbs the other side via Display
            (Value::String(s), other) => Ok((Value::String(s.clone()), Value::String(other.to_string()))),
            (other, Value::String(s)) => Ok((Value::String(other.to_string()), Value::String(s.clone()))),
            _ => Err(format!("Cannot coerce {:?} and {:?} to a common type", left.get_type(), right.get_type())),
        }
    }

    /// Returns the boolean interpretation of a value.
    ///
    /// `false`, `0`, `0.0`, and `""` are falsy; everything else is truthy.
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
        }
    }

    /// Converts a value to `i64` for use in bitwise operations.
    ///
    /// Returns `Err` for `String` values, which cannot be meaningfully
    /// interpreted as integers.
    pub fn to_integer(&self) -> Result<i64, String> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::String(_) => Err("Cannot convert string to integer for bitwise operations".to_string()),
        }
    }

    /// Tests two values for equality, allowing `Integer`↔`Float` comparison.
    ///
    /// Float equality uses an epsilon of [`f64::EPSILON`].
    pub fn equals(&self, other: &Value) -> Result<bool, String> {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => Ok(a == b),
            (Value::Float(a), Value::Float(b)) => Ok((a - b).abs() < f64::EPSILON),
            (Value::Boolean(a), Value::Boolean(b)) => Ok(a == b),
            (Value::String(a), Value::String(b)) => Ok(a == b),
            (Value::Integer(i), Value::Float(f)) | (Value::Float(f), Value::Integer(i)) => {
                Ok((*i as f64 - f).abs() < f64::EPSILON)
            },
            _ => Err(format!("Cannot compare {:?} and {:?} for equality", self.get_type(), other.get_type())),
        }
    }

    /// Orders two values, allowing `Integer`↔`Float` comparison.
    ///
    /// Strings are ordered lexicographically.
    pub fn compare(&self, other: &Value) -> Result<std::cmp::Ordering, String> {
        use std::cmp::Ordering;

        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
            (Value::Float(a), Value::Float(b)) => {
                if a < b { Ok(Ordering::Less) }
                else if a > b { Ok(Ordering::Greater) }
                else { Ok(Ordering::Equal) }
            },
            (Value::Boolean(a), Value::Boolean(b)) => Ok(a.cmp(b)),
            (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
            // Allow comparison between int and float
            (Value::Integer(i), Value::Float(f)) => {
                let i_float = *i as f64;
                if i_float < *f { Ok(Ordering::Less) }
                else if i_float > *f { Ok(Ordering::Greater) }
                else { Ok(Ordering::Equal) }
            },
            (Value::Float(f), Value::Integer(i)) => {
                let i_float = *i as f64;
                if f < &i_float { Ok(Ordering::Less) }
                else if f > &i_float { Ok(Ordering::Greater) }
                else { Ok(Ordering::Equal) }
            },
            _ => Err(format!("Cannot compare {:?} and {:?}", self.get_type(), other.get_type())),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Integer => write!(f, "Integer"),
            DataType::Float => write!(f, "Float"),
            DataType::Boolean => write!(f, "Boolean"),
            DataType::String => write!(f, "String"),
            DataType::Any => write!(f, "Any"),
        }
    }
}
