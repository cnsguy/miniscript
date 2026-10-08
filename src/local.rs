use crate::symbol::Symbol;
use std::collections::HashMap;

#[derive(Clone)]
pub struct LocalMap {
    scopes: HashMap<Symbol, usize>,
}

#[derive(Clone)]
pub struct LocalTracker {
    locals: Vec<LocalMap>,
    num_locals: usize,
}

impl LocalMap {
    fn new() -> Self {
        Self {
            scopes: HashMap::new(),
        }
    }

    pub fn get(&self, sym: &Symbol) -> Option<usize> {
        self.scopes.get(sym).copied()
    }

    pub fn insert(&mut self, sym: Symbol, index: usize) {
        self.scopes.insert(sym, index);
    }
}

impl LocalTracker {
    pub fn new() -> Self {
        Self {
            locals: vec![LocalMap::new()],
            num_locals: 0,
        }
    }

    pub fn top_mut(&mut self) -> &mut LocalMap {
        self.locals.last_mut().unwrap()
    }

    pub fn get(&self, sym: &Symbol) -> Option<usize> {
        for scope in self.locals.iter().rev() {
            if let Some(index) = scope.get(sym) {
                return Some(index);
            }
        }

        None
    }

    pub fn insert(&mut self, sym: Symbol) -> usize {
        let index = self.num_locals;
        self.num_locals += 1;
        self.top_mut().insert(sym, index);
        index
    }

    pub fn num_locals(&self) -> usize {
        self.num_locals
    }

    pub fn push_scope(&mut self) {
        self.locals.push(LocalMap::new())
    }

    pub fn pop_scope(&mut self) -> Option<LocalMap> {
        self.locals.pop()
    }
}
