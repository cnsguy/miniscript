// XXX TODO reorder the impl blocks in this file
use crate::compile::{
    CompileError, SpecialForm, SpecialFormStore, SpecialFormTarget, compile_eval,
};
use crate::constant::Constant;
use crate::function::FunctionPrototype;
use crate::global::GlobalStore;
use crate::instruction::{Instruction, InstructionBuilder, InstructionReader, InstructionStream};
use crate::keyword::Keyword;
use crate::local::{LocalMap, LocalTracker};
use crate::symbol::Symbol;
use crate::util::InternBuilder;
use crate::value::{BuiltinFnPtr, BuiltinFunction, Cell, Closure, Value, ValueKey};
use std::cmp::Ordering;
use std::collections::{HashMap, TryReserveError};
use std::fmt::{self, Display, Formatter};
use std::ptr;
use std::rc::Rc;
use std::vec;
use thiserror::Error;

// TODO make this customizable?
const MAX_CALL_DEPTH: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpvalueDescriptor {
    Local(usize),
    Upvalue(usize),
}

#[derive(Clone)]
struct UpvalueTracker {
    upvalues: Vec<UpvalueDescriptor>,
}

type DebugInfo = Box<str>;
type DebugIP = usize;
type DebugEntry = (DebugIP, DebugIP, DebugInfo);

#[derive(Clone)]
struct DebugTracker {
    ranges: Vec<DebugEntry>,
}

#[derive(Clone)]
pub struct ChunkBuilder {
    insns: InstructionBuilder,
    locals: LocalTracker,
    upvalues: UpvalueTracker,
    debug: DebugTracker,
}

pub struct Chunk {
    insns: Box<InstructionStream>,
    num_locals: usize,
    upvalues: Box<[UpvalueDescriptor]>,
    debug: Box<[DebugEntry]>,
}

struct ProgramData {
    consts: Box<[Constant]>,
    symbols: Box<[Symbol]>,
    keywords: Box<[Symbol]>,
    chunks: Box<[Chunk]>,
    protos: Box<[FunctionPrototype]>,
    builder: ProgramBuilder,
    main_chunk: usize,
}

struct MutableProgramData {
    stack: ValueStack,
    locals: LocalStack,
    globals: GlobalStore,
}

pub struct Program {
    data: ProgramData,
    data_mut: MutableProgramData,
}

#[derive(Clone)]
pub struct ChunkTracker {
    chunks: Vec<ChunkBuilder>,
    chunk_stack: Vec<usize>,
}

#[derive(Clone)]
pub struct ProgramBuilder {
    consts: InternBuilder<Constant>,
    symbols: InternBuilder<Symbol>,
    keywords: InternBuilder<Symbol>,
    specials: SpecialFormStore,
    chunks: ChunkTracker,
    protos: Vec<FunctionPrototype>,
}

#[derive(Debug, Clone)]
pub struct ValueStack {
    values: Vec<Value>,
    frames: Vec<usize>,
}

#[derive(Debug, Clone)]
struct LocalStack {
    cells: Vec<Cell>,
    frames: Vec<usize>,
}

// XXX TODO better errors
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Invalid constant referenced: {0}")]
    InvalidConstant(usize),
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Invalid symbol referenced: {0}")]
    InvalidSymbol(usize),
    #[error("Undefined global referenced: {0}")]
    UndefinedGlobal(Symbol),
    #[error("Tried to call a non-callable value: {0}")]
    NotCallable(Value),
    #[error("Float division by zero")]
    FloatDivisionByZero,
    #[error("Integer division by zero")]
    IntDivisionByZero,
    #[error("Mismatched operand types for: {0}")]
    MismatchedOpType(&'static str), // XXX TODO: get rid of hardcoded strings here for v1 release
    #[error("Integer too large for this operation")]
    IntTooLarge,
    #[error("Too many arguments to function")]
    TooManyArguments,
    #[error("Too few arguments to function")]
    TooFewArguments,
    #[error("Mimsatched function arguments: expected {0} at position {1}")]
    MismatchedArgType(&'static str, usize), // XXX TODO: get rid of hardcoded strings here for v1 release
    #[error("Failed to reserve memory: {0}")]
    TryReserveError(#[from] TryReserveError),
    #[error("Invalid local referenced: {0}")]
    InvalidLocal(usize),
    #[error("Invalid function referenced: {0}")]
    InvalidFunction(usize),
    #[error("Invalid chunk referenced: {0}")]
    InvalidChunk(usize),
    #[error("Invalid upvalue referenced: {0}")]
    InvalidUpvalue(usize),
    #[error("Maximum recursion depth exceeded")]
    RecursionTooDeep,
    #[error("Invalid table key")]
    InvalidKey,
    #[error("Compilation error: {0}")]
    Compile(#[from] CompileError),
    #[error("Invalid keyword: {0}")]
    InvalidKeyword(usize),
    #[error("Uncompareable values compared")]
    UnorderableValues,
}

#[derive(Debug, Error)]
pub struct ProgramError {
    err: RuntimeError,
    backtrace: Vec<DebugInfo>,
}

impl Display for ProgramError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.err())
    }
}

