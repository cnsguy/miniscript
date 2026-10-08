use crate::constant::Constant;
use crate::keyword::Keyword;
use crate::program::{RuntimeError, ValueStack};
use crate::symbol::Symbol;
use crate::util::fmt_simple_sequence;
use num::bigint::BigInt;
use num::{Signed, Zero};
use num_traits::ToPrimitive;
use ordered_float::OrderedFloat;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Deref, Div, Mul, Neg, Sub};
use std::ptr::fn_addr_eq;
use std::rc::Rc;

// fn pointer pointing to a Rust-side function
pub type BuiltinFnPtr = fn(&mut ValueStack, usize) -> Result<Value, RuntimeError>;

#[derive(Debug, Clone)]
pub struct BuiltinFunction {
    name: &'static str,
    func: BuiltinFnPtr,
}

impl PartialEq for BuiltinFunction {
    fn eq(&self, other: &Self) -> bool {
        fn_addr_eq(self.func, other.func)
    }
}

impl Eq for BuiltinFunction {}

impl PartialOrd for BuiltinFunction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BuiltinFunction {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.func as usize).cmp(&(other.func as usize))
    }
}

impl BuiltinFunction {
    pub fn new(name: &'static str, func: BuiltinFnPtr) -> Self {
        Self { name, func }
    }

    pub fn call(&self, stack: &mut ValueStack, nargs: usize) -> Result<Value, RuntimeError> {
        (self.func)(stack, nargs)
    }
}

#[derive(Debug, Clone)]
pub struct Closure {
    proto_index: usize,
    upvalues: Rc<[Cell]>,
}

impl PartialEq for Closure {
    fn eq(&self, other: &Self) -> bool {
        self.proto_index == other.proto_index && Rc::ptr_eq(&self.upvalues, &other.upvalues)
    }
}

impl Eq for Closure {}

impl PartialOrd for Closure {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Closure {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.proto_index, Rc::as_ptr(&self.upvalues) as *const ())
            .cmp(&(other.proto_index, Rc::as_ptr(&other.upvalues) as *const ()))
    }
}

impl Closure {
    pub fn new(proto_index: usize, upvalues: Rc<[Cell]>) -> Self {
        Self {
            proto_index,
            upvalues,
        }
    }

    pub fn proto_index(&self) -> usize {
        self.proto_index
    }

    pub fn upvalues(&self) -> Rc<[Cell]> {
        self.upvalues.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Cell(Rc<RefCell<Value>>);

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Cell {}

impl PartialOrd for Cell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cell {
    fn cmp(&self, other: &Self) -> Ordering {
        Rc::as_ptr(&self.0).cmp(&Rc::as_ptr(&other.0))
    }
}

impl Deref for Cell {
    type Target = RefCell<Value>;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl Cell {
    pub fn new(value: Value) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }

    pub fn set(&self, value: Value) {
        let mut cell = self.borrow_mut();
        *cell = value;
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(Rc<BigInt>),
    Float(OrderedFloat<f64>),
    String(Rc<str>),
    BuiltinFunction(BuiltinFunction), // TODO merge with closures
    Closure(Closure),
    Vector(Rc<[Self]>),
    Table(Rc<HashMap<ValueKey, Self>>),
    List(Rc<[Self]>), // XXX TODO use a proper list
    Symbol(Symbol),
    Keyword(Keyword), // XXX TODO add name info for debugging
}

// XXX NOTE: while (== 1 1.0) is true, they are NOT hashed to the same slot in a dict
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueKey {
    Nil,
    Bool(bool),
    Int(Rc<BigInt>),
    Float(OrderedFloat<f64>),
    String(Rc<str>),
    Symbol(Symbol),
    Keyword(Keyword), // XXX TODO add name info for debugging
}

impl From<ValueKey> for Value {
    fn from(value: ValueKey) -> Self {
        match value {
            ValueKey::Nil => Self::Nil,
            ValueKey::Bool(x) => Self::Bool(x),
            ValueKey::Int(x) => Self::Int(x),
            ValueKey::Float(x) => Self::Float(x),
            ValueKey::String(x) => Self::String(x),
            ValueKey::Symbol(x) => Self::Symbol(x),
            ValueKey::Keyword(x) => Self::Keyword(x),
        }
    }
}

impl TryFrom<Value> for ValueKey {
    type Error = ();

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(Self::Nil),
            Value::Bool(x) => Ok(Self::Bool(x)),
            Value::Int(x) => Ok(Self::Int(x)),
            Value::Float(x) => Ok(Self::Float(x)),
            Value::String(x) => Ok(Self::String(x)),
            Value::Symbol(x) => Ok(Self::Symbol(x)),
            Value::Keyword(x) => Ok(Self::Keyword(x)),
            _ => Err(()),
        }
    }
}

impl Value {
    pub fn new<I: Into<Value>>(value: I) -> Self {
        value.into()
    }

