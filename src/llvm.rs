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
    pub value: LLVMValueRef,
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

    pub fn append_basic_block(
        &self,
        function: Function,
        block_name: LLVMString,
    ) -> LLVMBasicBlockRef {
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
    pub fn new(module: &Module, bb: LLVMBasicBlockRef) -> Self {
        unsafe {
            let inner_builder = LLVMCreateBuilderInContext(module.inner_context);
            LLVMPositionBuilderAtEnd(inner_builder, bb);
            Builder { inner_builder }
        }
    }

    pub fn add(
        &self,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        result_name: LLVMString,
    ) -> LLVMValueRef {
        unsafe { LLVMBuildAdd(self.inner_builder, lhs, rhs, result_name) }
    }

    pub fn call(
        &self,
        function: Function,
        arguments: &mut [LLVMValueRef],
        result_name: LLVMString,
    ) -> LLVMValueRef {
        unsafe {
            LLVMBuildCall(
                self.inner_builder,
                function.value,
                arguments.as_mut_ptr(),
                arguments.len() as u32,
                result_name,
            )
        }
    }

    pub fn call_void(&self, function: Function, arguments: &mut [LLVMValueRef]) -> LLVMValueRef {
        self.call(function, arguments, llvm_str!(b"\0"))
    }

    pub fn load(&self, ptr_source: LLVMValueRef, result_name: LLVMString) -> LLVMValueRef {
        unsafe { LLVMBuildLoad(self.inner_builder, ptr_source, result_name) }
    }

    pub fn store(&self, value: LLVMValueRef, ptr_destination: LLVMValueRef) -> LLVMValueRef {
        unsafe { LLVMBuildStore(self.inner_builder, value, ptr_destination) }
    }

    pub fn getelementptr(
        &self,
        pointer: LLVMValueRef,
        index: LLVMValueRef,
        result_name: LLVMString,
    ) -> LLVMValueRef {
        let mut indeces = vec![index];
        unsafe {
            LLVMBuildGEP(
                self.inner_builder,
                pointer,
                indeces.as_mut_ptr(),
                indeces.len() as u32,
                result_name,
            )
        }
    }

    pub fn icmp(
        &self,
        op: LLVMIntPredicate,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        result_name: LLVMString,
    ) -> LLVMValueRef {
        unsafe { LLVMBuildICmp(self.inner_builder, op, lhs, rhs, result_name) }
    }

    pub fn const_unsigned_int(&self, tp: Type, value: u64) -> LLVMValueRef {
        unsafe { LLVMConstInt(tp.inner_type, value, 0) }
    }

    pub fn const_signed_int(&self, tp: Type, value: i64) -> LLVMValueRef {
        unsafe { LLVMConstInt(tp.inner_type, value as u64, 1) }
    }

    pub fn ret(&self, value: LLVMValueRef) {
        unsafe {
            LLVMBuildRet(self.inner_builder, value);
        }
    }

    pub fn br(&self, dest: LLVMBasicBlockRef) -> LLVMValueRef {
        unsafe { LLVMBuildBr(self.inner_builder, dest) }
    }

    pub fn cond_br(
        &self,
        if_value: LLVMValueRef,
        then_block: LLVMBasicBlockRef,
        else_block: LLVMBasicBlockRef,
    ) -> LLVMValueRef {
        unsafe { LLVMBuildCondBr(self.inner_builder, if_value, then_block, else_block) }
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

    pub fn append_basic_block(&self, name: LLVMString) -> LLVMBasicBlockRef {
        unsafe { LLVMAppendBasicBlock(self.value, name) }
    }
}