impl ProgramError {
    fn wrap_err(err: RuntimeError, info: DebugInfo) -> Self {
        Self {
            err,
            backtrace: vec![info],
        }
    }

    fn wrap_program_err(existing: Self, info: DebugInfo) -> Self {
        let mut backtrace = existing.backtrace;
        backtrace.push(info);

        Self {
            err: existing.err,
            backtrace: backtrace,
        }
    }

    pub fn err(&self) -> &RuntimeError {
        &self.err
    }

    pub fn backtrace(&self) -> &[DebugInfo] {
        &self.backtrace
    }
}

// XXX rewrite to an iterator-based solution
fn lookup_debug_info(ip: usize, debug: &[DebugEntry]) -> DebugInfo {
    let mut last_info = None;

    for (start_ip, end_ip, info) in debug {
        if ip >= *start_ip && ip < *end_ip {
            last_info = Some(info);
            break;
        }
    }

    last_info
        .cloned()
        .unwrap_or_else(|| "<unknown>".to_owned().into_boxed_str())
}

fn wrap_err(err: RuntimeError, ip: usize, debug: &[DebugEntry]) -> ProgramError {
    ProgramError::wrap_err(err, lookup_debug_info(ip, debug))
}

fn wrap_program_err(err: ProgramError, ip: usize, debug: &[DebugEntry]) -> ProgramError {
    ProgramError::wrap_program_err(err, lookup_debug_info(ip, debug))
}

impl ValueStack {
    fn new() -> Self {
        Self {
            values: Vec::with_capacity(512),
            frames: Vec::with_capacity(64),
        }
    }

    fn push_frame(&mut self) {
        self.frames.push(self.values.len());
    }

    fn pop_frame(&mut self) {
        let new_top = self.frames.pop().unwrap();
        self.values.truncate(new_top);
    }

    fn push(&mut self, value: Value) {
        self.values.push(value)
    }

    fn pop(&mut self) -> Option<Value> {
        self.values.pop()
    }

    fn pop_required(&mut self) -> Result<Value, RuntimeError> {
        self.pop().ok_or(RuntimeError::StackUnderflow)
    }

    pub fn pop_many(&mut self, num: usize) -> Option<vec::Drain<'_, Value>> {
        if num > self.values.len() {
            return None;
        }

        let start = self.values.len() - num;
        Some(self.values.drain(start..))
    }

    fn pop_many_required(&mut self, num: usize) -> Result<vec::Drain<'_, Value>, RuntimeError> {
        self.pop_many(num).ok_or(RuntimeError::StackUnderflow)
    }
}

impl LocalStack {
    fn new() -> Self {
        Self {
            cells: Vec::with_capacity(512),
            frames: Vec::with_capacity(64),
        }
    }

    fn push_frame(&mut self) {
        self.frames.push(self.cells.len());
    }

    fn pop_frame(&mut self) {
        let new_top = self.frames.pop().unwrap();
        self.cells.truncate(new_top);
    }

    fn push_value(&mut self, value: Value) {
        self.cells.push(Cell::new(value));
    }

