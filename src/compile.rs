use crate::constant::Constant;
use crate::function::FunctionPrototype;
use crate::instruction::Instruction;
use crate::keyword::Keyword;
use crate::parse::Syntax;
use crate::program::Program;
use crate::program::ProgramBuilder;
use crate::program::SymbolResolution;
use crate::symbol::Symbol;
use crate::util::fmt_simple_sequence;
use crate::value::Value;
use crate::value::ValueKey;
use num::{BigInt, ToPrimitive};
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use thiserror::Error;

#[derive(Clone)]
pub enum Expr {
    Nil,
    Bool(bool),
    Int(BigInt),
    Float(f64),
    String(Box<str>),
    Symbol(Symbol),
    List(Box<[Self]>),
    Vector(Box<[Self]>),
    Table(Box<[(Self, Self)]>),
    Quote(Box<Self>),
    Keyword(Keyword),
}

// TODO dedup Display implementations
impl Display for Expr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(x) => write!(f, "{}", x),
            Self::Int(x) => write!(f, "{}", x),
            Self::Float(x) => write!(f, "{}", x),
            Self::String(x) => write!(f, "{:?}", x),
            Self::Symbol(x) => write!(f, "{}", x),
            Self::List(x) => {
                write!(f, "(")?;
                fmt_simple_sequence(f, x.iter(), " ")?;
                write!(f, ")")
            }
            Self::Vector(x) => {
                write!(f, "[")?;
                fmt_simple_sequence(f, x.iter(), " ")?;
                write!(f, "]")
            }
            Self::Table(x) => {
                write!(f, "{{")?;
                let mut peekable = x.iter().peekable();

                while let Some((key, value)) = peekable.next() {
                    key.fmt(f)?;
                    write!(f, " ")?;
                    value.fmt(f)?;

                    if peekable.peek().is_some() {
                        write!(f, " ")?;
                    }
                }

                write!(f, "}}")
            }
            Self::Quote(x) => write!(f, "'{}", x),
            Self::Keyword(x) => write!(f, "{}", x),
        }
    }
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Too many arguments to special form")]
    TooManyArguments,
    #[error("Too few arguments to special form")]
    TooFewArguments,
    #[error("Expected symbol")]
    ExpectedSymbol,
    #[error("Expected vector")]
    ExpectedVector,
    #[error("Uncompileable value")]
    UncompileableExpr, // TODO report what value we can't compile
    #[error("Empty list in expression context")]
    EmptyList, // TODO decide whether to make this self-evaluating or something
}

pub type SpecialFormTarget =
    fn(builder: &mut ProgramBuilder, reader: ExprReader, tail: bool) -> Result<(), CompileError>;

#[derive(Clone)]
pub struct SpecialForm {
    name: Symbol,
    target: SpecialFormTarget,
}

#[derive(Clone)]
pub struct SpecialFormStore {
    map: HashMap<Symbol, SpecialForm>,
}

pub struct ExprReader {
    iter: std::vec::IntoIter<Expr>,
}

impl Iterator for ExprReader {
    type Item = Expr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl ExprReader {
    fn new(iter: std::vec::IntoIter<Expr>) -> Self {
        Self { iter }
    }

    fn as_slice(&self) -> &[Expr] {
        self.iter.as_slice()
    }

    fn len(&self) -> usize {
        self.iter.len()
    }

    fn next_required(&mut self) -> Result<Expr, CompileError> {
        self.next().ok_or(CompileError::TooFewArguments)
    }

    fn next_symbol_required(&mut self) -> Result<Symbol, CompileError> {
        match self.next_required()? {
            Expr::Symbol(item) => Ok(item),
            _ => Err(CompileError::ExpectedSymbol),
        }
    }

    fn next_vec_required(&mut self) -> Result<std::vec::IntoIter<Expr>, CompileError> {
        match self.next_required()? {
            Expr::Vector(item) => Ok(item.into_iter()),
            _ => Err(CompileError::ExpectedVector),
        }
    }

    fn end_required(&mut self) -> Result<(), CompileError> {
        if self.next().is_some() {
            Err(CompileError::TooManyArguments)
        } else {
            Ok(())
        }
    }
}

fn expect_symbol(syntax: Expr) -> Result<Symbol, CompileError> {
    match syntax {
        Expr::Symbol(sym) => Ok(sym),
        _ => Err(CompileError::ExpectedSymbol),
    }
}

impl SpecialForm {
    pub fn new(name: impl Into<Symbol>, target: SpecialFormTarget) -> Self {
        Self {
            name: name.into(),
            target,
        }
    }

