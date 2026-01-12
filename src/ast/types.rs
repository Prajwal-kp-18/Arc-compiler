use std::fmt;

/// Represents the different data types supported by the compiler
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Float,
    Boolean,
    String,
    Unknown,
}

/// Represents a value with its type
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(String),
}

impl Value {
    pub fn get_type(&self) -> DataType {
        match self {
            Value::Integer(_) => DataType::Integer,
            Value::Float(_) => DataType::Float,
            Value::Boolean(_) => DataType::Boolean,
            Value::String(_) => DataType::String,
        }
    }

    /// Try to coerce values to a common type for binary operations
    pub fn coerce_to_common_type(left: &Value, right: &Value) -> Result<(Value, Value), String> {
        match (left, right) {
            // Same types - no coercion needed
            (Value::Integer(l), Value::Integer(r)) => Ok((Value::Integer(*l), Value::Integer(*r))),
            (Value::Float(l), Value::Float(r)) => Ok((Value::Float(*l), Value::Float(*r))),
            (Value::Boolean(l), Value::Boolean(r)) => Ok((Value::Boolean(*l), Value::Boolean(*r))),
            (Value::String(l), Value::String(r)) => Ok((Value::String(l.clone()), Value::String(r.clone()))),
            
            // Integer to Float coercion
            (Value::Integer(i), Value::Float(f)) => Ok((Value::Float(*i as f64), Value::Float(*f))),
            (Value::Float(f), Value::Integer(i)) => Ok((Value::Float(*f), Value::Float(*i as f64))),
            
            // String concatenation with any type
            (Value::String(s), other) => Ok((Value::String(s.clone()), Value::String(other.to_string()))),
            (other, Value::String(s)) => Ok((Value::String(other.to_string()), Value::String(s.clone()))),
            
            _ => Err(format!("Cannot coerce {:?} and {:?} to a common type", left.get_type(), right.get_type())),
        }
    }

    /// Convert value to boolean for logical operations
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
        }
    }

    /// Convert value to integer (for bitwise operations)
    pub fn to_integer(&self) -> Result<i64, String> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::String(_) => Err("Cannot convert string to integer for bitwise operations".to_string()),
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
            DataType::Unknown => write!(f, "Unknown"),
        }
    }
}
