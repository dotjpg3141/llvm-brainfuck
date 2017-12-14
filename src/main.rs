#[macro_use]
mod llvm;
mod bf;

use llvm::*;
use bf::*;

#[cfg(test)]
mod test;

fn main() {

    let hello_world = "++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.";

    let machine = BfMachine {
        cache_size: 16,
        instructions: parse_bf(hello_world.chars()),
    };

    for i in machine.instructions.iter() {
        println!("{:?}", i);
    }
    
    let (module, function_name) = compile(&machine);
    
    println!("\n<dump>");
    module.dump();
    println!("\n</dump>");

    println!("\n<verify module>");
    module.verify();
    println!("\n</verify module>");

    module.jit_function(function_name);
}

fn compile(machine: &BfMachine) -> (Module, LLVMString) {
	let function_name = llvm_str!(b"brainfuck\0");
    let module_name = llvm_str!(b"brainfuck\0");

    let module = Module::new(module_name);

    let value_type = module.i8_type;
    let size_type = module.i32_type;
    let ptr_type = value_type.ptr_type();

    let malloc = module.add_function(llvm_str!(b"malloc\0"), &mut [size_type], ptr_type);
    let free = module.add_function(llvm_str!(b"free\0"), &mut [ptr_type], module.void_type);

    let putchar = module.add_function(llvm_str!(b"putchar\0"), &mut [value_type], value_type);
    let getchar = module.add_function(llvm_str!(b"getchar\0"), &mut [], value_type);

    let function = module.add_function(function_name, &mut [], value_type);
    let mut block = module.append_basic_block(function, llvm_str!(b"entry\0"));
    let mut builder = Builder::new(&module, block);

    let zero_value = builder.const_signed_int(value_type, 0);

    let cache_size = builder.const_unsigned_int(size_type, machine.cache_size);
    let array = builder.call(malloc, &mut [cache_size], llvm_str!(b"array\0"));
    let mut ptr = array;
    let mut index = zero_value;

    //TODO(jpg): clear all memory with zeros via loop
    for i in 0..machine.cache_size {
        let index = builder.const_unsigned_int(size_type, i);
        let ptr = builder.getelementptr(array, index, llvm_str!(b"ptr\0"));
        builder.store(zero_value, ptr);
    }

    let mut loop_stack = Vec::new();

    for insn in machine.instructions.iter() {
        match *insn {

            BfInstruction::SetValue(value) => {
                let value = builder.const_signed_int(value_type, value as i64);
                builder.store(value, ptr);
            }

            BfInstruction::AddValue(value) => {
                let lhs = builder.load(ptr, llvm_str!(b"val\0"));
                let rhs = builder.const_signed_int(value_type, value as i64);
                let sum = builder.add(lhs, rhs, llvm_str!(b"sum\0"));
                builder.store(sum, ptr);
            }

            BfInstruction::SetPointer(value) => {
                index = builder.const_signed_int(value_type, value as i64);
                ptr = builder.getelementptr(array, index, llvm_str!(b"ptr\0"));
            }

            BfInstruction::AddPointer(value) => {
                let lhs = index;
                let rhs = builder.const_signed_int(value_type, value as i64);
                let sum = builder.add(lhs, rhs, llvm_str!(b"idx\0"));
                index = sum;
                ptr = builder.getelementptr(array, index, llvm_str!(b"ptr\0"));
            }

            BfInstruction::Input => {
                let value = builder.call(getchar, &mut [], llvm_str!(b"chr\0"));
                builder.store(value, ptr);
            }

            BfInstruction::Output => {
                let out = builder.load(ptr, llvm_str!(b"val\0"));
                builder.call_void(putchar, &mut [out]);
            }

            BfInstruction::BeginLoop => {

                let loop_header = function.append_basic_block(llvm_str!(b"loop-header\0"));
                let loop_body = function.append_basic_block(llvm_str!(b"loop-body\0"));
                let loop_footer = function.append_basic_block(llvm_str!(b"loop-footer\0"));

                let header_builder = Builder::new(&module, loop_header);

                // goto loop_header;
                builder.br(loop_header);

                // loop_header: if *ptr == 0 { goto loop_footer; } else { goto loop_body; }
                let value = header_builder.load(ptr, llvm_str!(b"val\0"));
                let cmp = header_builder.icmp(
                    llvm::sys::LLVMIntPredicate::LLVMIntEQ,
                    value,
                    zero_value,
                    llvm_str!(b"cmp\0"),
                );
                header_builder.cond_br(cmp, loop_footer, loop_body);

                // loop_body: { /* inside loop */ } goto loop_header;
                block = loop_body;
                builder = Builder::new(&module, block);

                // loop_footer: /* after loop */
                loop_stack.push(LoopContext {
                    loop_header,
                    loop_footer,
                });
            }

            BfInstruction::EndLoop => {

                let context = loop_stack.pop().expect(
                    "Could not find machting opening 'BeginLoop' instruction",
                );

                builder.br(context.loop_header);

                block = context.loop_footer;
                builder = Builder::new(&module, block);
            }
        }
    }

    let result = builder.load(ptr, llvm_str!(b"val\0"));
    builder.call_void(free, &mut [array]);
    builder.ret(result);
    
    (module, function_name)
}

struct LoopContext {
    loop_header: llvm::sys::prelude::LLVMBasicBlockRef,
    loop_footer: llvm::sys::prelude::LLVMBasicBlockRef,
}
