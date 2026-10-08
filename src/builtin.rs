use crate::program::{Program, RuntimeError, ValueStack};
use crate::value::{Value, ValueKey};
use num::{BigInt, ToPrimitive};
use std::collections::HashMap;
use std::iter::Peekable;
use std::rc::Rc;
use std::vec;

// XXX TODO this entire logic could be replaced with an .enumerate()
struct Args<'a> {
    iter: vec::Drain<'a, Value>,
    index: usize,
}

impl<'a> Iterator for Args<'a> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        let res = self.iter.next()?;
        self.index += 1;
        Some(res)
    }
}

impl<'a> Args<'a> {
    fn new(iter: vec::Drain<'a, Value>) -> Self {
        Self { iter, index: 0 }
    }

    fn next_required(&mut self) -> Result<Value, RuntimeError> {
        self.next().ok_or(RuntimeError::StackUnderflow) // XXX TODO this should be a different error
    }

    // XXX TODO: get rid of hardcoded strings here for v1 release
    // use something like ValueType instead
    fn next_int_required(&mut self) -> Result<Rc<BigInt>, RuntimeError> {
        match self.next_required()? {
            Value::Int(x) => Ok(x.clone()),
            _ => Err(RuntimeError::MismatchedArgType("int", self.index)),
        }
    }

    // XXX TODO: get rid of hardcoded strings here for v1 release
    // use something like ValueType instead
    fn next_table_required(&mut self) -> Result<Rc<HashMap<ValueKey, Value>>, RuntimeError> {
        match self.next_required()? {
            Value::Table(x) => Ok(x),
            _ => Err(RuntimeError::MismatchedArgType("table", self.index)),
        }
    }

    // XXX TODO: get rid of hardcoded strings here for v1 release
    // use something like ValueType instead
    fn next_string_required(&mut self) -> Result<Rc<str>, RuntimeError> {
        match self.next_required()? {
            Value::String(x) => Ok(x.clone()),
            _ => Err(RuntimeError::MismatchedArgType("string", self.index)),
        }
    }
}

fn read_args(stack: &mut ValueStack, nargs: usize) -> Result<Args<'_>, RuntimeError> {
    Ok(Args::new(
        stack.pop_many(nargs).ok_or(RuntimeError::StackUnderflow)?,
    ))
}

fn read_fixed_args(
    stack: &mut ValueStack,
    nargs: usize,
    wanted: usize,
) -> Result<Args<'_>, RuntimeError> {
    if nargs < wanted {
        Err(RuntimeError::TooFewArguments)
    } else if nargs > wanted {
        Err(RuntimeError::TooManyArguments)
    } else {
        read_args(stack, nargs)
    }
}

fn print_args(mut iter: Peekable<Args<'_>>) {
    while let Some(arg) = iter.next() {
        match arg {
            Value::String(s) => print!("{s}"),
            _ => print!("{arg}"),
        }

        if iter.peek().is_some() {
            print!(" ");
        }
    }
}

// XXX no alloc failure handling?
pub fn println(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let iter = read_args(stack, nargs)?.peekable();
    print_args(iter);
    println!();
    Ok(Value::Nil)
}

// XXX no alloc failure handling?
pub fn print(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let iter = read_args(stack, nargs)?.peekable();
    print_args(iter);
    Ok(Value::Nil)
}

pub fn tget(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let mut iter = read_fixed_args(stack, nargs, 2)?;
    let key = iter.next_required()?;
    let table = iter.next_table_required()?;
    let key: ValueKey = key.try_into().map_err(|_| RuntimeError::InvalidKey)?; // XXX TODO cleanup
    let value = table.get(&key).unwrap_or(&Value::Nil);
    Ok(value.clone())
}

// XXX no alloc failure handling?
pub fn str(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let mut iter = read_args(stack, nargs)?.peekable();
    let mut result = String::new();

    while let Some(item) = iter.next() {
        result += &item.to_string();

        if iter.peek().is_some() {
            result.push(' ');
        }
    }

    Ok(Value::new(result))
}

// XXX no alloc failure handling
pub fn repr(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let mut iter = read_fixed_args(stack, nargs, 1)?;
    let obj = iter.next().unwrap();
    Ok(Value::new(format!("{obj:?}")))
}

// XXX fighting usize conversion and trying to do safe memory allocation in Rust is a NIGHTMARE
pub fn str_repeat(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let mut args = read_fixed_args(stack, nargs, 2)?;
    let str = args.next_string_required()?;
    let count = args
        .next_int_required()?
        .to_usize()
        .ok_or(RuntimeError::IntTooLarge)?;

    let capacity = str
        .len()
        .checked_mul(count)
        .ok_or(RuntimeError::IntTooLarge)?;

    let mut result = String::new();
    result.try_reserve_exact(capacity)?;

    for _ in 0..count {
        result.push_str(&str);
    }

    Ok(Value::new(result))
}

// XXX fighting usize conversion and trying to do safe memory allocation in Rust is a NIGHTMARE
pub fn str_append(stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
    let mut args = read_fixed_args(stack, nargs, 2)?;
    let a = args.next_string_required()?;
    let b = args.next_string_required()?;

    let capacity = a
        .len()
        .checked_add(b.len())
        .ok_or(RuntimeError::IntTooLarge)?;

    let mut result = String::new();
    result.try_reserve_exact(capacity)?;
    result.push_str(&a);
    result.push_str(&b);
    Ok(Value::new(result))
}

pub fn load_standard_builtins(program: &mut Program) {
    program.add_global_builtin_fn("print", print);
    program.add_global_builtin_fn("println", println);
    program.add_global_builtin_fn("repr", repr);
    program.add_global_builtin_fn("str-repeat", str_repeat);
    program.add_global_builtin_fn("str-append", str_append);
    program.add_global_builtin_fn("tget", tget);
    program.add_global_builtin_fn("str", str);
}