    fn symbol(&self) -> &Symbol {
        &self.name
    }

    fn target(&self) -> SpecialFormTarget {
        self.target
    }
}

impl SpecialFormStore {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, form: SpecialForm) {
        self.map.insert(form.symbol().to_owned(), form);
    }

    pub fn get(&self, key: &Symbol) -> Option<&SpecialForm> {
        self.map.get(key)
    }
}

#[derive(Clone, Copy)]
enum CallTarget<'a> {
    Dynamic,
    SpecialForm(&'a SpecialForm),
}

fn resolve_call_target<'a>(builder: &'a ProgramBuilder, syntax: &Expr) -> CallTarget<'a> {
    let sym = match syntax {
        Expr::Symbol(sym) => sym,
        _ => return CallTarget::Dynamic,
    };

    match builder.get_special_form(&sym) {
        Some(form) => CallTarget::SpecialForm(form),
        None => CallTarget::Dynamic,
    }
}

fn compile_args(builder: &mut ProgramBuilder, reader: ExprReader) -> Result<(), CompileError> {
    for arg in reader {
        compile_expr(builder, arg, false)?;
    }

    Ok(())
}

fn special_not(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_expr(builder, reader.next_required()?, false)?;
    reader.end_required()?;
    builder.emit(Instruction::Not);
    Ok(())
}

fn special_if(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let cond = reader.next_required()?;
    let true_arm = reader.next_required()?;
    let false_arm = reader.next_required()?;

    reader.end_required()?;
    compile_expr(builder, cond, false)?;

    let jump_to_false = builder.emit_jump(Instruction::JumpIfFalse(0));

    compile_expr(builder, true_arm, tail)?;

    let jump_to_end = builder.emit_jump(Instruction::Jump(0));
    let false_arm_start = builder.position();

    compile_expr(builder, false_arm, tail)?;
    builder.patch_jump(jump_to_false, false_arm_start);
    builder.patch_jump(jump_to_end, builder.position());
    Ok(())
}

fn special_when(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let cond = reader.next_required()?;
    compile_expr(builder, cond, false)?;
    let jump_to_false = builder.emit_jump(Instruction::JumpIfFalse(0));
    compile_progn(builder, reader, tail)?;
    let jump_to_end = builder.emit_jump(Instruction::Jump(0));
    builder.patch_jump(jump_to_false, builder.position());
    builder.emit(Instruction::LoadNil);
    builder.patch_jump(jump_to_end, builder.position());
    Ok(())
}

fn special_cond(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let mut end_jumps = vec![];

    while let Some(cond) = reader.next() {
        let body = reader.next_required()?;
        compile_expr(builder, cond, false)?;
        let jump_to_next = builder.emit_jump(Instruction::JumpIfFalse(0));
        compile_expr(builder, body, tail && reader.len() == 0)?;
        let jump_to_end = builder.emit_jump(Instruction::Jump(0));
        end_jumps.push(jump_to_end);
        builder.patch_jump(jump_to_next, builder.position());
    }

    builder.emit(Instruction::LoadNil);

    for jump in end_jumps {
        builder.patch_jump(jump, builder.position());
    }

    Ok(())
}

fn compile_function_body(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    compile_progn(builder, reader, tail)?;
    builder.emit(Instruction::Return);
    Ok(())
}

fn compile_progn(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let total = reader.len();

    if total == 0 {
        builder.emit(Instruction::LoadNil);
        return Ok(());
    }

    for (index, value) in reader.enumerate() {
        let last = index + 1 == total;

        if last && total > 1 {
            builder.emit(Instruction::Pop(total - 1));
        }

        compile_expr(builder, value, tail && last)?;
    }

    Ok(())
}

fn special_fn(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let arg_vec = reader.next_vec_required()?;
    let mut num_args = 0;
    let chunk = builder.push_chunk_scope();

    for arg in arg_vec {
        let sym = expect_symbol(arg)?;
        builder.insert_local(sym);
        num_args += 1;
    }

    compile_function_body(builder, reader, true)?;
    builder.pop_chunk_scope();
    let proto = FunctionPrototype::new(num_args, chunk);
    let proto_idx = builder.register_proto(proto);
    builder.emit(Instruction::LoadClosure(proto_idx));
    Ok(())
}

