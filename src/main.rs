extern crate tempfile;
extern crate argparse;

#[macro_use]
mod llvm;
mod bf;
mod compiler;

#[cfg(test)]
mod test;

use std::fs;
use std::io::{self, Write};

use tempfile::NamedTempFile;
use argparse::{ArgumentParser, StoreTrue, Store};

use bf::{InstructionList, MemoryOverflowBehaviour, BfMachine};
use compiler::compile;

struct Config {
    verbose: bool,
    input: String,
    output: String,
    force_binary_stdout: bool,
    output_format: OutputFormat,
    emit_debug: bool,
    memory_check: MemoryOverflowBehaviour,
    memory_size: i64,
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

pub struct ParseEnumError {}
macro_rules! derive_FromStr {
	($enum:path, {$( $enum_value:path: $str_val:expr ),*}) => {
		impl std::str::FromStr for $enum {
			type Err = ParseEnumError;
			fn from_str(s: &str) -> Result<Self, Self::Err> {
				match s {
					$(
						$str_val => Ok($enum_value),
					)*
					_ => Err( ParseEnumError {} ),
				}
			}
		}
	}
}

derive_FromStr!(OutputFormat, {
	OutputFormat::BrainfuckIR: "bf-ir",
    OutputFormat::LlvmIRUnoptimized: "llvm-ir-unop",
    OutputFormat::LlvmIR: "llvm-ir",
    OutputFormat::ObjectFile: "obj",
    OutputFormat::ExecutableFile: "exec",
    OutputFormat::Run: "run"
});

derive_FromStr!(MemoryOverflowBehaviour, {
	MemoryOverflowBehaviour::Undefined: "undefined",
	MemoryOverflowBehaviour::Wrap: "wrap",
	MemoryOverflowBehaviour::Abort: "abort"
});

fn main() {
    let cfg = parse_config_or_exit();
    let result = run(cfg);
    let code = match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{}", message);
            4
        }
    };
    std::process::exit(code);
}

fn run(cfg: Config) -> Result<i32, String> {

    let input = read_input(cfg.input.as_str());
    let mut output = create_output_writer(&cfg.output);

    let machine = create_bf_machine(input, &cfg);

    if cfg.output_format == OutputFormat::BrainfuckIR {
        for item in machine.instructions.list.iter().enumerate() {
            output.write_fmt(format_args!("{:?}\n", item)).unwrap();
        }
        return Ok(0);
    }

    let (module, function_name) = compile(&machine, true);

    if cfg.output_format == OutputFormat::LlvmIRUnoptimized {
        module.dump(); // TODO(jpg): write this to output writer
        return Ok(0);
    }

    module.verify(); // TODO(jpg): print error and exit
    module.optimize(3);

    if cfg.output_format == OutputFormat::LlvmIR {
        module.dump(); // TODO(jpg): write this to output writer
        return Ok(0);
    }

    if cfg.output_format == OutputFormat::Run {
        let result: i32 = module.jit_function(function_name);

        return if result != -1 {
            Ok(result)
        } else {
            Err("Error encountered during execution. Aborting!".to_owned())
        };
    }

    let obj_file = NamedTempFile::new().map_err(|_| {
        "failed to create temporary object file".to_owned()
    })?;

    let obj_path = obj_file.path().to_str().ok_or(
        "temporary object file name is not valid utf8"
            .to_owned(),
    )?;

    module.write_object_file(obj_path)?;

    if cfg.output_format == OutputFormat::ObjectFile {
        // TODO(jpg): write this to output writer
        return Ok(0);
    }

    let target_triple = module.get_target().ok_or(
        "failed determine target triple"
            .to_owned(),
    )?;

    let exec_path = if cfg.output == "" {
        "./bf"
    } else {
        cfg.output.as_str()
    }; // TODO: use output writer instead

    link_object_file(obj_path, exec_path, target_triple.as_str())?;

    if let Err(_) = fs::remove_file(obj_file.path()) {
        // TODO(jpg): warning, object file could not be removed
    }

    if cfg.output_format == OutputFormat::ExecutableFile {
        // TODO(jpg): write this to output writer
        return Ok(0);
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
        memory_check: MemoryOverflowBehaviour::Undefined,
        memory_size: 4096,
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
            "Emit debug calls",
        );
        parser.refer(&mut cfg.memory_check).add_option(
            &["-m", "--mem-check"],
            Store,
            "Memory check:
        		undefined (default, no memory check is performed),
        		abort (program aborts on invalid memory access),
        		wrap (memory pointer wraps on invalid memory access)",
        );
        parser.refer(&mut cfg.memory_size).add_option(
            &["-s", "--mem-size"],
            Store,
            "Initial memory size. Default: 4096",
        );

        parser.parse_args_or_exit();
    }

    if !cfg.force_binary_stdout && cfg.output_format.is_binary() && cfg.output == "" {
        eprintln!("Writing binary to stdout is disabled, force with flag'-f'.");
        std::process::exit(1);
    }

    if cfg.memory_size < 1 {
        eprintln!("Invalid memory size. Must be positive");
        std::process::exit(1);
    }

    cfg
}

fn create_bf_machine(source: String, cfg: &Config) -> BfMachine {

    let mut insns = InstructionList::from_chars(source.chars());
    if cfg.emit_debug {
        insns.insert_debug_logs();
    }

    BfMachine {
        cache_size: cfg.memory_size,
        instructions: insns,
        memory_overflow: cfg.memory_check,
    }
}

fn read_input(input_file_option: &str) -> String {
    let input: Box<io::Read> = if input_file_option == "" {
        let stdin = io::stdin();
        // TODO(jpg): stdin.lock() ???
        Box::new(stdin)
    } else {
        let input_file = fs::File::open(input_file_option).expect("Could not open input file");
        Box::new(input_file)
    };

    let mut input = io::BufReader::new(input);
    read_text(&mut input)
}

fn create_output_writer(output_file_option: &String) -> io::BufWriter<Box<io::Write>> {

    let output: Box<io::Write> = if output_file_option == "" {
        let stdout = io::stdout();
        Box::new(stdout)
    } else {
        let output_file =
            fs::File::create(output_file_option).expect("Could not create output file");
        Box::new(output_file)
    };

    io::BufWriter::new(output)
}

fn read_text<R: io::BufRead + ?Sized>(r: &mut R) -> String {
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

    let status = std::process::Command::new("clang")
        .args(arguments)
        .status()
        .map_err(|_| "failed to execute clang")?;

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
