use crate::symbol::Symbol;
use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct Keyword {
    index: usize,
    symbol: Symbol,
}

impl Keyword {
    pub fn new(index: usize, symbol: Symbol) -> Self {
        Self { index, symbol }
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

impl Hash for Keyword {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state)
    }
}

impl PartialEq for Keyword {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for Keyword {}

impl PartialOrd for Keyword {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Keyword {
    fn cmp(&self, other: &Self) -> Ordering {
        self.index.cmp(&other.index)
    }
}

impl Display for Keyword {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, ":{}", self.symbol)
    }
}