fn special_defnl(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let name = reader.next_symbol_required()?;
    let index = builder.insert_local(name); // defnl always shadows variables
    let arg_vec = reader.next_vec_required()?;
    let mut num_args = 0;
    let chunk = builder.push_chunk_scope();

    for arg in arg_vec {
        let sym = expect_symbol(arg)?;
        builder.insert_local(sym);
        num_args += 1;
    }

    compile_function_body(builder, reader, true)?;
    builder.pop_chunk_scope();
    let proto = FunctionPrototype::new(num_args, chunk);
    let proto_idx = builder.register_proto(proto);
    builder.emit(Instruction::LoadClosure(proto_idx));
    builder.emit(Instruction::StoreLocal(index));
    builder.emit(Instruction::LoadLocal(index)); // so defnl expressions have a value
    Ok(())
}

fn special_defn(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let name = reader.next_symbol_required()?;
    // XXX TODO clean up
    let index = if builder.is_toplevel() {
        builder.intern_symbol(name)
    } else {
        builder.insert_local(name) // defn always shadows non global variables
    };

    let arg_vec = reader.next_vec_required()?;
    let mut num_args = 0;
    let chunk = builder.push_chunk_scope();

    for arg in arg_vec {
        let sym = expect_symbol(arg)?;
        builder.insert_local(sym);
        num_args += 1;
    }

    compile_function_body(builder, reader, true)?;
    builder.pop_chunk_scope();
    let proto = FunctionPrototype::new(num_args, chunk);
    let proto_idx = builder.register_proto(proto);
    builder.emit(Instruction::LoadClosure(proto_idx));

    // XXX TODO dedup with defnl
    if builder.is_toplevel() {
        builder.emit(Instruction::StoreGlobal(index));
        builder.emit(Instruction::LoadGlobal(index)); // so defn expressions have a "value"
    } else {
        builder.emit(Instruction::StoreLocal(index));
        builder.emit(Instruction::LoadLocal(index)); // so defn expressions have a value
    }

    Ok(())
}

fn special_defl(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let sym = reader.next_symbol_required()?;
    let value = reader.next_required()?;
    reader.end_required()?;

    compile_expr(builder, value, false)?;
    let index = builder.insert_local(sym); // defl always shadows variables
    builder.emit(Instruction::StoreLocal(index));
    builder.emit(Instruction::LoadLocal(index)); // so defl expressions have a value
    Ok(())
}

fn special_def(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let sym = reader.next_symbol_required()?;
    let value = reader.next_required()?;
    reader.end_required()?;

    compile_expr(builder, value, false)?;

    // XXX TODO dedup with defl
    if builder.is_toplevel() {
        let index = builder.intern_symbol(sym);
        builder.emit(Instruction::StoreGlobal(index));
        builder.emit(Instruction::LoadGlobal(index)); // so def expressions have a "value"
    } else {
        let index = builder.insert_local(sym); // def always shadows non global variables
        builder.emit(Instruction::StoreLocal(index));
        builder.emit(Instruction::LoadLocal(index)); // so def expressions have a value
    }

    Ok(())
}

fn special_let(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let bindings = reader.next_vec_required()?;
    let mut bindings_reader = ExprReader::new(bindings);

    builder.push_local_scope();

    while let Some(binding) = bindings_reader.next() {
        let binding = match binding {
            Expr::Symbol(sym) => sym,
            _ => return Err(CompileError::ExpectedSymbol),
        };
        let value = bindings_reader.next_required()?;
        compile_expr(builder, value, false)?;
        let index = builder.insert_local(binding); // let shadows only after its initializer
        builder.emit(Instruction::StoreLocal(index));
    }

    compile_progn(builder, reader, tail)?;
    builder.pop_local_scope();
    Ok(())
}

fn special_do(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    compile_progn(builder, reader, tail)
}

fn special_quote(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let value = reader.next_required()?;
    reader.end_required()?;
    compile_quoted(builder, value)
}

fn special_keyword(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let value = reader.next_symbol_required()?;
    reader.end_required()?;
    let index = builder.intern_keyword(value);
    builder.emit(Instruction::LoadKeyword(index));
    Ok(())
}

fn special_table(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let len = reader.len();

    while let Some(key) = reader.next() {
        let value = reader.next_required()?;
        compile_expr(builder, key, false)?;
        compile_expr(builder, value, false)?;
    }

    builder.emit(Instruction::MakeTable(len));
    reader.end_required()?;
    Ok(())
}