    fn frame(&mut self) -> Option<&mut [Cell]> {
        let top_start = self.frames.last().copied()?;
        Some(&mut self.cells[top_start..])
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut Cell> {
        self.frame()?.get_mut(index)
    }

    fn get_mut_required(&mut self, index: usize) -> Result<&mut Cell, RuntimeError> {
        self.get_mut(index).ok_or(RuntimeError::InvalidLocal(index))
    }
}

fn arith_op<F>(stack: &mut ValueStack, num: usize, func: F) -> Result<(), RuntimeError>
where
    F: Fn(&Value, &Value) -> Result<Value, RuntimeError>,
{
    if num == 0 {
        return Ok(());
    }

    let mut values = stack.pop_many(num).ok_or(RuntimeError::StackUnderflow)?;
    let mut accum = values.next().unwrap();

    for value in values {
        accum = func(&accum, &value)?;
    }

    stack.push(accum);
    Ok(())
}

fn cmp_op<F>(stack: &mut ValueStack, func: F) -> Result<(), RuntimeError>
where
    F: Fn(&Value, &Value) -> bool,
{
    let b = stack.pop_required()?;
    let a = stack.pop_required()?;
    let res = func(&a, &b);
    stack.push(Value::new(res));
    Ok(())
}

fn run_chunk(
    data: &ProgramData,
    data_mut: &mut MutableProgramData,
    chunk: &Chunk,
    mut upvalues: Rc<[Cell]>,
    call_depth: usize,
) -> Result<Value, ProgramError> {
    let mut reader = InstructionReader::new(chunk.insns());

    if call_depth > MAX_CALL_DEPTH {
        return Err(RuntimeError::RecursionTooDeep)
            .map_err(|err| wrap_err(err, reader.ip(), chunk.debug()));
    }

    let mut last_ip = 0;

    while let Some(insn) = reader.next() {
        let single_trace = |err| wrap_err(err, last_ip, chunk.debug());
        let program_trace = |err| wrap_program_err(err, last_ip, chunk.debug());

        match insn {
            Instruction::LoadNil => data_mut.push(Value::Nil),
            Instruction::LoadFalse => data_mut.push(Value::new(false)),
            Instruction::LoadTrue => data_mut.push(Value::new(true)),
            Instruction::LoadFloat(float) => data_mut.push(Value::new(float)),
            Instruction::LoadSmallInt(int) => data_mut.push(Value::new(int)), // TODO this is early optimization
            Instruction::LoadConstant(index) => {
                let item = data
                    .consts
                    .get(index)
                    .ok_or(RuntimeError::InvalidConstant(index))
                    .map_err(single_trace)?;

                data_mut.push(Value::new(item.clone()));
            }

            Instruction::Add(num) => {
                arith_op(data_mut.stack_mut(), num, |a, b| a + b).map_err(single_trace)?
            }
            Instruction::Sub(num) => {
                arith_op(data_mut.stack_mut(), num, |a, b| a - b).map_err(single_trace)?
            }
            Instruction::Mul(num) => {
                arith_op(data_mut.stack_mut(), num, |a, b| a * b).map_err(single_trace)?
            }
            Instruction::Div(num) => {
                arith_op(data_mut.stack_mut(), num, |a, b| a / b).map_err(single_trace)?
            }

            Instruction::Neg => {
                let value = data_mut.pop_required().map_err(single_trace)?;
                data_mut.push((-value).map_err(single_trace)?);
            }

            Instruction::Recip => {
                let value = data_mut.pop_required().map_err(single_trace)?;
                let one = Value::new(1);
                data_mut.push((one / value).map_err(single_trace)?);
            }

            Instruction::Call(num_args) => {
                let target = data_mut.pop_required().map_err(single_trace)?;

                match target {
                    Value::BuiltinFunction(func) => {
                        let result = func
                            .call(data_mut.stack_mut(), num_args)
                            .map_err(single_trace)?;

                        data_mut.push(result);
                    }

                    Value::Closure(closure) => {
                        let proto_idx = closure.proto_index();

                        let proto = data
                            .protos
                            .get(proto_idx)
                            .ok_or(RuntimeError::InvalidFunction(proto_idx))
                            .map_err(single_trace)?;

                        if num_args > proto.num_args() {
                            return Err(RuntimeError::TooManyArguments).map_err(single_trace);
                        } else if num_args < proto.num_args() {
                            return Err(RuntimeError::TooFewArguments).map_err(single_trace);
                        }

                        let chunk_idx = proto.chunk();

                        let target_chunk = data
                            .chunks
                            .get(chunk_idx)
                            .ok_or(RuntimeError::InvalidChunk(chunk_idx))
                            .map_err(single_trace)?;

                        let args = data_mut
                            .stack
                            .pop_many_required(proto.num_args())
                            .map_err(single_trace)?;

                        data_mut.locals.push_frame();

                        for arg in args {
                            data_mut.locals.push_value(arg);
                        }

                        for _ in 0..target_chunk.num_locals() {
                            data_mut.locals.push_value(Value::Nil);
                        }

                        data_mut.stack.push_frame();

                        let result = run_chunk(
                            data,
                            data_mut,
                            target_chunk,
                            closure.upvalues(),
                            call_depth + 1,
                        )
                        .map_err(program_trace)?;

                        data_mut.locals.pop_frame();
                        data_mut.stack.pop_frame();
                        data_mut.push(result);
                    }

                    _ => {
                        return Err(RuntimeError::NotCallable(target)).map_err(single_trace);
                    }
                }
            }

            // XXX TODO deduplicate this with Call
            // we emit this for any tail position right now
            // eventually we will want to only emit this for genuine tail calls
            // for now however, there's tons of duplicate logic shared with Call
            Instruction::PossibleTailCall(num_args) => {
                let target = data_mut.pop_required().map_err(single_trace)?;

                match target {
                    Value::BuiltinFunction(func) => {
                        let result = func
                            .call(data_mut.stack_mut(), num_args)
                            .map_err(single_trace)?;

                        data_mut.push(result);
                    }

                    Value::Closure(closure) => {
                        let proto = data
                            .protos
                            .get(closure.proto_index())
                            .ok_or(RuntimeError::InvalidFunction(closure.proto_index()))
                            .map_err(single_trace)?;

                        if num_args > proto.num_args() {
                            return Err(RuntimeError::TooManyArguments).map_err(single_trace);
                        } else if num_args < proto.num_args() {
                            return Err(RuntimeError::TooFewArguments).map_err(single_trace);
                        }

                        let chunk_idx = proto.chunk();

                        let target_chunk = data
                            .chunks
                            .get(chunk_idx)
                            .ok_or(RuntimeError::InvalidChunk(chunk_idx))
                            .map_err(single_trace)?;

                        let args = data_mut
                            .stack
                            .pop_many_required(proto.num_args())
                            .map_err(single_trace)?;

                        // Genuine tail call case, reset mut_data.locals and reset the instruction reader
                        if ptr::eq(target_chunk, chunk) {
                            data_mut.locals.pop_frame();
                            data_mut.locals.push_frame();

                            for arg in args {
                                data_mut.locals.push_value(arg);
                            }

                            for _ in 0..target_chunk.num_locals() {
                                data_mut.locals.push_value(Value::Nil);
                            }

                            data_mut.stack.pop_frame();
                            data_mut.stack.push_frame();
                            upvalues = closure.upvalues();
                            reader = InstructionReader::new(chunk.insns());
                            continue;
                        }

                        data_mut.locals.push_frame();

                        for arg in args {
                            data_mut.locals.push_value(arg);
                        }

                        for _ in 0..target_chunk.num_locals() {
                            data_mut.locals.push_value(Value::Nil);
                        }

                        data_mut.stack.push_frame();

                        let result = run_chunk(
                            data,
                            data_mut,
                            target_chunk,
                            closure.upvalues(),
                            call_depth + 1,
                        )
                        .map_err(program_trace)?;

                        data_mut.locals.pop_frame();
                        data_mut.stack.pop_frame();
                        data_mut.push(result);
                    }

                    _ => {
                        return Err(RuntimeError::NotCallable(target)).map_err(single_trace);
                    }
                }
            }

            Instruction::LoadGlobal(index) => {
                let sym = data
                    .symbols
                    .get(index)
                    .ok_or(RuntimeError::InvalidSymbol(index))
                    .map_err(single_trace)?;

                let cell = data_mut
                    .globals
                    .get_mut(sym)
                    .ok_or_else(|| RuntimeError::UndefinedGlobal(sym.clone()))
                    .map_err(single_trace)?;

                let value = cell.borrow();
                data_mut.stack.push(value.clone());
            }

            Instruction::StoreGlobal(index) => {
                let sym = data
                    .symbols
                    .get(index)
                    .ok_or(RuntimeError::InvalidSymbol(index))
                    .map_err(single_trace)?;

                let value = data_mut.pop_required().map_err(single_trace)?;

                data_mut.globals.insert(sym.clone(), value);
            }

            Instruction::JumpIfFalse(num) => {
                let value = data_mut.pop_required().map_err(single_trace)?;

                if value.falsy() {
                    reader.jump_forward(num);
                }
            }

            Instruction::Jump(num) => {
                reader.jump_forward(num);
            }

            Instruction::Greater => {
                let b = data_mut.pop_required().map_err(single_trace)?;
                let a = data_mut.pop_required().map_err(single_trace)?;

                let cmp_res = a
                    .partial_cmp(&b)
                    .ok_or(RuntimeError::UnorderableValues)
                    .map_err(single_trace)?;

                let res = matches!(cmp_res, Ordering::Greater);
                data_mut.push(Value::new(res));
            }

            Instruction::GreaterOrEqual => {
                let b = data_mut.pop_required().map_err(single_trace)?;
                let a = data_mut.pop_required().map_err(single_trace)?;

                let cmp_res = a
                    .partial_cmp(&b)
                    .ok_or(RuntimeError::UnorderableValues)
                    .map_err(single_trace)?;

                let res = matches!(cmp_res, Ordering::Greater | Ordering::Equal);
                data_mut.push(Value::new(res));
            }

            Instruction::Less => {
                let b = data_mut.pop_required().map_err(single_trace)?;
                let a = data_mut.pop_required().map_err(single_trace)?;

                let cmp_res = a
                    .partial_cmp(&b)
                    .ok_or(RuntimeError::UnorderableValues)
                    .map_err(single_trace)?;

                let res = matches!(cmp_res, Ordering::Less);
                data_mut.push(Value::new(res));
            }

            Instruction::LessOrEqual => {
                let b = data_mut.pop_required().map_err(single_trace)?;
                let a = data_mut.pop_required().map_err(single_trace)?;

                let cmp_res = a
                    .partial_cmp(&b)
                    .ok_or(RuntimeError::UnorderableValues)
                    .map_err(single_trace)?;

                let res = matches!(cmp_res, Ordering::Less | Ordering::Equal);
                data_mut.push(Value::new(res));
            }

            Instruction::Equal => {
                cmp_op(data_mut.stack_mut(), Value::eq).map_err(single_trace)?;
            }

            Instruction::NotEqual => {
                cmp_op(data_mut.stack_mut(), Value::ne).map_err(single_trace)?;
            }

            Instruction::Not => {
                let value = data_mut.stack_mut().pop_required().map_err(single_trace)?;

                data_mut.push(Value::new(!value.truthy()));
            }

            Instruction::LoadLocal(index) => {
                data_mut.load_local(index).map_err(single_trace)?;
            }

            Instruction::StoreLocal(index) => {
                data_mut.store_local(index).map_err(single_trace)?;
            }

            Instruction::LoadClosure(proto_index) => {
                let proto = data
                    .protos
                    .get(proto_index)
                    .ok_or(RuntimeError::InvalidFunction(proto_index))
                    .map_err(single_trace)?;

                let chunk = data
                    .chunks
                    .get(proto.chunk())
                    .ok_or(RuntimeError::InvalidChunk(proto.chunk()))
                    .map_err(single_trace)?;

                let mut cells = Vec::with_capacity(chunk.upvalues().len());

                for upval in chunk.upvalues().iter() {
                    let cell = match upval {
                        UpvalueDescriptor::Local(index) => data_mut
                            .locals
                            .get_mut_required(*index)
                            .map_err(single_trace)?
                            .clone(),

                        UpvalueDescriptor::Upvalue(index) => upvalues
                            .get(*index)
                            .ok_or(RuntimeError::InvalidUpvalue(*index))
                            .map_err(single_trace)?
                            .clone(),
                    };

                    cells.push(cell);
                }

                data_mut.push(Value::new(Closure::new(proto_index, cells.into())));
            }

            Instruction::LoadUpvalue(index) => {
                let cell = upvalues
                    .get(index)
                    .ok_or(RuntimeError::InvalidUpvalue(index))
                    .map_err(single_trace)?;

                data_mut.push(cell.borrow().clone());
            }

            Instruction::StoreUpvalue(index) => {
                let cell = upvalues
                    .get(index)
                    .ok_or(RuntimeError::InvalidUpvalue(index))
                    .map_err(single_trace)?;

                let value = data_mut.pop_required().map_err(single_trace)?;

                *cell.borrow_mut() = value;
            }

            Instruction::MakeVector(num) => {
                let args = data_mut.pop_many_required(num).map_err(single_trace)?;
                let vec: Vec<_> = args.collect();
                data_mut.push(Value::new(Rc::<[Value]>::from(vec)));
            }

            Instruction::MakeList(num) => {
                // XXX TODO use a proper list
                let args = data_mut.pop_many_required(num).map_err(single_trace)?;
                let vec: Vec<_> = args.collect();
                data_mut.push(Value::List(Rc::<[Value]>::from(vec)));
            }

            Instruction::MakeTable(num) => {
                let mut map = HashMap::new();
                let mut args = data_mut.pop_many_required(num).map_err(single_trace)?;

                while let Some(key) = args.next() {
                    let key: ValueKey = key
                        .try_into()
                        .map_err(|_| RuntimeError::InvalidKey)
                        .map_err(single_trace)?; // XXX TODO cleanup

                    let val = args.next().unwrap();
                    map.insert(key, val);
                }

                drop(args);
                data_mut.push(Value::new(Rc::new(map)));
            }

            // TODO collect the enclosing chunk code
            Instruction::Eval => {
                let value = data_mut.pop_required().map_err(single_trace)?;

                let mut builder = data.builder.clone();
                builder.push_chunk_scope();

                let mut program = compile_eval(builder, value)
                    .map_err(RuntimeError::Compile)
                    .map_err(single_trace)?;

                let chunk = program.data.chunks.get(program.data.main_chunk).unwrap();
                program.data_mut.stack = data_mut.stack.clone();
                program.data_mut.locals = data_mut.locals.clone();
                let mut globals = data_mut.globals.clone();

                program.data_mut.locals.push_frame();
                program.data_mut.stack.push_frame();

                for (key, cell) in program.data_mut.globals.iter() {
                    globals.insert_cell(key.clone(), cell.clone());
                }

                for _ in 0..chunk.num_locals() {
                    program.data_mut.locals.push_value(Value::Nil);
                }

                program.data_mut.globals = globals;

                let mut cells = Vec::with_capacity(chunk.upvalues().len());

                // XXX needs dedup refactoring with closure value creation logic
                for upval in chunk.upvalues().iter() {
                    let cell = match upval {
                        UpvalueDescriptor::Local(index) => data_mut
                            .locals
                            .get_mut_required(*index)
                            .map_err(single_trace)?
                            .clone(),

                        UpvalueDescriptor::Upvalue(index) => upvalues
                            .get(*index)
                            .ok_or(RuntimeError::InvalidUpvalue(*index))
                            .map_err(single_trace)?
                            .clone(),
                    };

                    cells.push(cell);
                }

                if std::env::var_os("TRACE_DISASM").is_some() {
                    let disassembly = program.disassemble();
                    println!("==== Subprogram disassembly ====");
                    println!("{disassembly}\n");
                    println!("======================");
                }

                let result = run_chunk(
                    &program.data,
                    &mut program.data_mut,
                    chunk,
                    cells.into(),
                    call_depth + 1,
                )
                .map_err(program_trace)?;

                data_mut.push(result);
            }

            Instruction::LoadKeyword(index) => {
                // TODO keyword table
                let sym = data
                    .keywords
                    .get(index)
                    .ok_or(RuntimeError::InvalidKeyword(index))
                    .map_err(single_trace)?;

                let keyword = Keyword::new(index, sym.clone());
                data_mut.push(Value::new(keyword));
            }

            Instruction::Pop(num) => {
                drop(data_mut.pop_many_required(num).map_err(single_trace)?);
            }
        }

        last_ip = reader.ip();
    }

    Ok(data_mut.pop().unwrap_or(Value::Nil))
}

impl Program {
    fn new(data: ProgramData, data_mut: MutableProgramData) -> Self {
        Self { data, data_mut }
    }

