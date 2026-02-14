//! Symbol table - manages variables and scopes

use crate::ast::types::{DataType, Value};
use std::collections::HashMap;

/// Variable storage with type and mutability info
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub value: Value,
    pub data_type: DataType,
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

/// Single scope level containing variables
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

/// Manages nested scopes for variable lookup and assignment
#[derive(Debug)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            scopes: vec![Scope::new()], // Start with global scope
        }
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Exit current scope
    pub fn exit_scope(&mut self) -> Result<(), String> {
        if self.scopes.len() <= 1 {
            return Err("Cannot exit global scope".to_string());
        }
        self.scopes.pop();
        Ok(())
    }

    /// Get current scope depth
    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    /// Define a new variable in the current scope
    pub fn define(&mut self, name: String, value: Value, is_mutable: bool) -> Result<(), String> {
        let data_type = value.get_type();
        let symbol = Symbol::new(name.clone(), value, data_type, is_mutable);
        
        if let Some(current_scope) = self.scopes.last_mut() {
            current_scope.define(name, symbol)
        } else {
            Err("No active scope".to_string())
        }
    }

    /// Look up a variable by name (searches from current scope up to global)
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        // Search from innermost to outermost scope (lexical scoping)
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol); // Return first match found
            }
        }
        None // Variable not found in any scope
    }

    /// Assign a value to an existing variable
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
                            "Type mismatch: variable '{}' has type {:?}, cannot assign value of type {:?}",
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

    /// Check if a variable exists in any scope
    pub fn exists(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Get the value of a variable
    pub fn get_value(&self, name: &str) -> Result<Value, String> {
        match self.lookup(name) {
            Some(symbol) => Ok(symbol.value.clone()),
            None => Err(format!("Variable '{}' not found", name)),
        }
    }

    /// Check if a variable is mutable
    pub fn is_mutable(&self, name: &str) -> Result<bool, String> {
        match self.lookup(name) {
            Some(symbol) => Ok(symbol.is_mutable),
            None => Err(format!("Variable '{}' not found", name)),
        }
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
