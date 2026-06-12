//! # Symbol Table
//!
//! Manages variable storage across nested lexical scopes.
//!
//! ## Structure
//!
//! A [`SymbolTable`] owns a stack of [`Scope`]s. Variable lookup walks the
//! stack from innermost to outermost (standard lexical scoping). Entering a
//! block calls [`enter_scope`](SymbolTable::enter_scope); exiting calls
//! [`exit_scope`](SymbolTable::exit_scope).
//!
//! ## Mutability
//!
//! Variables declared with `let` are mutable; those declared with `const` are
//! not. Attempting to assign to an immutable variable returns an `Err`.
//!
//! ## Type checking on assignment
//!
//! The assigned value's type must match the variable's declared type.
//! The only implicit widening allowed is `Integer → Float`.

use crate::ast::types::{DataType, Value};
use std::collections::{HashMap, HashSet};

/// A single variable binding with its value, type, and mutability.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub value: Value,
    pub data_type: DataType,
    /// `true` for `let`, `false` for `const`.
    pub is_mutable: bool,
    pub is_initialized: bool,
}

impl Symbol {
    pub fn new(name: String, value: Value, data_type: DataType, is_mutable: bool) -> Self {
        Symbol {
            name,
            value,
            data_type,
            is_mutable,
            is_initialized: true,
        }
    }
}

/// A single scope level — a flat map of name → [`Symbol`].
#[derive(Debug, Clone)]
pub struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn new() -> Self {
        Scope {
            symbols: HashMap::new(),
        }
    }

    pub fn define(&mut self, name: String, symbol: Symbol) -> Result<(), String> {
        if self.symbols.contains_key(&name) {
            return Err(format!("Variable '{}' already declared in this scope", name));
        }
        self.symbols.insert(name, symbol);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        self.symbols.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

/// A lexically scoped symbol table.
///
/// Maintains a stack of [`Scope`]s; the first element is always the global scope.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
}

impl SymbolTable {
    /// Creates a symbol table with a single global scope.
    pub fn new() -> Self {
        SymbolTable {
            scopes: vec![Scope::new()], // Start with global scope
        }
    }

    /// Pushes a new inner scope onto the stack.
    pub fn enter_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pops the current scope.
    ///
    /// Returns `Err` if called while only the global scope remains.
    pub fn exit_scope(&mut self) -> Result<(), String> {
        if self.scopes.len() <= 1 {
            return Err("Cannot exit global scope".to_string());
        }
        self.scopes.pop();
        Ok(())
    }

    /// Returns the current nesting depth (1 = global only).
    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    /// Declares a new variable in the innermost scope.
    ///
    /// Returns `Err` if the name is already declared in the current scope.
    pub fn define(&mut self, name: String, value: Value, is_mutable: bool) -> Result<(), String> {
        let data_type = value.get_type();
        let symbol = Symbol::new(name.clone(), value, data_type, is_mutable);
        
        if let Some(current_scope) = self.scopes.last_mut() {
            current_scope.define(name, symbol)
        } else {
            Err("No active scope".to_string())
        }
    }

    /// Looks up a variable by name, searching from innermost to outermost scope.
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        // Search from innermost to outermost scope (lexical scoping)
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol); // Return first match found
            }
        }
        None
    }

    /// Assigns a new value to an existing variable.
    ///
    /// # Errors
    ///
    /// - `"Cannot assign to immutable variable '…'"` if declared with `const`.
    /// - `"Type mismatch: …"` if the new value's type differs from the declared
    ///   type and is not an `Integer → Float` widening.
    /// - `"Variable '…' not found"` if the name is undefined in any scope.
    pub fn assign(&mut self, name: &str, value: Value) -> Result<(), String> {
        // Search from innermost to outermost scope
        for scope in self.scopes.iter_mut().rev() {
            if let Some(symbol) = scope.get_mut(name) {
                // Enforce immutability for const variables
                if !symbol.is_mutable {
                    return Err(format!("Cannot assign to immutable variable '{}'", name));
                }
                
                // Type checking: ensure assigned value matches variable's declared type
                let new_type = value.get_type();
                if symbol.data_type != new_type {
                    // Special case: allow int to float widening conversion
                    if !(symbol.data_type == DataType::Float && new_type == DataType::Integer) {
                        return Err(format!(
                            "Type mismatch: variable '{}' has type {:?}, cannot assign {:?}",
                            name, symbol.data_type, new_type
                        ));
                    }
                    // Perform the coercion
                    if let Value::Integer(i) = value {
                        symbol.value = Value::Float(i as f64);
                        return Ok(());
                    }
                }
                
                symbol.value = value;
                return Ok(());
            }
        }

        Err(format!("Variable '{}' not found", name))
    }

    /// Returns `true` if the name is defined in any scope.
    pub fn exists(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Returns a clone of the variable's current value.
    ///
    /// Returns `Err` if the variable is not defined.
    pub fn get_value(&self, name: &str) -> Result<Value, String> {
        match self.lookup(name) {
            Some(symbol) => Ok(symbol.value.clone()),
            None => Err(format!("Variable '{}' not found", name)),
        }
    }

    /// Returns whether the named variable is mutable.
    ///
    /// Returns `Err` if the variable is not defined.
    pub fn is_mutable(&self, name: &str) -> Result<bool, String> {
        match self.lookup(name) {
            Some(symbol) => Ok(symbol.is_mutable),
            None => Err(format!("Variable '{}' not found", name)),
        }
    }

    /// Returns all visible variable names from innermost to outermost scope.
    pub fn all_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();

        for scope in self.scopes.iter().rev() {
            for name in scope.symbols.keys() {
                if seen.insert(name.clone()) {
                    names.push(name.clone());
                }
            }
        }

        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_define_and_lookup() {
        let mut table = SymbolTable::new();
        table.define("x".to_string(), Value::Integer(10), false).unwrap();
        
        assert!(table.exists("x"));
        assert_eq!(table.get_value("x").unwrap(), Value::Integer(10));
    }

    #[test]
    fn test_mutable_assignment() {
        let mut table = SymbolTable::new();
        table.define("x".to_string(), Value::Integer(10), true).unwrap();
        table.assign("x", Value::Integer(20)).unwrap();
        
        assert_eq!(table.get_value("x").unwrap(), Value::Integer(20));
    }

    #[test]
    fn test_immutable_assignment() {
        let mut table = SymbolTable::new();
        table.define("x".to_string(), Value::Integer(10), false).unwrap();
        
        let result = table.assign("x", Value::Integer(20));
        assert!(result.is_err());
    }

    #[test]
    fn test_nested_scopes() {
        let mut table = SymbolTable::new();
        table.define("x".to_string(), Value::Integer(10), false).unwrap();
        
        table.enter_scope();
        table.define("y".to_string(), Value::Integer(20), false).unwrap();
        
        assert!(table.exists("x"));
        assert!(table.exists("y"));
        
        table.exit_scope().unwrap();
        assert!(table.exists("x"));
        assert!(!table.exists("y"));
    }
}
