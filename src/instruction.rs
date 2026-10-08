use crate::constant::Constant;
use crate::symbol::Symbol;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::mem::size_of;

// XXX TODO make this a tuple struct?
pub type InstructionByte = u8;
pub type InstructionStream = [InstructionByte];

// XXX usize is pretty wasteful here, come up with some smaller alternative
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instruction {
    LoadNil,
    LoadFalse,
    LoadTrue,
    LoadFloat(f64),
    LoadSmallInt(i64),
    LoadConstant(usize),     // index of interned constant
    Add(usize),              // number of values to add
    Sub(usize),              // number of values to substract
    Mul(usize),              // number of values to multiply
    Div(usize),              // number of values to divide
    Neg,                     // unary negation
    Recip,                   // unary reciprocial
    Call(usize),             // number of arguments
    PossibleTailCall(usize), // number of arguments
    LoadGlobal(usize),       // index of symbol
    StoreGlobal(usize),      // index of symbol
    JumpIfFalse(usize),
    Jump(usize),
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Equal,
    NotEqual,
    Not,
    LoadLocal(usize),   // offset for the current frame
    StoreLocal(usize),  // offset for the current frame
    LoadClosure(usize), // index in the global function prototype table
    LoadUpvalue(usize),
    StoreUpvalue(usize),
    MakeVector(usize), // number of elements in vector
    MakeTable(usize),  // number of elements in table
    MakeList(usize),   // number of elements in list
    Eval,
    LoadKeyword(usize), // index in the global keywords table
    Pop(usize),         // number of entries to pop off the ValueStack
    Return,
}

#[derive(Clone)]
pub struct InstructionReader<'a> {
    bytes: &'a InstructionStream,
    ip: usize,
}

#[derive(Clone)]
pub struct InstructionBuilder {
    bytes: Vec<InstructionByte>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
enum Opcode {
    LoadNil = 0,
    LoadFalse = 1,
    LoadTrue = 2,
    LoadFloat = 3,
    LoadSmallInt = 4,
    LoadConstant = 5,
    Add = 6,
    Sub = 7,
    Mul = 8,
    Div = 9,
    Neg = 10,
    Recip = 11,
    Call = 12,
    PossibleTailCall = 13,
    LoadGlobal = 14,
    StoreGlobal = 15,
    JumpIfFalse = 16,
    Jump = 17,
    Greater = 18,
    GreaterOrEqual = 19,
    Less = 20,
    LessOrEqual = 21,
    Equal = 22,
    NotEqual = 23,
    Not = 24,
    LoadLocal = 25,
    StoreLocal = 26,
    LoadClosure = 27,
    LoadUpvalue = 28,
    StoreUpvalue = 29,
    MakeVector = 30,
    MakeTable = 31,
    MakeList = 32,
    Eval = 33,
    LoadKeyword = 34,
    Pop = 35,
    Return = 36,
}

// XXX it would be better to use generics if it was possible
// you seemingly can't have a size_of::<T>() sized buffer
macro_rules! define_read {
    ($fn_name:ident, $type:ty) => {
        fn $fn_name(&mut self) -> Option<$type> {
            let bytes = self.read_bytes(size_of::<$type>())?;
            let buffer: [InstructionByte; size_of::<$type>()] = bytes.try_into().ok()?;
            Some(<$type>::from_le_bytes(buffer))
        }
    };
}

macro_rules! define_write {
    ($fn_name:ident, $type:ty) => {
        fn $fn_name(&mut self, value: $type) {
            let buf = value.to_le_bytes();
            self.bytes.extend_from_slice(&buf);
        }
    };
}

impl<'a> InstructionReader<'a> {
    pub fn new(bytes: &'a InstructionStream, ip: usize) -> Self {
        Self { bytes, ip }
    }

    pub fn ip(&self) -> usize {
        self.ip
    }

    // TODO backward jumps
    pub fn jump_forward(&mut self, num: usize) {
        self.ip = self.ip.saturating_add(num)
    }

