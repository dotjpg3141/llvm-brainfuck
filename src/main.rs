extern crate num;

#[macro_use]
mod llvm;
mod bf;

use llvm::*;
use llvm::sys::LLVMIntPredicate::*;

use bf::*;
use bf::MemoryOverflowBehaviour::*;

#[cfg(test)]
mod test;

fn main() {

    let hello_world = "++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.";
    //let hello_world = "+++++++++++++++++++++++++++++++++[->+<]>.";

    let machine = BfMachine {
        cache_size: 64,
        instructions: parse_bf(hello_world.chars()),
        memory_overflow: MemoryOverflowBehaviour::Undefined,
        emit_debug_log: false,
    };

    for item in machine.instructions.iter().enumerate() {
        println!("{:?}", item);
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
    let module = Module::new(llvm_str!(b"brainfuck\0"));

    let value_type = module.i8_type;
    let size_type = module.i32_type;
    let ptr_type = value_type.ptr_type();

    let malloc = module.add_function(llvm_str!(b"malloc\0"), &mut [size_type], ptr_type);
    let free = module.add_function(llvm_str!(b"free\0"), &mut [ptr_type], module.void_type);
    let putchar = module.add_function(llvm_str!(b"putchar\0"), &mut [value_type], value_type);
    let getchar = module.add_function(llvm_str!(b"getchar\0"), &mut [], value_type);
    let fflush = module.add_function(llvm_str!(b"fflush\0"), &mut [], module.i32_type);

    let debug_log = if machine.emit_debug_log {
        module.add_function(
            llvm_str!(b"debug_log\0"),
            &mut [module.i32_type, ptr_type, size_type, size_type],
            module.void_type,
        )
    } else {
        Function::null()
    };

    let function = module.add_function(function_name, &mut [], value_type);
    let mut bb = module.append_basic_block(function, llvm_str!(b"entry\0"));
    let mut builder = Builder::new(&module, bb);

    let zero_value = builder.const_signed_int(value_type, 0);

    let cache_size = builder.const_unsigned_int(size_type, machine.cache_size as u64);
    let array = builder.call(malloc, &mut [cache_size], llvm_str!(b"array\0"));

    let index_var = Var::alloc(
        &builder,
        size_type,
        builder.const_unsigned_int(size_type, 0),
        llvm_str!(b"index_var\0"),
    );

    let ptr_var = Var::alloc(
        &builder,
        ptr_type,
        builder.getelementptr(array, index_var.load(&builder), llvm_str!(b"ptr_value\0")),
        llvm_str!(b"ptr_var\0"),
    );

    // NOTE(jpg): clear memory
    {
        let before_bb = bb;
        let entry_bb = function.append_basic_block(llvm_str!(b"clear-loop-cond\0"));
        let body_bb = function.append_basic_block(llvm_str!(b"clear-loop-body\0"));
        let exit_bb = function.append_basic_block(llvm_str!(b"bf-begin\0"));

        // int i = 0; goto entry;
        let counter_before = builder.const_unsigned_int(size_type, 0);
        builder.br(entry_bb);

        // entry: if i != cache_size { goto body; } else { goto exit; }
        builder = Builder::new(&module, entry_bb);
        let counter_entry_phi = builder.phi(size_type, llvm_str!(b"i\0"));
        let counter_entry = counter_entry_phi.value;
        let cmp = builder.icmp(LLVMIntNE, counter_entry, cache_size, llvm_str!(b"cmp\0"));
        builder.cond_br(cmp, body_bb, exit_bb);

        // body: memory[i] = 0; goto entry;
        builder = Builder::new(&module, body_bb);
        let ptr = builder.getelementptr(array, counter_entry, llvm_str!(b"ptr\0"));
        builder.store(zero_value, ptr);
        let counter_body = builder.add(
            counter_entry,
            builder.const_unsigned_int(size_type, 1),
            llvm_str!(b"i\0"),
        );
        builder.br(entry_bb);

        // exit: { ... }
        bb = exit_bb;
        builder = Builder::new(&module, bb);

        //NOTE(jpg): adding phi incomming later
        counter_entry_phi.add_incoming(counter_before, before_bb);
        counter_entry_phi.add_incoming(counter_body, body_bb);
    }

    // NOTE(jpg): emit instructions
    let mut abort_bb = None;
    let mut loop_abort_depth = 0;
    let mut loop_stack = Vec::new();

    // TODO(jpg): rewrite this as lambda expression if possible
    macro_rules! allow_write { () => {{ loop_abort_depth == 0 }} }

    for (i, insn) in machine.instructions.iter().enumerate() {

        if !allow_write!() {
            match *insn {
                BfInstruction::BeginLoop => loop_abort_depth += 1,
                BfInstruction::EndLoop => loop_abort_depth -= 1,
                _ => {} // no op
            }

            if !allow_write!() {
                continue;
            }
        }

        if machine.emit_debug_log {
            let insn_index = builder.const_unsigned_int(module.i32_type, i as u64);
            let index = index_var.load(&builder);
            builder.call_void(debug_log, &mut [insn_index, array, cache_size, index]);
        }

        match *insn {

            BfInstruction::SetValue(value) => {
                let value = builder.const_signed_int(value_type, value as i64);
                builder.store(value, ptr_var.load(&builder));
            }

            BfInstruction::AddValue(value) => {
                let lhs = builder.load(ptr_var.load(&builder), llvm_str!(b"val\0"));
                let rhs = builder.const_signed_int(value_type, value as i64);
                let sum = builder.add(lhs, rhs, llvm_str!(b"sum\0"));
                builder.store(sum, ptr_var.load(&builder));
            }

            BfInstruction::AddPointer(value) => {

                let value = builder.const_signed_int(size_type, value as i64);
                let mut index = index_var.load(&builder);
                index = builder.add(index, value, llvm_str!(b"idx\0"));

                if machine.emit_debug_log {
                    let insn_index = builder.const_unsigned_int(module.i32_type, i as u64);
                    builder.call_void(debug_log, &mut [insn_index, array, cache_size, index]);
                }

                match machine.memory_overflow {
                    Undefined => {} // no op
                    Wrap => index = builder.urem(index, cache_size, llvm_str!(b"idx\0")),
                    Abort => {

                        let success_bb = function.append_basic_block(llvm_str!(b"check_success\0"));
                        if abort_bb == None {
                            abort_bb =
                                Some(function.append_basic_block(llvm_str!(b"check_abort\0")));
                        }

                        let cmp = builder.icmp(LLVMIntULT, index, cache_size, llvm_str!(b"cmp\0"));
                        builder.cond_br(cmp, success_bb, abort_bb.unwrap());

                        bb = success_bb;
                        builder = Builder::new(&module, bb);
                    }
                }

                index_var.store(&builder, index);
                ptr_var.store(
                    &builder,
                    builder.getelementptr(array, index, llvm_str!(b"ptr\0")),
                );
            }

            BfInstruction::Input => {
                let value = builder.call(getchar, &mut [], llvm_str!(b"chr\0"));
                builder.store(value, ptr_var.load(&builder));
            }

            BfInstruction::Output => {
                let out = builder.load(ptr_var.load(&builder), llvm_str!(b"val\0"));
                builder.call_void(putchar, &mut [out]);
            }

            BfInstruction::BeginLoop => {

                let loop_header_bb = function.append_basic_block(llvm_str!(b"loop-header\0"));
                let loop_body_bb = function.append_basic_block(llvm_str!(b"loop-body\0"));
                let loop_footer_bb = function.append_basic_block(llvm_str!(b"loop-footer\0"));

                // goto loop_header;
                builder.br(loop_header_bb);

                // loop_header: if *ptr == 0 { goto loop_footer; } else { goto loop_body; }
                builder = Builder::new(&module, loop_header_bb);
                let value = builder.load(ptr_var.load(&builder), llvm_str!(b"val\0"));
                let cmp = builder.icmp(LLVMIntEQ, value, zero_value, llvm_str!(b"cmp\0"));
                builder.cond_br(cmp, loop_footer_bb, loop_body_bb);

                // loop_body: { /* inside loop */ } goto loop_header;
                bb = loop_body_bb;
                builder = Builder::new(&module, bb);

                // loop_footer: /* after loop */
                loop_stack.push(LoopContext {
                    loop_header_bb,
                    loop_footer_bb,
                });
            }

            BfInstruction::EndLoop => {

                let context = loop_stack.pop().expect(
                    "Could not find machting opening 'BeginLoop' instruction",
                );

                builder.br(context.loop_header_bb);

                bb = context.loop_footer_bb;
                builder = Builder::new(&module, bb);
            }
        }
    }

    if allow_write!() {
        // NOTE(jpg): succsess: free memory and exit
        let result = builder.load(ptr_var.load(&builder), llvm_str!(b"val\0"));
        builder.call_void(free, &mut [array]);
        builder.ret(result);
    }

    if let Some(bb) = abort_bb {
        // NOTE(jpg): abort
        let builder = Builder::new(&module, bb);
        builder.call_void(free, &mut [array]);
        builder.ret(builder.const_signed_int(value_type, -1));
    }

    if machine.emit_debug_log {

        // TODO(jpg): simplify this debug call, maybe by calling an external function
        let mut bb = debug_log.append_basic_block(llvm_str!(b"entry\0"));
        let mut builder = Builder::new(&module, bb);

        let insn_index = debug_log.get_param(0);
        let array = debug_log.get_param(1);
        let cache_size = debug_log.get_param(2);
        let index = debug_log.get_param(3);

        let before_bb = bb;
        let entry_bb = debug_log.append_basic_block(llvm_str!(b"loop-cond\0"));
        let body_bb = debug_log.append_basic_block(llvm_str!(b"loop-body\0"));
        let exit_bb = debug_log.append_basic_block(llvm_str!(b"loop-exit\0"));

        builder.call_void(
            putchar,
            &mut [builder.const_signed_int(value_type, '\n' as i64)],
        );

        emit_print_char(&module, &builder, insn_index, 6, putchar, value_type);
        builder.call_void(
            putchar,
            &mut [builder.const_signed_int(value_type, ' ' as i64)],
        );
        emit_print_char(&module, &builder, index, 6, putchar, value_type);

        // int i = 0; goto entry;
        let counter_before = builder.const_unsigned_int(size_type, 0);
        builder.br(entry_bb);

        // entry: if i != cache_size { goto body; } else { goto exit; }
        builder = Builder::new(&module, entry_bb);
        let counter_entry_phi = builder.phi(size_type, llvm_str!(b"i\0"));
        let counter_entry = counter_entry_phi.value;
        let cmp = builder.icmp(LLVMIntNE, counter_entry, cache_size, llvm_str!(b"cmp\0"));
        builder.cond_br(cmp, body_bb, exit_bb);

        // body: { .. } goto entry;
        builder = Builder::new(&module, body_bb);

        let ptr = builder.getelementptr(array, counter_entry, llvm_str!(b"ptr\0"));
        let val = builder.load(ptr, llvm_str!(b"val\0"));

        builder.call_void(putchar, &mut [val]);
        builder.call_void(
            putchar,
            &mut [builder.const_signed_int(value_type, '|' as i64)],
        );

        let counter_body = builder.add(
            counter_entry,
            builder.const_unsigned_int(size_type, 1),
            llvm_str!(b"i\0"),
        );
        builder.br(entry_bb);

        // exit: { ... }
        bb = exit_bb;
        builder = Builder::new(&module, bb);

        builder.call_void(
            putchar,
            &mut [builder.const_signed_int(value_type, '\n' as i64)],
        );

        builder.ret_void();

        //NOTE(jpg): adding phi incomming later
        counter_entry_phi.add_incoming(counter_before, before_bb);
        counter_entry_phi.add_incoming(counter_body, body_bb);
    }

    (module, function_name)
}

fn emit_print_char(
    module: &Module,
    builder: &Builder,
    value: Value,
    decimal_places: u32,
    putchar: Function,
    putchar_type: Type,
) {
    for decimal_place in (0..decimal_places - 1).rev() {
        let div_value = u64::pow(10, decimal_place);
        let mod_value = 10;
        let zero_value = '0';

        let div_value = builder.const_unsigned_int(module.i32_type, div_value);
        let mod_value = builder.const_unsigned_int(module.i32_type, mod_value);
        let zero_value = builder.const_unsigned_int(module.i32_type, zero_value as u64);

        let name = llvm_str!(b"char\0");
        let chr = value;
        let chr = builder.udiv(chr, div_value, name);
        let chr = builder.urem(chr, mod_value, name);
        let chr = builder.add(chr, zero_value, name);
        let chr = builder.trunc(chr, putchar_type, name);
        builder.call_void(putchar, &mut [chr]);
    }
}

struct LoopContext {
    loop_header_bb: BasicBlock,
    loop_footer_bb: BasicBlock,
}

struct Var {
    value_ptr: Value,
}

impl Var {
    fn alloc(builder: &Builder, tp: Type, value: Value, name: LLVMString) -> Self {
        let value_ptr = builder.alloca(tp, name);
        let result = Var { value_ptr };
        result.store(builder, value);
        return result;
    }

    fn load(&self, builder: &Builder) -> Value {
        builder.load(self.value_ptr, llvm_str!(b"value\0"))
    }

    fn store(&self, builder: &Builder, value: Value) {
        builder.store(value, self.value_ptr);
    }
}

fn modulo<N: num::Num + num::Zero + PartialOrd + Copy>(a: N, m: N) -> N {
    let result = a % m;
    if result < N::zero() {
        result + m
    } else {
        result
    }
}