fn special_eval(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let value = reader.next_required()?;
    reader.end_required()?;
    compile_expr(builder, value, false)?;
    builder.emit(Instruction::Eval);
    Ok(())
}

fn special_set(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    let sym = reader.next_symbol_required()?;
    let value = reader.next_required()?;
    compile_expr(builder, value, false)?;
    reader.end_required()?;

    match builder.resolve_symbol(&sym) {
        SymbolResolution::Local(index) => {
            builder.emit(Instruction::StoreLocal(index));
            builder.emit(Instruction::LoadLocal(index)); // so set expressions have a "value"
        }

        SymbolResolution::Global => {
            let index = builder.intern_symbol(sym);
            builder.emit(Instruction::StoreGlobal(index));
            builder.emit(Instruction::LoadGlobal(index)); // so set expressions have a "value"
        }

        SymbolResolution::Upvalue(desc) => {
            let index = builder.insert_upvalue(desc);
            builder.emit(Instruction::StoreUpvalue(index));
            builder.emit(Instruction::LoadUpvalue(index));
        }
    }

    Ok(())
}

fn compile_list_expr(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    tail: bool,
) -> Result<(), CompileError> {
    let head = reader.next().ok_or(CompileError::EmptyList)?;

    match resolve_call_target(builder, &head) {
        CallTarget::SpecialForm(form) => {
            let target = form.target();
            (target)(builder, reader, tail)
        }

        CallTarget::Dynamic => {
            let len = reader.len();
            compile_args(builder, reader)?;
            compile_expr(builder, head, false)?;

            if tail {
                builder.emit(Instruction::PossibleTailCall(len));
            } else {
                builder.emit(Instruction::Call(len));
            }

            Ok(())
        }
    }
}

fn compile_quoted(builder: &mut ProgramBuilder, expr: Expr) -> Result<(), CompileError> {
    match expr {
        Expr::Quote(x) => compile_quoted(builder, *x),

        Expr::Table(items) => {
            let len = items.len();

            for (key, value) in items.into_iter() {
                compile_expr(builder, Expr::from(key.clone()), false)?;
                compile_expr(builder, value, false)?;
            }

            builder.emit(Instruction::MakeTable(len * 2));
            Ok(())
        }

        Expr::Keyword(keyword) => {
            builder.emit(Instruction::LoadKeyword(keyword.index()));
            Ok(())
        }

        Expr::Nil => {
            builder.emit(Instruction::LoadNil);
            Ok(())
        }

        Expr::Vector(items) => {
            let len = items.len();

            for item in items.into_iter() {
                compile_quoted(builder, item)?;
            }

            builder.emit(Instruction::MakeVector(len));
            Ok(())
        }

        Expr::Bool(false) => {
            builder.emit(Instruction::LoadFalse);
            Ok(())
        }

        Expr::Bool(true) => {
            builder.emit(Instruction::LoadTrue);
            Ok(())
        }

        Expr::Float(float) => {
            builder.emit(Instruction::LoadFloat(float));
            Ok(())
        }

        Expr::Int(int) => match int.to_i64() {
            Some(small) => {
                builder.emit(Instruction::LoadSmallInt(small));
                Ok(())
            }

            None => {
                let index = builder.intern_constant(Constant::new(int.clone()));
                builder.emit(Instruction::LoadConstant(index));
                Ok(())
            }
        },

        Expr::String(str) => {
            let index = builder.intern_constant(Constant::new(str.as_ref()));
            builder.emit(Instruction::LoadConstant(index));
            Ok(())
        }

        Expr::Symbol(sym) => {
            let index = builder.intern_constant(Constant::new(sym.clone()));
            builder.emit(Instruction::LoadConstant(index));
            Ok(())
        }

        Expr::List(items) => {
            let len = items.len();

            for item in items.into_iter() {
                compile_quoted(builder, item)?;
            }

            builder.emit(Instruction::MakeList(len));
            Ok(())
        }
    }
}