    pub fn disassemble(&self, consts: &[Constant], symbols: &[Symbol]) -> String {
        let copy = self.clone(); // XXX
        let mut disassembly = vec![];
        disassembly.push(format!("IP: {}", copy.ip));

        for insn in copy {
            disassembly.push(insn.disassemble(consts, symbols));
        }

        disassembly.join("\n")
    }

    fn read_byte(&mut self) -> Option<InstructionByte> {
        // XXX bounds check in hot loop
        let new_ip = self.ip.checked_add(1)?;
        let byte = self.bytes.get(self.ip).cloned();
        self.ip = new_ip;
        byte
    }

    fn read_bytes(&mut self, num: usize) -> Option<&[InstructionByte]> {
        // XXX bounds check in hot loop
        let new_ip = self.ip.checked_add(num)?;
        let slice = self.bytes.get(self.ip..new_ip);
        self.ip = new_ip;
        slice
    }

    // XXX TODO invalid opcodes should be an error
    fn read_opcode(&mut self) -> Option<Opcode> {
        Opcode::from_u8(self.read_byte()?)
    }

    define_read!(read_f64, f64);
    define_read!(read_i64, i64);
    define_read!(read_usize, usize);
}

impl Iterator for InstructionReader<'_> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_opcode()? {
            Opcode::LoadNil => Some(Instruction::LoadNil),
            Opcode::LoadFalse => Some(Instruction::LoadFalse),
            Opcode::LoadTrue => Some(Instruction::LoadTrue),
            Opcode::LoadFloat => Some(Instruction::LoadFloat(self.read_f64()?)),
            Opcode::LoadSmallInt => Some(Instruction::LoadSmallInt(self.read_i64()?)),
            Opcode::LoadConstant => Some(Instruction::LoadConstant(self.read_usize()?)),

            Opcode::Add => {
                let num = self.read_usize()?;
                Some(Instruction::Add(num))
            }

            Opcode::Sub => {
                let num = self.read_usize()?;
                Some(Instruction::Sub(num))
            }

            Opcode::Mul => {
                let num = self.read_usize()?;
                Some(Instruction::Mul(num))
            }

            Opcode::Div => {
                let num = self.read_usize()?;
                Some(Instruction::Div(num))
            }

            Opcode::Neg => Some(Instruction::Neg),
            Opcode::Recip => Some(Instruction::Recip),

            Opcode::Call => Some(Instruction::Call(self.read_usize()?)),

            Opcode::PossibleTailCall => Some(Instruction::PossibleTailCall(self.read_usize()?)),

            Opcode::LoadGlobal => Some(Instruction::LoadGlobal(self.read_usize()?)),
            Opcode::StoreGlobal => Some(Instruction::StoreGlobal(self.read_usize()?)),

            Opcode::JumpIfFalse => {
                let num = self.read_usize()?;
                Some(Instruction::JumpIfFalse(num))
            }

            Opcode::Jump => {
                let num = self.read_usize()?;
                Some(Instruction::Jump(num))
            }

            Opcode::Greater => Some(Instruction::Greater),
            Opcode::GreaterOrEqual => Some(Instruction::GreaterOrEqual),
            Opcode::Less => Some(Instruction::Less),
            Opcode::LessOrEqual => Some(Instruction::LessOrEqual),
            Opcode::Equal => Some(Instruction::Equal),
            Opcode::NotEqual => Some(Instruction::NotEqual),
            Opcode::Not => Some(Instruction::Not),

            Opcode::LoadLocal => Some(Instruction::LoadLocal(self.read_usize()?)),
            Opcode::StoreLocal => Some(Instruction::StoreLocal(self.read_usize()?)),

            Opcode::LoadClosure => Some(Instruction::LoadClosure(self.read_usize()?)),

            Opcode::LoadUpvalue => Some(Instruction::LoadUpvalue(self.read_usize()?)),
            Opcode::StoreUpvalue => Some(Instruction::StoreUpvalue(self.read_usize()?)),

            Opcode::MakeVector => Some(Instruction::MakeVector(self.read_usize()?)),
            Opcode::MakeTable => Some(Instruction::MakeTable(self.read_usize()?)),
            Opcode::MakeList => Some(Instruction::MakeList(self.read_usize()?)),