    // XXX TODO use traits if possible
    pub fn truthy(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Bool(x) => *x,
            Self::Int(x) => !x.is_zero(),
            Self::Float(x) => !x.is_zero(),
            Self::String(x) => !x.is_empty(),
            Self::BuiltinFunction(_) => true,
            Self::Closure(_) => true,
            Self::Vector(x) => !x.is_empty(),
            Self::Table(x) => !x.is_empty(),
            Self::List(x) => !x.is_empty(),
            Self::Symbol(x) => !x.is_empty(),
            Self::Keyword(_) => true,
        }
    }

    pub fn falsy(&self) -> bool {
        !self.truthy()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Int(a), Self::Float(b)) => bigint_to_f64(a) == **b, // XXX ugly
            (Self::Float(a), Self::Int(b)) => **a == bigint_to_f64(b), // XXX ugly
            (Self::String(a), Self::String(b)) => a == b,
            (Self::BuiltinFunction(a), Self::BuiltinFunction(b)) => a == b,
            (Self::Closure(a), Self::Closure(b)) => a == b,
            (Self::Vector(a), Self::Vector(b)) => a == b,
            (Self::Table(a), Self::Table(b)) => a == b,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Symbol(a), Self::Symbol(b)) => a == b,
            (Self::Keyword(a), Self::Keyword(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Nil, Self::Nil) => Some(Ordering::Equal),
            (Self::Bool(a), Self::Bool(b)) => Some(a.cmp(b)),
            (Self::Int(a), Self::Int(b)) => Some(a.cmp(b)),
            (Self::Float(a), Self::Float(b)) => Some(a.cmp(b)),
            (Self::String(a), Self::String(b)) => Some(a.cmp(b)),
            (Self::BuiltinFunction(a), Self::BuiltinFunction(b)) => Some(a.cmp(b)),
            (Self::Closure(a), Self::Closure(b)) => a.partial_cmp(b),
            (Self::Vector(a), Self::Vector(b)) => a.partial_cmp(b),
            (Self::List(a), Self::List(b)) => a.partial_cmp(b),
            (Self::Symbol(a), Self::Symbol(b)) => Some(a.cmp(b)),
            (Self::Keyword(a), Self::Keyword(b)) => Some(a.cmp(b)),
            (Self::Int(a), Self::Float(b)) => {
                Some(OrderedFloat(bigint_to_f64(a)).cmp(b)) // XXX ugly
            }
            (Self::Float(a), Self::Int(b)) => {
                Some(a.cmp(&OrderedFloat(bigint_to_f64(b)))) // XXX ugly
            }
            _ => None,
        }
    }
}

fn sorted_entries(table: &HashMap<ValueKey, Value>) -> Vec<(&ValueKey, &Value)> {
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_unstable_by_key(|(key, _)| *key);
    entries
}

impl Display for ValueKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value:?}"),
            Self::String(value) => write!(f, "{value:?}"),
            Self::Symbol(value) => write!(f, "{value}"),
            Self::Keyword(value) => write!(f, ":{}", value.symbol()),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value:?}"),
            Self::String(value) => write!(f, "{value:?}"),
            Self::BuiltinFunction(func) => write!(f, "#<builtin function {}>", func.name),
            Self::Closure(_) => write!(f, "#<closure>"), // XXX TODO write name here
            Self::Vector(value) => {
                write!(f, "[")?;
                fmt_simple_sequence(f, value.iter(), " ")?;
                write!(f, "]")
            }
            Self::Table(value) => {
                let entries = sorted_entries(value);
                let mut peekable = entries.iter().peekable();

                write!(f, "{{")?;

                while let Some((key, val)) = peekable.next() {
                    key.fmt(f)?;
                    write!(f, " ")?;
                    val.fmt(f)?;

                    if peekable.peek().is_some() {
                        write!(f, " ")?;
                    }
                }

                write!(f, "}}")
            }
            Self::List(value) => {
                write!(f, "(")?;
                fmt_simple_sequence(f, value.iter(), " ")?;
                write!(f, ")")
            }
            Self::Symbol(value) => write!(f, "{value}"),
            Self::Keyword(value) => write!(f, ":{}", value.symbol()),
        }
    }
}

fn bigint_to_f64(a: &BigInt) -> f64 {
    a.to_f64().unwrap_or_else(|| {
        if a.is_positive() {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        }
    })
}

impl Neg for &Value {
    type Output = Result<Value, RuntimeError>;

    fn neg(self) -> Self::Output {
        match self {
            Value::Float(x) => Ok(Value::Float(-x)),
            Value::Int(x) => Ok(Value::new(-(*x).as_ref())),
            _ => Err(RuntimeError::MismatchedOpType("Negation")),
        }
    }
}

