use crate::symbol::Symbol;
use crate::value::{Cell, Value};
use std::collections::HashMap;

#[derive(Clone)]
pub struct GlobalStore {
    globals: HashMap<Symbol, Cell>,
}

impl GlobalStore {
    pub fn new() -> GlobalStore {
        Self {
            globals: HashMap::new(),
        }
    }

    pub fn insert_cell(&mut self, key: Symbol, cell: Cell) {
        self.globals.insert(key, cell);
    }

    pub fn insert(&mut self, key: Symbol, value: Value) {
        match self.globals.get(&key) {
            Some(cell) => {
                let mut cell = cell.borrow_mut();
                *cell = value;
            }
            None => self.insert_cell(key, Cell::new(value)),
        }
    }

    pub fn get_mut(&mut self, key: &Symbol) -> Option<&mut Cell> {
        self.globals.get_mut(key)
    }
}
