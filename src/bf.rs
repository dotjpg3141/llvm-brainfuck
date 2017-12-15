use std::str::Chars;
use self::BfInstruction::*;

pub fn parse_bf(chars: Chars) -> Vec<BfInstruction> {

    let mut result = InstructionList::new();
    for c in chars.fuse() {
        let insn = match c {
            '-' => Some(AddValue(-1)),
            '+' => Some(AddValue(1)),
            '<' => Some(AddPointer(-1)),
            '>' => Some(AddPointer(1)),
            ',' => Some(Input),
            '.' => Some(Output),
            '[' => Some(BeginLoop),
            ']' => Some(EndLoop),
            _ => None,
        };
        if let Some(insn) = insn {
            result.push(insn);
        }
    }
    return result.list;
}

pub fn optimize(input: Vec<BfInstruction>) -> Vec<BfInstruction> {
    let mut result = InstructionList::new();
    for insn in input {
        result.push(insn);
    }
    return result.list;
}

struct InstructionList {
    list: Vec<BfInstruction>,
    loop_comment_depth: u32,
}

impl InstructionList {
    fn new() -> Self {
        InstructionList {
            list: Vec::new(),
            loop_comment_depth: 0,
        }
    }

    fn push(&mut self, insn: BfInstruction) {

        if self.loop_comment_depth != 0 {
            match insn {
                BeginLoop => self.loop_comment_depth += 1,
                EndLoop => self.loop_comment_depth -= 1,
                _ => {}
            }
            return;
        }

        match (self.list.last(), insn) {

            // value += 0; => <empty>
            (_, AddValue(0)) => {
                // skip instruction
            }

            // value += a; value += b; => value += a + b;
            (Some(&AddValue(value)), AddValue(other)) => {
                self.list.pop();
                self.push(AddValue(value + other));
            }

            // value = a; value += b; => value = a + b;
            (Some(&SetValue(value)), AddValue(other)) => {
                self.list.pop();
                self.push(SetValue(value + other));
            }

            // value  = a; value = b; => value = b;
            // value += a; value = b; => value = b;
            (Some(&SetValue(_)), SetValue(_)) |
            (Some(&AddValue(_)), SetValue(_)) => {
                self.list.pop();
                self.push(insn);
            }

            // ptr += a; ptr += b; => ptr += a + b;
            (Some(&AddPointer(value)), AddPointer(other)) => {
                self.list.pop();
                self.push(AddPointer(value + other));
            }

            // while(value) value--; => value = 0;
            (Some(&AddValue(value)), EndLoop)
                if value % 2 != 0 && self.list.get(self.list.len() - 2) == Some(&BeginLoop) => {
                self.list.pop();
                self.list.pop();
                self.push(SetValue(0));
            }

            // value = 0;           while(value) { ... } => value = 0;
            // while(a) { stmt(); } while(a)     { ... } => while (a) { stmt(); }
            (Some(&SetValue(0)), BeginLoop) |
            (Some(&EndLoop), BeginLoop) => {
                self.loop_comment_depth += 1;
            }

            _ => self.list.push(insn),
        }
    }
}

pub struct BfMachine {
    pub cache_size: i64,
    pub instructions: Vec<BfInstruction>,
    pub memory_overflow: MemoryOverflowBehaviour,
    pub emit_debug_log: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BfInstruction {
    SetValue(i8),
    AddValue(i8),
    AddPointer(i64),
    Input,
    Output,
    BeginLoop,
    EndLoop,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MemoryOverflowBehaviour {
    Undefined,
    Wrap,
    Abort,
}
