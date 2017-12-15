pub extern crate llvm_sys as sys;

use std::{mem, ptr};

use self::sys::*;
use self::sys::prelude::*;
use self::sys::core::*;
use self::sys::execution_engine::*;
use self::sys::target::*;
use self::sys::analysis::*;

macro_rules! llvm_str {
	($e:expr) => {{
		debug_assert_eq!($e.last(), Some(&0)); // string must terminate with '\0
		$e.as_ptr() as *const i8
	}}
}

pub type LLVMString = *const i8;
pub type Value = LLVMValueRef;
pub type BasicBlock = LLVMBasicBlockRef;

pub struct Module {
    inner_context: LLVMContextRef,
    inner_module: LLVMModuleRef,

    pub void_type: Type,
    pub i8_type: Type,
    pub i32_type: Type,
}

#[derive(Copy, Clone)]
pub struct Type {
    inner_type: LLVMTypeRef,
}

#[derive(Clone)]
pub struct Builder {
    inner_builder: LLVMBuilderRef,
}

#[derive(Copy, Clone)]
pub struct Function {
    pub value: Value,
}

#[derive(Copy, Clone)]
pub struct PhiNode {
    pub value: Value,
}

impl Module {
    pub fn new(module_name: LLVMString) -> Self {
        unsafe {
            let inner_context = LLVMContextCreate();
            let inner_module = LLVMModuleCreateWithNameInContext(module_name, inner_context);

            let void_type = Type::new(LLVMVoidTypeInContext(inner_context));
            let i8_type = Type::new(LLVMInt8TypeInContext(inner_context));
            let i32_type = Type::new(LLVMInt32TypeInContext(inner_context));

            Module {
                inner_context,
                inner_module,

                void_type,
                i8_type,
                i32_type,
            }
        }
    }

    pub fn add_function(
        &self,
        function_name: LLVMString,
        arguments: &mut [Type],
        return_type: Type,
    ) -> Function {

        let mut args: Vec<_> = arguments.iter().map(|tp| tp.inner_type).collect();

        unsafe {
            let function_type = LLVMFunctionType(
                return_type.inner_type,
                args.as_mut_ptr(),
                args.len() as u32,
                0,
            );
            let func = LLVMAddFunction(self.inner_module, function_name, function_type);
            Function { value: func }
        }
    }

    pub fn append_basic_block(&self, function: Function, block_name: LLVMString) -> BasicBlock {
        unsafe { LLVMAppendBasicBlockInContext(self.inner_context, function.value, block_name) }
    }

    pub fn dump(&self) {
        unsafe {
            LLVMDumpModule(self.inner_module);
        }
    }

    pub fn verify(&self) {
        unsafe {
            let message = ptr::null_mut();
            LLVMVerifyModule(
                self.inner_module,
                LLVMVerifierFailureAction::LLVMPrintMessageAction,
                message,
            );
        }
    }

    pub fn jit_function(&self, function_name: LLVMString) {
        unsafe {
            LLVMLinkInMCJIT();
            LLVM_InitializeNativeTarget();
            LLVM_InitializeNativeAsmPrinter();

            let mut ee = mem::uninitialized();
            let mut out = mem::zeroed();
            LLVMCreateExecutionEngineForModule(&mut ee, self.inner_module, &mut out);

            let addr = LLVMGetFunctionAddress(ee, function_name);
            let func: extern "C" fn() -> u8 = mem::transmute(addr);

            println!(">>>");
            let return_value = func();
            println!("<<<");
            println!("Return Value: {}", return_value);

            LLVMDisposeExecutionEngine(ee);
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            LLVMDisposeModule(self.inner_module);
            LLVMContextDispose(self.inner_context);
        }
    }
}

impl Type {
    pub fn new(inner_type: LLVMTypeRef) -> Self {
        Type { inner_type }
    }

    pub fn ptr_type(&self) -> Type {
        let inner_type = unsafe { LLVMPointerType(self.inner_type, 0) };
        Type::new(inner_type)
    }
}

impl Builder {
    pub fn new(module: &Module, bb: BasicBlock) -> Self {
        unsafe {
            let inner_builder = LLVMCreateBuilderInContext(module.inner_context);
            LLVMPositionBuilderAtEnd(inner_builder, bb);
            Builder { inner_builder }
        }
    }

    pub fn call(&self, function: Function, arguments: &mut [Value], name: LLVMString) -> Value {
        unsafe {
            LLVMBuildCall(
                self.inner_builder,
                function.value,
                arguments.as_mut_ptr(),
                arguments.len() as u32,
                name,
            )
        }
    }