    pub fn disassemble(&self) -> String {
        let mut collect = vec![];

        for (i, chunk) in self.data.chunks.iter().enumerate() {
            let reader = InstructionReader::new(chunk.insns());
            let sub_asm = reader.disassemble(&self.data.consts, &self.data.symbols);

            collect.push(format!("==== CHUNK {i} UPVALUES ===="));

            for val in chunk.upvalues() {
                collect.push(format!("{val:?}"));
            }

            collect.push(format!("==== CHUNK {i} DEBUG RANGES ===="));

            for (start, end, val) in chunk.debug() {
                collect.push(format!("{start}..{end}: {val}"));
            }

            collect.push(format!("==== CHUNK {i} LOCAlS ===="));
            collect.push(format!("{:?}", chunk.num_locals()));
            collect.push(format!("==== CHUNK {i} CODE ===="));
            collect.push(sub_asm);
            collect.push(format!("==== CHUNK {i} END ===="));
        }

        collect.join("\n")
    }

    fn mut_data(&mut self) -> &mut MutableProgramData {
        &mut self.data_mut
    }

    pub fn add_global(&mut self, key: &str, value: Value) {
        self.mut_data()
            .globals_mut()
            .insert(Symbol::new(key), value);
    }

    pub fn add_global_builtin_fn(&mut self, key: &'static str, ptr: BuiltinFnPtr) {
        self.add_global(key, Value::BuiltinFunction(BuiltinFunction::new(key, ptr)));
    }