impl Neg for Value {
    type Output = Result<Value, RuntimeError>;

    fn neg(self) -> Self::Output {
        Neg::neg(&self)
    }
}

impl Add for &Value {
    type Output = Result<Value, RuntimeError>;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::new(a + b)),
            (Value::Int(a), Value::Int(b)) => Ok(Value::new((*a).as_ref() + (*b).as_ref())),
            (Value::Int(a), Value::Float(b)) => Ok(Value::new(bigint_to_f64(a) + **b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::new(*a + bigint_to_f64(b))),
            _ => Err(RuntimeError::MismatchedOpType("Addition")),
        }
    }
}

impl Add for Value {
    type Output = Result<Value, RuntimeError>;

    fn add(self, rhs: Self) -> Self::Output {
        Add::add(&self, &rhs)
    }
}

impl Sub for &Value {
    type Output = Result<Value, RuntimeError>;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::new(a - b)),
            (Value::Int(a), Value::Int(b)) => Ok(Value::new((*a).as_ref() - (*b).as_ref())),
            (Value::Int(a), Value::Float(b)) => Ok(Value::new(bigint_to_f64(a) - **b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::new(*a - bigint_to_f64(b))),
            _ => Err(RuntimeError::MismatchedOpType("Substraction")),
        }
    }
}

impl Sub for Value {
    type Output = Result<Value, RuntimeError>;

    fn sub(self, rhs: Self) -> Self::Output {
        Sub::sub(&self, &rhs)
    }
}

impl Mul for &Value {
    type Output = Result<Value, RuntimeError>;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::new(a * b)),
            (Value::Int(a), Value::Int(b)) => Ok(Value::new((*a).as_ref() * (*b).as_ref())),
            (Value::Int(a), Value::Float(b)) => Ok(Value::new(bigint_to_f64(a) * **b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::new(*a * bigint_to_f64(b))),
            _ => Err(RuntimeError::MismatchedOpType("Multiplication")),
        }
    }
}

impl Mul for Value {
    type Output = Result<Value, RuntimeError>;

    fn mul(self, rhs: Self) -> Self::Output {
        Mul::mul(&self, &rhs)
    }
}

impl Div for &Value {
    type Output = Result<Value, RuntimeError>;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Float(a), Value::Float(b)) => {
                if b.is_zero() {
                    Err(RuntimeError::FloatDivisionByZero)
                } else {
                    Ok(Value::Float(a / b))
                }
            }

            (Value::Int(a), Value::Int(b)) => {
                if b.is_zero() {
                    Err(RuntimeError::IntDivisionByZero)
                } else {
                    Ok(Value::new((*a).as_ref() / (*b).as_ref()))
                }
            }

            (Value::Int(a), Value::Float(b)) => {
                if b.is_zero() {
                    Err(RuntimeError::FloatDivisionByZero)
                } else {
                    Ok(Value::new(bigint_to_f64(a) / **b))
                }
            }

            (Value::Float(a), Value::Int(b)) => {
                if b.is_zero() {
                    Err(RuntimeError::FloatDivisionByZero)
                } else {
                    Ok(Value::new(**a / bigint_to_f64(b)))
                }
            }

            _ => Err(RuntimeError::MismatchedOpType("Division")),
        }
    }
}

impl Div for Value {
    type Output = Result<Value, RuntimeError>;

    fn div(self, rhs: Self) -> Self::Output {
        Div::div(&self, &rhs)
    }
}

impl From<Constant> for Value {
    fn from(value: Constant) -> Self {
        match value {
            Constant::Int(x) => Self::Int(x.clone()),
            Constant::String(x) => Self::String(x.clone()),
            Constant::Symbol(x) => Self::Symbol(x.clone()),
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<BigInt> for Value {
    fn from(value: BigInt) -> Self {
        Self::Int(value.into())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::from(BigInt::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::from(OrderedFloat(value))
    }
}

impl From<OrderedFloat<f64>> for Value {
    fn from(value: OrderedFloat<f64>) -> Self {
        Self::Float(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}

impl From<BuiltinFunction> for Value {
    fn from(value: BuiltinFunction) -> Self {
        Self::BuiltinFunction(value)
    }
}

impl From<Rc<[Value]>> for Value {
    fn from(value: Rc<[Value]>) -> Self {
        Self::Vector(value)
    }
}

impl From<Closure> for Value {
    fn from(value: Closure) -> Self {
        Self::Closure(value)
    }
}

impl From<Rc<HashMap<ValueKey, Value>>> for Value {
    fn from(value: Rc<HashMap<ValueKey, Value>>) -> Self {
        Self::Table(value)
    }
}

impl From<Keyword> for Value {
    fn from(value: Keyword) -> Self {
        Self::Keyword(value)
    }
}