            Opcode::Eval => Some(Instruction::Eval),
            Opcode::LoadKeyword => Some(Instruction::LoadKeyword(self.read_usize()?)),
            Opcode::Pop => Some(Instruction::Pop(self.read_usize()?)),

            Opcode::Return => Some(Instruction::Return),
        }
    }
}

impl InstructionBuilder {
    pub fn new() -> Self {
        Self { bytes: vec![] }
    }

    fn write_opcode(&mut self, op: Opcode) {
        self.bytes.push(op as u8);
    }

    define_write!(write_f64, f64);
    define_write!(write_i64, i64);
    define_write!(write_usize, usize);

    pub fn write(&mut self, insn: Instruction) {
        match insn {
            Instruction::LoadNil => self.write_opcode(Opcode::LoadNil),
            Instruction::LoadFalse => self.write_opcode(Opcode::LoadFalse),
            Instruction::LoadTrue => self.write_opcode(Opcode::LoadTrue),
            Instruction::LoadFloat(x) => {
                self.write_opcode(Opcode::LoadFloat);
                self.write_f64(x);
            }

            Instruction::LoadSmallInt(x) => {
                self.write_opcode(Opcode::LoadSmallInt);
                self.write_i64(x);
            }

            Instruction::LoadConstant(x) => {
                self.write_opcode(Opcode::LoadConstant);
                self.write_usize(x);
            }

            Instruction::Add(num) => {
                self.write_opcode(Opcode::Add);
                self.write_usize(num);
            }

            Instruction::Sub(num) => {
                self.write_opcode(Opcode::Sub);
                self.write_usize(num);
            }

            Instruction::Mul(num) => {
                self.write_opcode(Opcode::Mul);
                self.write_usize(num);
            }

            Instruction::Div(num) => {
                self.write_opcode(Opcode::Div);
                self.write_usize(num);
            }

            Instruction::Neg => self.write_opcode(Opcode::Neg),
            Instruction::Recip => self.write_opcode(Opcode::Recip),

            Instruction::Call(num) => {
                self.write_opcode(Opcode::Call);
                self.write_usize(num);
            }

            Instruction::PossibleTailCall(num) => {
                self.write_opcode(Opcode::PossibleTailCall);
                self.write_usize(num);
            }

            Instruction::LoadGlobal(x) => {
                self.write_opcode(Opcode::LoadGlobal);
                self.write_usize(x);
            }

            Instruction::StoreGlobal(x) => {
                self.write_opcode(Opcode::StoreGlobal);
                self.write_usize(x);
            }

            Instruction::JumpIfFalse(num) => {
                self.write_opcode(Opcode::JumpIfFalse);
                self.write_usize(num);
            }

            Instruction::Jump(num) => {
                self.write_opcode(Opcode::Jump);
                self.write_usize(num);
            }

            Instruction::Greater => self.write_opcode(Opcode::Greater),
            Instruction::GreaterOrEqual => self.write_opcode(Opcode::GreaterOrEqual),
            Instruction::Less => self.write_opcode(Opcode::Less),
            Instruction::LessOrEqual => self.write_opcode(Opcode::LessOrEqual),
            Instruction::Equal => self.write_opcode(Opcode::Equal),
            Instruction::NotEqual => self.write_opcode(Opcode::NotEqual),
            Instruction::Not => self.write_opcode(Opcode::Not),

            Instruction::LoadLocal(x) => {
                self.write_opcode(Opcode::LoadLocal);
                self.write_usize(x);
            }

            Instruction::StoreLocal(x) => {
                self.write_opcode(Opcode::StoreLocal);
                self.write_usize(x);
            }

            Instruction::LoadClosure(x) => {
                self.write_opcode(Opcode::LoadClosure);
                self.write_usize(x);
            }

            Instruction::LoadUpvalue(x) => {
                self.write_opcode(Opcode::LoadUpvalue);
                self.write_usize(x);
            }

            Instruction::StoreUpvalue(x) => {
                self.write_opcode(Opcode::StoreUpvalue);
                self.write_usize(x);
            }

            Instruction::MakeVector(x) => {
                self.write_opcode(Opcode::MakeVector);
                self.write_usize(x);
            }

            Instruction::MakeTable(x) => {
                self.write_opcode(Opcode::MakeTable);
                self.write_usize(x);
            }

            Instruction::MakeList(x) => {
                self.write_opcode(Opcode::MakeList);
                self.write_usize(x);
            }

            Instruction::Eval => self.write_opcode(Opcode::Eval),

            Instruction::LoadKeyword(x) => {
                self.write_opcode(Opcode::LoadKeyword);
                self.write_usize(x);
            }

            Instruction::Pop(x) => {
                self.write_opcode(Opcode::Pop);
                self.write_usize(x);
            }

            Instruction::Return => self.write_opcode(Opcode::Return),
        }
    }