    pub fn run(&mut self) -> Result<Value, ProgramError> {
        let chunk = self.data.chunks.get(self.data.main_chunk).unwrap();
        self.data_mut.locals.push_frame();
        self.data_mut.stack.push_frame();

        for _ in 0..chunk.num_locals() {
            self.data_mut.locals.push_value(Value::Nil);
        }

        run_chunk(&self.data, &mut self.data_mut, chunk, Rc::from(vec![]), 0)
    }
}

impl UpvalueTracker {
    fn new() -> Self {
        Self { upvalues: vec![] }
    }

    fn finish(self) -> Box<[UpvalueDescriptor]> {
        self.upvalues.into()
    }

    fn insert(&mut self, val: UpvalueDescriptor) -> usize {
        // XXX is this correct?
        let index = self.upvalues.iter().position(|v| *v == val);

        match index {
            Some(index) => index,
            None => {
                let index = self.upvalues.len();
                self.upvalues.push(val);
                index
            }
        }
    }
}

impl DebugTracker {
    fn new() -> Self {
        Self { ranges: vec![] }
    }

    fn add_info(&mut self, start: DebugIP, end: DebugIP, text: Box<str>) {
        self.ranges.push((start, end, text));
    }

    fn finish(self) -> Box<[DebugEntry]> {
        self.ranges.into()
    }
}

impl ChunkBuilder {
    fn new() -> Self {
        Self {
            insns: InstructionBuilder::new(),
            locals: LocalTracker::new(),
            upvalues: UpvalueTracker::new(),
            debug: DebugTracker::new(),
        }
    }

