use std::collections::hash_map::DefaultHasher;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Symbol {
    str: Rc<str>,
    hash: usize,
}

impl Debug for Symbol {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self.str)
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.str)
    }
}

impl Deref for Symbol {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.str.as_ref()
    }
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

fn prehash_string(str: &str) -> usize {
    let mut hasher = DefaultHasher::new();
    str.hash(&mut hasher);
    hasher.finish() as usize
}

impl Symbol {
    pub fn new(str: impl Into<Rc<str>>) -> Self {
        let str = str.into();
        let hash = prehash_string(str.as_ref());

        Self { str, hash }
    }
}

impl From<String> for Symbol {
    fn from(value: String) -> Self {
        Symbol::new(value)
    }
}

impl From<&str> for Symbol {
    fn from(value: &str) -> Self {
        Symbol::new(value)
    }
}