    pub fn position(&self) -> usize {
        self.bytes.len()
    }

    // TODO clean up
    pub fn patch_jump(&mut self, operand_off: usize, target: usize) {
        let delta = target - operand_off - size_of::<usize>();
        let buf = delta.to_le_bytes();
        self.bytes[operand_off..operand_off + size_of::<usize>()].copy_from_slice(&buf);
    }

    pub fn finish(self) -> Box<InstructionStream> {
        self.bytes.into()
    }
}

impl Instruction {
    pub fn disassemble(&self, consts: &[Constant], symbols: &[Symbol]) -> String {
        match &self {
            item @ Instruction::LoadConstant(x) => {
                let constant = consts.get(*x);
                format!("{item:?} ; {constant:?}")
            }

            item @ Instruction::LoadGlobal(x) => {
                let symbol = symbols.get(*x);
                format!("{item:?} ; {symbol:?}")
            }

            item @ Instruction::StoreGlobal(x) => {
                let symbol = symbols.get(*x);
                format!("{item:?} ; {symbol:?}")
            }

            item @ Instruction::LoadClosure(_x) => {
                // XXX TODO print closure info here
                format!("{item:?}")
            }

            default => format!("{default:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_builder() {
        // TODO rewrite parser tests to use a helper function like this
        fn run_test(insns: &[Instruction]) {
            let mut builder = InstructionBuilder::new();

            for &insn in insns {
                builder.write(insn);
            }

            let bytes = builder.finish();
            let reader = InstructionReader::new(&bytes, 0);
            let result = Vec::from_iter(reader);

            for (x, y) in insns.iter().zip(result.iter()) {
                assert_eq!(x, y);
            }
        }

        run_test(&[]);
        run_test(&[Instruction::LoadFalse, Instruction::LoadTrue]);

        run_test(&[
            Instruction::LoadFloat(0.0),
            Instruction::LoadFloat(1.1),
            Instruction::Add(2),
        ]);

        run_test(&[
            Instruction::LoadNil,
            Instruction::LoadFalse,
            Instruction::LoadTrue,
            Instruction::LoadFloat(0.0),
            Instruction::LoadSmallInt(0),
            Instruction::LoadConstant(0),
            Instruction::Add(1),
            Instruction::Sub(1),
            Instruction::Mul(1),
            Instruction::Div(1),
            Instruction::Neg,
            Instruction::Recip,
            Instruction::Call(2),
            Instruction::PossibleTailCall(1),
            Instruction::LoadGlobal(0),
            Instruction::StoreGlobal(0),
            Instruction::JumpIfFalse(0),
            Instruction::Jump(0),
            Instruction::Greater,
            Instruction::GreaterOrEqual,
            Instruction::Less,
            Instruction::LessOrEqual,
            Instruction::Equal,
            Instruction::NotEqual,
            Instruction::Not,
            Instruction::LoadLocal(0),
            Instruction::StoreLocal(0),
            Instruction::LoadClosure(0),
            Instruction::LoadUpvalue(0),
            Instruction::StoreUpvalue(0),
            Instruction::MakeVector(0),
            Instruction::MakeTable(0),
            Instruction::MakeList(0),
            Instruction::Eval,
            Instruction::Pop(1),
            Instruction::Return,
        ]);
    }
}