    fn finish(self) -> Chunk {
        Chunk::new(
            self.insns.finish(),
            self.locals.num_locals(),
            self.upvalues.finish(),
            self.debug.finish(),
        )
    }

    fn locals(&self) -> &LocalTracker {
        &self.locals
    }

    fn locals_mut(&mut self) -> &mut LocalTracker {
        &mut self.locals
    }

    fn insns(&self) -> &InstructionBuilder {
        &self.insns
    }

    fn insns_mut(&mut self) -> &mut InstructionBuilder {
        &mut self.insns
    }

    fn upvalues_mut(&mut self) -> &mut UpvalueTracker {
        &mut self.upvalues
    }

    fn debug_mut(&mut self) -> &mut DebugTracker {
        &mut self.debug
    }
}

impl Chunk {
    fn new(
        insns: Box<InstructionStream>,
        num_locals: usize,
        upvalues: Box<[UpvalueDescriptor]>,
        debug: Box<[DebugEntry]>,
    ) -> Self {
        Self {
            insns,
            num_locals,
            upvalues,
            debug,
        }
    }

    fn insns(&self) -> &InstructionStream {
        &self.insns
    }

    fn num_locals(&self) -> usize {
        self.num_locals
    }

    fn upvalues(&self) -> &[UpvalueDescriptor] {
        &self.upvalues
    }