fn compile_expr(builder: &mut ProgramBuilder, expr: Expr, tail: bool) -> Result<(), CompileError> {
    let debug_text = expr.to_string().into_boxed_str();

    match expr {
        Expr::Symbol(sym) => {
            match builder.resolve_symbol(&sym) {
                SymbolResolution::Local(index) => {
                    builder.emit(Instruction::LoadLocal(index));
                }

                SymbolResolution::Global => {
                    let index = builder.intern_symbol(sym);
                    builder.emit(Instruction::LoadGlobal(index));
                }

                // XXX TODO does this still hold up
                // with the new hole punching recursive lookup algorithm?
                SymbolResolution::Upvalue(desc) => {
                    let index = builder.insert_upvalue(desc);
                    builder.emit(Instruction::LoadUpvalue(index));
                }
            }

            Ok(())
        }

        Expr::Vector(items) => {
            let debug_start = builder.position();
            let len = items.len();

            for item in items.into_iter() {
                compile_expr(builder, item, false)?;
            }

            builder.emit(Instruction::MakeVector(len));
            builder.add_debug_info(debug_start, builder.position(), debug_text);
            Ok(())
        }

        Expr::Table(items) => {
            let debug_start = builder.position();
            let len = items.len();

            for (key, value) in items.into_iter() {
                compile_expr(builder, Expr::from(key.clone()), false)?;
                compile_expr(builder, value, false)?;
            }

            builder.emit(Instruction::MakeTable(len * 2));
            builder.add_debug_info(debug_start, builder.position(), debug_text);
            Ok(())
        }

        Expr::List(expr) => {
            let debug_start = builder.position();
            let reader = ExprReader::new(expr.into_iter());
            compile_list_expr(builder, reader, tail)?;
            builder.add_debug_info(debug_start, builder.position(), debug_text);
            Ok(())
        }

        _ => compile_quoted(builder, expr),
    }
}

fn compile_varop(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    target: impl FnOnce(usize) -> Instruction,
) -> Result<(), CompileError> {
    let len = reader.len();

    if reader.as_slice().is_empty() {
        return Err(CompileError::TooFewArguments);
    }

    compile_args(builder, reader)?;
    builder.emit(target(len));
    Ok(())
}

fn special_add(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_varop(builder, reader, Instruction::Add)
}

fn special_sub(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    if let [_] = reader.as_slice() {
        compile_expr(builder, reader.next().unwrap(), false)?; // XXX
        builder.emit(Instruction::Neg);
        Ok(())
    } else {
        compile_varop(builder, reader, Instruction::Sub)
    }
}

fn special_mul(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_varop(builder, reader, Instruction::Mul)
}

fn special_div(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    if let [_] = reader.as_slice() {
        compile_expr(builder, reader.next().unwrap(), false)?; // XXX
        builder.emit(Instruction::Recip);
        Ok(())
    } else {
        compile_varop(builder, reader, Instruction::Div)
    }
}

fn compile_binop(
    builder: &mut ProgramBuilder,
    mut reader: ExprReader,
    target: Instruction,
) -> Result<(), CompileError> {
    compile_expr(builder, reader.next_required()?, false)?;
    compile_expr(builder, reader.next_required()?, false)?;
    reader.end_required()?;
    builder.emit(target);
    Ok(())
}

fn special_gt(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::Greater)
}

fn special_ge(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::GreaterOrEqual)
}

fn special_lt(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::Less)
}

fn special_le(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::LessOrEqual)
}

fn special_eq(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::Equal)
}

fn special_ne(
    builder: &mut ProgramBuilder,
    reader: ExprReader,
    _tail: bool,
) -> Result<(), CompileError> {
    compile_binop(builder, reader, Instruction::NotEqual)
}

fn syntax_table_to_expr(builder: &mut ProgramBuilder, table: Vec<(Syntax, Syntax)>) -> Expr {
    let mut expr = vec![];

    for (key, val) in table {
        let key = syntax_to_expr(builder, key);
        let val = syntax_to_expr(builder, val);
        expr.push((key, val));
    }

    Expr::Table(expr.into())
}

fn syntax_to_expr(builder: &mut ProgramBuilder, syntax: Syntax) -> Expr {
    match syntax {
        Syntax::Nil => Expr::Nil,
        Syntax::Bool(x) => Expr::Bool(x),
        Syntax::Int(x) => Expr::Int(x),
        Syntax::Float(x) => Expr::Float(x),
        Syntax::String(x) => Expr::String(x),
        Syntax::Symbol(x) => Expr::Symbol(x),
        Syntax::List(x) => Expr::List(x.into_iter().map(|v| syntax_to_expr(builder, v)).collect()),
        Syntax::Vector(x) => {
            Expr::Vector(x.into_iter().map(|v| syntax_to_expr(builder, v)).collect())
        }
        Syntax::Table(x) => syntax_table_to_expr(builder, x),
        Syntax::Quote(x) => Expr::Quote(syntax_to_expr(builder, *x).into()),
        Syntax::Keyword(x) => {
            let index = builder.intern_keyword(x.clone());
            Expr::Keyword(Keyword::new(index, x))
        }
    }
}

