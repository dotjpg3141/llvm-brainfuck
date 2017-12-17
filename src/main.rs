extern crate tempfile;

#[macro_use]
mod llvm;
mod bf;
mod compiler;

use bf::*;
use compiler::compile;

use tempfile::NamedTempFile;
use std::fs::File;
use std::process::Command;

#[cfg(test)]
mod test;

fn main() {

    let hello_world = "++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++
    				   ..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.";

	let mut insns = InstructionList::from_chars(hello_world.chars());
	//insns.insert_debug_logs();

    let machine = BfMachine {
        cache_size: 64,
        instructions: insns,
        memory_overflow: MemoryOverflowBehaviour::Undefined,
    };

    for item in machine.instructions.list.iter().enumerate() {
        println!("{:?}", item);
    }

    let (module, function_name) = compile(&machine, true);

    println!("\n<dump>");
    module.dump();
    println!("\n</dump>");

    println!("\n<verify module>");
    module.verify();
    println!("\n</verify module>");

    module.optimize(3);

    println!("\n<dump>");
    module.dump();
    println!("\n</dump>");

    //module.jit_function(function_name);
    
    //let mut obj_file = NamedTempFile::new().expect("failed to create temporary object file");
    //let obj_path = obj_file.path().to_str().expect("temporary object file name is not valid utf8");
    let obj_path = "./bf.out";
	
	module.write_object_file(obj_path);    
    
    let target_triple = module.get_target();
    let exec_path = "./bf";
    let result = link_object_file(obj_path, exec_path, target_triple.as_ref().map(String::as_ref));
    
    if let Err(s) = result {
    	panic!(s)
    }
}

fn link_object_file(obj_path: &str, exec_path: &str, target_triple: Option<&str>) -> Result<(), String> {

	let mut arguments = vec![obj_path, "-o", exec_path];	
	if let Some(target_triple) = target_triple {
		arguments.extend_from_slice(&["-target", target_triple]);
	}
	
	println!("clang args: {:?}", arguments);
	
	let status = Command::new("clang")
		.args(arguments)
		.status()
		.map_err(|_| "failed to execute clang")?;
	
	if status.success() {
		Ok(())
	} else {
		match status.code() {
			Some(code) => Err(format!("clang terminated unsuccessfully with code {}.", code)),
			None => Err("clang terminated unsuccessfully".to_owned()),
		}
	}
}