    fn debug(&self) -> &[DebugEntry] {
        &self.debug
    }
}

pub enum SymbolResolution {
    Local(usize),
    Upvalue(UpvalueDescriptor),
    Global,
}

impl ChunkTracker {
    fn new() -> Self {
        Self {
            chunks: vec![ChunkBuilder::new()],
            chunk_stack: vec![0],
        }
    }

    fn current_chunk(&self) -> usize {
        *self.chunk_stack.last().unwrap()
    }

    fn chunk(&self) -> &ChunkBuilder {
        self.chunks.get(self.current_chunk()).unwrap()
    }

    fn chunk_mut(&mut self) -> &mut ChunkBuilder {
        let current_chunk = self.current_chunk();
        self.chunks.get_mut(current_chunk).unwrap()
    }

    fn is_toplevel(&self) -> bool {
        self.chunk_stack.len() == 1
    }

    fn try_capture_upvalue(&mut self, stack_idx: usize, sym: &Symbol) -> Option<UpvalueDescriptor> {
        let next_stack_index = stack_idx.checked_sub(1)?;
        let parent_index = self.chunk_stack[next_stack_index];

        if let Some(slot) = self.chunks[parent_index].locals().get(sym) {
            return Some(UpvalueDescriptor::Local(slot));
        }

        let desc = self.try_capture_upvalue(next_stack_index, sym)?;
        let index = self.chunks[parent_index].upvalues_mut().insert(desc);
        Some(UpvalueDescriptor::Upvalue(index))
    }

    fn resolve_symbol(&mut self, sym: &Symbol) -> SymbolResolution {
        let current = self.current_chunk();

        // Local case, we're in a chunk which defines this symbol
        if let Some(local_index) = self.chunks[current].locals().get(sym) {
            return SymbolResolution::Local(local_index);
        }

        // Variable must be captured from an enclosing scope
        match self.try_capture_upvalue(self.chunk_stack.len() - 1, sym) {
            Some(desc) => SymbolResolution::Upvalue(desc),
            None => SymbolResolution::Global,
        }
    }

    fn push_chunk_scope(&mut self) -> usize {
        self.chunks.push(ChunkBuilder::new());
        let index = self.chunks.len() - 1;
        self.chunk_stack.push(index);
        index
    }

    fn pop_chunk_scope(&mut self) {
        self.chunk_stack.pop();
    }

    fn finish(self) -> Box<[Chunk]> {
        let chunks: Vec<_> = self.chunks.into_iter().map(ChunkBuilder::finish).collect();
        chunks.into()
    }
}

impl ProgramBuilder {
    pub fn new() -> Self {
        Self {
            consts: InternBuilder::new(),
            symbols: InternBuilder::new(),
            keywords: InternBuilder::new(),
            specials: SpecialFormStore::new(),
            chunks: ChunkTracker::new(),
            protos: vec![],
        }
    }

    fn chunk(&self) -> &ChunkBuilder {
        self.chunks.chunk()
    }

    fn chunk_mut(&mut self) -> &mut ChunkBuilder {
        self.chunks.chunk_mut()
    }

    fn locals_mut(&mut self) -> &mut LocalTracker {
        self.chunk_mut().locals_mut()
    }

    fn upvalues_mut(&mut self) -> &mut UpvalueTracker {
        self.chunk_mut().upvalues_mut()
    }

    pub fn add_debug_info(&mut self, start: usize, end: usize, text: Box<str>) {
        let chunk = self.chunk_mut();
        chunk.debug_mut().add_info(start, end, text);
    }

