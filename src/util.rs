use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::hash::Hash;

pub fn fmt_simple_sequence<T: Display>(
    f: &mut Formatter,
    iter: impl Iterator<Item = T>,
    separator: &'static str,
) -> fmt::Result {
    let mut peekable = iter.peekable();

    while let Some(sub) = peekable.next() {
        sub.fmt(f)?;

        if peekable.peek().is_some() {
            write!(f, "{separator}")?;
        }
    }

    Ok(())
}

#[derive(Clone)]
pub struct InternBuilder<T: Hash + Eq + Clone> {
    values: Vec<T>,
    indices: HashMap<T, usize>,
}

impl<T: Hash + Eq + Clone> InternBuilder<T> {
    pub fn new() -> Self {
        Self {
            values: vec![],
            indices: HashMap::new(),
        }
    }

    // XXX TODO ugly as hell clone
    pub fn intern(&mut self, value: T) -> usize {
        let existing_idx = self.indices.get(&value);

        match existing_idx {
            Some(&index) => index,
            None => {
                let new_idx = self.values.len();
                self.values.push(value.clone());
                self.indices.insert(value, new_idx);
                new_idx
            }
        }
    }

    pub fn finish(self) -> Box<[T]> {
        self.values.into()
    }
}