pub fn syntax_to_exprs(builder: &mut ProgramBuilder, syntax: Box<[Syntax]>) -> Box<[Expr]> {
    Vec::from_iter(syntax.into_iter().map(|v| syntax_to_expr(builder, v))).into()
}

fn load_standard_special_forms(builder: &mut ProgramBuilder) {
    builder.add_special_form("+", special_add);
    builder.add_special_form("-", special_sub);
    builder.add_special_form("*", special_mul);
    builder.add_special_form("/", special_div);
    builder.add_special_form(">", special_gt);
    builder.add_special_form(">=", special_ge);
    builder.add_special_form("<", special_lt);
    builder.add_special_form("<=", special_le);
    builder.add_special_form("==", special_eq);
    builder.add_special_form("!=", special_ne);
    builder.add_special_form("not", special_not);
    builder.add_special_form("if", special_if);
    builder.add_special_form("when", special_when);

    builder.add_special_form("defl", special_defl);
    builder.add_special_form("def", special_def);

    builder.add_special_form("set", special_set);
    builder.add_special_form("let", special_let);
    builder.add_special_form("fn", special_fn);

    builder.add_special_form("defn", special_defn);
    builder.add_special_form("defnl", special_defnl);

    builder.add_special_form("do", special_do);

    builder.add_special_form("eval", special_eval);
    builder.add_special_form("cond", special_cond);

    // TODO add a vector, list builtin
    builder.add_special_form("quote", special_quote);
    builder.add_special_form("table", special_table);
    builder.add_special_form("keyword", special_keyword);
}

pub fn compile_syntax(
    mut builder: ProgramBuilder,
    toplevel: Box<[Syntax]>,
) -> Result<Program, CompileError> {
    load_standard_special_forms(&mut builder);

    let toplevel = syntax_to_exprs(&mut builder, toplevel);
    let total = toplevel.len();

    if total > 0 {
        for (index, expr) in toplevel.into_iter().enumerate() {
            if index + 1 == total && total > 1 {
                builder.emit(Instruction::Pop(total - 1));
            }

            compile_expr(&mut builder, expr, false)?;
        }
    } else {
        builder.emit(Instruction::LoadNil);
    }

    // XXX TODO dedup with function body compilation
    builder.emit(Instruction::Return);
    Ok(builder.finish())
}

fn value_table_to_expr(
    table: std::collections::hash_map::Iter<'_, ValueKey, Value>,
) -> Result<Expr, CompileError> {
    let mut expr = vec![];

    for (key, val) in table {
        let key = value_to_expr(key.clone().into())?;
        let val = value_to_expr(val.clone())?;
        expr.push((key, val));
    }

    Ok(Expr::Table(expr.into()))
}

fn value_to_expr(value: Value) -> Result<Expr, CompileError> {
    match value {
        Value::Nil => Ok(Expr::Nil),
        Value::Bool(x) => Ok(Expr::Bool(x)),
        Value::Int(x) => Ok(Expr::Int(x.as_ref().clone())),
        Value::Float(x) => Ok(Expr::Float(*x)),
        Value::String(x) => Ok(Expr::String(Box::from(x.as_ref()))),
        Value::Symbol(x) => Ok(Expr::Symbol(x)),
        Value::List(x) => {
            let items: Vec<_> = x
                .into_iter()
                .flat_map(|v| value_to_expr(v.clone()))
                .collect();
            Ok(Expr::List(items.into()))
        }
        Value::Vector(x) => {
            let items: Vec<_> = x
                .into_iter()
                .flat_map(|v| value_to_expr(v.clone()))
                .collect();
            Ok(Expr::Vector(items.into()))
        }
        Value::Table(x) => value_table_to_expr(x.iter()),
        Value::Keyword(x) => Ok(Expr::Keyword(x)),
        _ => Err(CompileError::UncompileableExpr),
    }
}

pub fn compile_eval(mut builder: ProgramBuilder, value: Value) -> Result<Program, CompileError> {
    let expr = value_to_expr(value)?;
    load_standard_special_forms(&mut builder);
    compile_expr(&mut builder, expr, false)?;
    builder.emit(Instruction::Return);
    Ok(builder.finish())
}