    pub fn push_local_scope(&mut self) {
        self.locals_mut().push_scope();
    }

    pub fn pop_local_scope(&mut self) -> Option<LocalMap> {
        self.locals_mut().pop_scope()
    }

    pub fn insert_local(&mut self, sym: Symbol) -> usize {
        self.locals_mut().insert(sym)
    }

    pub fn is_toplevel(&self) -> bool {
        self.chunks.is_toplevel()
    }

    pub fn resolve_symbol(&mut self, sym: &Symbol) -> SymbolResolution {
        self.chunks.resolve_symbol(sym)
    }

    pub fn insert_upvalue(&mut self, desc: UpvalueDescriptor) -> usize {
        self.upvalues_mut().insert(desc)
    }

    pub fn push_chunk_scope(&mut self) -> usize {
        self.chunks.push_chunk_scope()
    }

    pub fn pop_chunk_scope(&mut self) {
        self.chunks.pop_chunk_scope()
    }

    pub fn register_proto(&mut self, proto: FunctionPrototype) -> usize {
        let index = self.protos.len();
        self.protos.push(proto);
        index
    }

    pub fn add_special_form(&mut self, name: impl Into<Symbol>, target: SpecialFormTarget) {
        self.specials.insert(SpecialForm::new(name, target))
    }

    pub fn get_special_form(&self, key: &Symbol) -> Option<&SpecialForm> {
        self.specials.get(key)
    }

    pub fn emit(&mut self, op: Instruction) {
        let chunk = self.chunk_mut();
        chunk.insns_mut().write(op)
    }

    pub fn position(&self) -> usize {
        let chunk = self.chunk();
        chunk.insns().position()
    }

    // TODO clean up
    pub fn emit_jump(&mut self, insn: Instruction) -> usize {
        let chunk = self.chunk_mut();
        let operand_off = chunk.insns().position() + 1;
        chunk.insns_mut().write(insn);
        operand_off
    }

    pub fn patch_jump(&mut self, operand_off: usize, target: usize) {
        let chunk = self.chunk_mut();
        chunk.insns_mut().patch_jump(operand_off, target)
    }

    pub fn intern_constant(&mut self, constant: Constant) -> usize {
        self.consts.intern(constant)
    }

    pub fn intern_symbol(&mut self, symbol: Symbol) -> usize {
        self.symbols.intern(symbol)
    }

    pub fn intern_keyword(&mut self, keyword: Symbol) -> usize {
        self.keywords.intern(keyword)
    }

    pub fn finish(self) -> Program {
        let cloned = self.clone();
        let main_chunk = self.chunks.chunk_stack.last().cloned().unwrap();

        Program::new(
            ProgramData::new(
                self.consts.finish(),
                self.symbols.finish(),
                self.keywords.finish(),
                self.chunks.finish(),
                self.protos.into(),
                cloned,
                main_chunk,
            ),
            MutableProgramData::new(),
        )
    }
}

impl ProgramData {
    fn new(
        consts: Box<[Constant]>,
        symbols: Box<[Symbol]>,
        keywords: Box<[Symbol]>,
        chunks: Box<[Chunk]>,
        protos: Box<[FunctionPrototype]>,
        builder: ProgramBuilder,
        main_chunk: usize,
    ) -> Self {
        Self {
            consts,
            symbols,
            keywords,
            chunks,
            protos,
            builder,
            main_chunk,
        }
    }
}

impl MutableProgramData {
    fn new() -> Self {
        Self {
            stack: ValueStack::new(),
            locals: LocalStack::new(),
            globals: GlobalStore::new(),
        }
    }

    fn stack_mut(&mut self) -> &mut ValueStack {
        &mut self.stack
    }

    fn globals_mut(&mut self) -> &mut GlobalStore {
        &mut self.globals
    }

    fn push(&mut self, value: Value) {
        self.stack_mut().push(value)
    }

    fn pop(&mut self) -> Option<Value> {
        self.stack_mut().pop()
    }

    fn pop_required(&mut self) -> Result<Value, RuntimeError> {
        self.stack_mut().pop_required()
    }

    fn pop_many_required(&mut self, num: usize) -> Result<vec::Drain<'_, Value>, RuntimeError> {
        self.stack_mut().pop_many_required(num)
    }

    fn store_local(&mut self, index: usize) -> Result<(), RuntimeError> {
        let cell = self.locals.get_mut_required(index)?;
        let value = self.stack.pop_required()?;
        *cell.borrow_mut() = value;
        Ok(())
    }

    fn load_local(&mut self, index: usize) -> Result<(), RuntimeError> {
        let cell = self.locals.get_mut_required(index)?;
        let value = cell.borrow();
        self.stack.push(value.clone()); // XXX
        Ok(())
    }
}