    pub fn call_void(&self, function: Function, arguments: &mut [Value]) -> Value {
        self.call(function, arguments, llvm_str!(b"\0"))
    }

    pub fn alloca(&self, tp: Type, name: LLVMString) -> Value {
        unsafe { LLVMBuildAlloca(self.inner_builder, tp.inner_type, name) }
    }

    pub fn load(&self, ptr_source: Value, name: LLVMString) -> Value {
        unsafe { LLVMBuildLoad(self.inner_builder, ptr_source, name) }
    }

    pub fn store(&self, value: Value, ptr_destination: Value) -> Value {
        unsafe { LLVMBuildStore(self.inner_builder, value, ptr_destination) }
    }

    pub fn getelementptr(&self, pointer: Value, index: Value, name: LLVMString) -> Value {
        let mut indeces = vec![index];
        unsafe {
            LLVMBuildGEP(
                self.inner_builder,
                pointer,
                indeces.as_mut_ptr(),
                indeces.len() as u32,
                name,
            )
        }
    }

    pub fn add(&self, lhs: Value, rhs: Value, name: LLVMString) -> Value {
        unsafe { LLVMBuildAdd(self.inner_builder, lhs, rhs, name) }
    }

    pub fn icmp(&self, op: LLVMIntPredicate, lhs: Value, rhs: Value, name: LLVMString) -> Value {
        unsafe { LLVMBuildICmp(self.inner_builder, op, lhs, rhs, name) }
    }

    pub fn udiv(&self, lhs: Value, rhs: Value, name: LLVMString) -> Value {
        unsafe { LLVMBuildUDiv(self.inner_builder, lhs, rhs, name) }
    }

    pub fn urem(&self, lhs: Value, rhs: Value, name: LLVMString) -> Value {
        unsafe { LLVMBuildURem(self.inner_builder, lhs, rhs, name) }
    }

    pub fn const_unsigned_int(&self, tp: Type, value: u64) -> Value {
        unsafe { LLVMConstInt(tp.inner_type, value, 0) }
    }

    pub fn const_signed_int(&self, tp: Type, value: i64) -> Value {
        unsafe { LLVMConstInt(tp.inner_type, value as u64, 1) }
    }

    pub fn bit_cast(&self, value: Value, dest_type: Type, name: LLVMString) -> Value {
        unsafe { LLVMBuildBitCast(self.inner_builder, value, dest_type.inner_type, name) }
    }

    pub fn trunc(&self, value: Value, dest_type: Type, name: LLVMString) -> Value {
        unsafe { LLVMBuildTrunc(self.inner_builder, value, dest_type.inner_type, name) }
    }

    pub fn ret(&self, value: Value) {
        unsafe {
            LLVMBuildRet(self.inner_builder, value);
        }
    }

    pub fn ret_void(&self) {
        unsafe {
            LLVMBuildRetVoid(self.inner_builder);
        }
    }

    pub fn br(&self, dest: BasicBlock) -> Value {
        unsafe { LLVMBuildBr(self.inner_builder, dest) }
    }

    pub fn cond_br(
        &self,
        if_value: Value,
        then_block: BasicBlock,
        else_block: BasicBlock,
    ) -> Value {
        unsafe { LLVMBuildCondBr(self.inner_builder, if_value, then_block, else_block) }
    }

    pub fn phi(&self, tp: Type, name: LLVMString) -> PhiNode {
        unsafe {
            let value = LLVMBuildPhi(self.inner_builder, tp.inner_type, name);
            PhiNode { value }
        }
    }
}

impl Drop for Builder {
    fn drop(&mut self) {
        unsafe {
            LLVMDisposeBuilder(self.inner_builder);
        }
    }
}

impl Function {
    pub fn verify(&self) {
        unsafe {
            LLVMVerifyFunction(
                self.value,
                LLVMVerifierFailureAction::LLVMPrintMessageAction,
            );
        }
    }

    pub fn append_basic_block(&self, name: LLVMString) -> BasicBlock {
        unsafe { LLVMAppendBasicBlock(self.value, name) }
    }

    pub fn get_param(&self, index: u32) -> Value {
        unsafe { LLVMGetParam(self.value, index) }
    }

    pub fn null() -> Self {
        Function { value: ptr::null_mut() }
    }
}

impl PhiNode {
    pub fn add_incoming(&self, incoming_value: Value, incoming_block: BasicBlock) {
        let mut values = vec![incoming_value];
        let mut block = vec![incoming_block];
        unsafe { LLVMAddIncoming(self.value, values.as_mut_ptr(), block.as_mut_ptr(), 1) }
    }
}
