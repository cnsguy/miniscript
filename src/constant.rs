use crate::symbol::Symbol;

use num::BigInt;
use std::rc::Rc;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub enum Constant {
    Int(Rc<BigInt>),
    String(Rc<str>),
    Symbol(Symbol),
}

impl Constant {
    pub fn new<I: Into<Constant>>(value: I) -> Self {
        value.into()
    }
}

impl From<BigInt> for Constant {
    fn from(value: BigInt) -> Self {
        Self::Int(Rc::new(value))
    }
}

impl From<&str> for Constant {
    fn from(value: &str) -> Self {
        Self::String(Rc::from(value))
    }
}

impl From<Symbol> for Constant {
    fn from(value: Symbol) -> Self {
        Self::Symbol(value)
    }
}
