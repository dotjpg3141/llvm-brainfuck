#[macro_use]
mod llvm;
mod bf;
mod compiler;

use bf::*;
use compiler::compile;

extern crate tempfile;
use tempfile::NamedTempFile;

extern crate argparse;
use argparse::{ArgumentParser, StoreTrue, Store};

use std::fs::*;
use std::process::Command;
use std::io::{self, Read, Write, BufRead, BufReader, BufWriter};
use std::str::FromStr;

#[cfg(test)]
mod test;

struct Config {
	verbose: bool,
	input: String,
	output: String,
	force_binary_stdout: bool,
	output_format: OutputFormat,
	emit_debug: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    BrainfuckIR,
    LlvmIRUnoptimized,
    LlvmIR,
    ObjectFile,
    ExecutableFile,
    Run,
}

impl OutputFormat {
	fn is_binary(self) -> bool {
		self == OutputFormat::ObjectFile || self == OutputFormat::ExecutableFile
	}
}

impl FromStr for OutputFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "bf-ir" => OutputFormat::BrainfuckIR,
            "llvm-ir-unop" => OutputFormat::LlvmIRUnoptimized,
            "llvm-ir" => OutputFormat::LlvmIR,
            "obj" => OutputFormat::ObjectFile,
            "exec" => OutputFormat::ExecutableFile,
            "run" => OutputFormat::Run,
            _ => return Err(()),
        })
    }
}

fn main() {
	let cfg = parse_config_or_exit();
	if let Err(message) = run(cfg) {
		eprintln!("{}", message);
		std::process::exit(4);
	}
}

fn run(cfg: Config) -> Result<(), String> {

	let input = read_input(cfg.input);
	let mut output = create_output_writer(&cfg.output);

    let machine = create_bf_machine(input, cfg.emit_debug);
    
    if cfg.output_format == OutputFormat::BrainfuckIR {
    	for item in machine.instructions.list.iter().enumerate() {
		    output.write_fmt(format_args!("{:?}\n", item)).unwrap();
		}
	    return Ok(());
    }

    let (module, function_name) = compile(&machine, true);

	if cfg.output_format == OutputFormat::LlvmIRUnoptimized {
		module.dump(); // TODO(jpg): write this to output writer
		return Ok(());
	}

    module.verify(); // TODO(jpg): print error and exit
    module.optimize(3);

	if cfg.output_format == OutputFormat::LlvmIR {
		module.dump(); // TODO(jpg): write this to output writer
		return Ok(());
	}

	if cfg.output_format == OutputFormat::Run {
    	module.jit_function(function_name);
    	return Ok(());
	}

    let obj_file = NamedTempFile::new().expect(
    	"failed to create temporary object file");
    	
    let obj_path = obj_file.path().to_str().expect(
        "temporary object file name is not valid utf8",
    );

    module.write_object_file(obj_path);
    
	if cfg.output_format == OutputFormat::ObjectFile {
		 // TODO(jpg): write this to output writer
		 return Ok(());
	}

    let target_triple = module.get_target().expect("failed determine target triple");
    let exec_path = if cfg.output == "" { "./bf" } else { cfg.output.as_str() }; // TODO: use output writer instead
    let result = link_object_file(obj_path, exec_path, target_triple.as_str())?;

    if let Err(_) = remove_file(obj_file.path()) {
        // TODO(jpg): warning, object file could not be removed
    }

    if cfg.output_format == OutputFormat::ExecutableFile {
		 // TODO(jpg): write this to output writer
		 return Ok(());
	}
	
	panic!("Unexpected program state");
}

fn parse_config_or_exit() -> Config {

	let mut cfg = Config {
		verbose: false,
		input: "".to_owned(),
		output: "".to_owned(),
		force_binary_stdout: false,
		output_format: OutputFormat::ExecutableFile,
		emit_debug: false,
	};

    {
        let mut parser = ArgumentParser::new();
        parser.set_description("Brainfuck compiler");
        parser.refer(&mut cfg.verbose).add_option(
            &["-v", "--verbose"],
            StoreTrue,
            "Verbose output",
        );
        parser.refer(&mut cfg.input).add_option(
            &["-i", "--input"],
            Store,
            "Input file; stdin if not set or empty.",
        );
        parser.refer(&mut cfg.output).add_option(
            &["-o", "--output"],
            Store,
            "Output file; stdout if not set or empty.",
        );
        parser.refer(&mut cfg.force_binary_stdout).add_option(
            &["-f", "--force-output"],
            StoreTrue,
            "Force binary output to stdout",
        );
        parser.refer(&mut cfg.output_format).add_option(
            &["-t", "--format"],
            Store,
            "Choose output format: 
				bf-ir (optimized brainfuck IR),
				llvm-ir-unop (unoptimized LLVM IR),
				llvm-ir (optimized LLVM IR),
				obj (object file),
				exec (default; executable file),
				run (compiles and executes the given source)",
        );
        parser.refer(&mut cfg.emit_debug).add_option(
        	&["-d", "--debug"], 
        	Store, 
        	"Emit debug logs");

        parser.parse_args_or_exit();
    }
    
    if !cfg.force_binary_stdout && cfg.output_format.is_binary() && cfg.output == "" {
		eprintln!("Writing binary to stdout is disabled, force with flag'-f'.");
		std::process::exit(1);
    }
    
    cfg
}

fn create_bf_machine(source: String, emit_debug: bool) -> BfMachine {

	let mut insns = InstructionList::from_chars(source.chars());
    if emit_debug {
		insns.insert_debug_logs();
    }

	BfMachine {
        cache_size: 64,
        instructions: insns,
        memory_overflow: MemoryOverflowBehaviour::Undefined, // TODO(jpg): make this an command line argument
    }
}

fn read_input(input_file_option: String) -> String {
    let input: Box<Read> = if input_file_option == "" {
    	let stdin = io::stdin();
    	// TODO(jpg): stdin.lock() ???
    	Box::new(stdin)
    } else {
    	let input_file = File::open(input_file_option).expect("Could not open input file");
    	Box::new(input_file)
    };
    
    let mut input = BufReader::new(input);
    read_text(&mut input)
}

fn create_output_writer(output_file_option: &String) -> BufWriter<Box<Write>> {

	let output: Box<Write> = if output_file_option == "" {
    	let stdout = io::stdout();
    	Box::new(stdout)
    } else {
    	let output_file = File::create(output_file_option).expect("Could not create output file");
    	Box::new(output_file)
    };
    
    BufWriter::new(output)
}

fn read_text<R: BufRead + ?Sized>(r: &mut R) -> String {
	let error_msg = "error while reading";
	let mut buff = Vec::new();
	let mut result = String::new();
	
	while r.read_until(b'\n', &mut buff).expect(error_msg) != 0 {
		let s = String::from_utf8(buff).expect(error_msg);
		result.push_str(s.as_str());
		buff = s.into_bytes();
		buff.clear();
	}
	
	return result;
}

fn link_object_file(obj_path: &str, exec_path: &str, target_triple: &str) -> Result<(), String> {

    let arguments = vec![obj_path, "-o", exec_path, "-target", target_triple];
    println!("clang args: {:?}", arguments);

    let status = Command::new("clang").args(arguments).status().map_err(
        |_| "failed to execute clang",
    )?;

    if status.success() {
        Ok(())
    } else {
        match status.code() {
            Some(code) => Err(format!(
                "clang terminated unsuccessfully with code {}.",
                code
            )),
            None => Err("clang terminated unsuccessfully".to_owned()),
        }
    }
}
