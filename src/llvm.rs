pub extern crate llvm_sys as sys;

use std::{mem, ptr};

use self::sys::*;
use self::sys::prelude::*;
use self::sys::core::*;
use self::sys::execution_engine::*;
use self::sys::target::*;
use self::sys::analysis::*;
use llvm::sys::transforms::pass_manager_builder::*;

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
    pub i1_type: Type,
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
            let i1_type = Type::new(LLVMInt1TypeInContext(inner_context));
            let i8_type = Type::new(LLVMInt8TypeInContext(inner_context));
            let i32_type = Type::new(LLVMInt32TypeInContext(inner_context));

            Module {
                inner_context,
                inner_module,

                void_type,
                i1_type,
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

    pub fn optimize(&self, opt_level: u32) {
        unsafe {

            let manager_builder = LLVMPassManagerBuilderCreate();
            LLVMPassManagerBuilderSetOptLevel(manager_builder, opt_level);

            let pass_manager = LLVMCreatePassManager();
            LLVMPassManagerBuilderPopulateModulePassManager(manager_builder, pass_manager);
            LLVMPassManagerBuilderDispose(manager_builder);
            LLVMRunPassManager(pass_manager, self.inner_module);

            LLVMDisposePassManager(pass_manager);
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

macro_rules! build_bin_op {
	($op_name:ident, $fn_name:ident) => {
		impl Builder {
			pub fn $op_name<LV: LoadValue, RV: LoadValue, RetV: StoreValue<Result>, Result>(
				&self,
				lhs_value: LV,
				rhs_value: RV,
				result: RetV,
				) -> Result {
				unsafe {
					let ret = $fn_name(
						self.inner_builder,
						lhs_value.load_value(self),
						rhs_value.load_value(self),
						result.get_name());
					result.store_value(self, ret)
				}
			}
		}
	}
}

macro_rules! build_cast_op {
	($op_name:ident, $fn_name:ident) => {
		impl Builder {
			pub fn $op_name<V: LoadValue, RetV: StoreValue<Result>, Result>(
				&self,
				value: V,
				tp: Type,
				result: RetV,
				) -> Result {
				unsafe {
					let ret = $fn_name(
						self.inner_builder,
						value.load_value(self),
						tp.inner_type,
						result.get_name());
					result.store_value(self, ret)
				}
			}
		}
	}
}

build_bin_op!(add, LLVMBuildAdd);
build_bin_op!(udiv, LLVMBuildUDiv);
build_bin_op!(urem, LLVMBuildURem);
build_cast_op!(bit_cast, LLVMBuildBitCast);
build_cast_op!(trunc, LLVMBuildTrunc);

impl Builder {
    pub fn new(module: &Module, bb: BasicBlock) -> Self {
        unsafe {
            let inner_builder = LLVMCreateBuilderInContext(module.inner_context);
            LLVMPositionBuilderAtEnd(inner_builder, bb);
            Builder { inner_builder }
        }
    }

    pub fn uint(&self, tp: Type, value: u64) -> Value {
        unsafe { LLVMConstInt(tp.inner_type, value, 0) }
    }

    pub fn sint(&self, tp: Type, value: i64) -> Value {
        unsafe { LLVMConstInt(tp.inner_type, value as u64, 1) }
    }

    pub fn call<RetV: StoreValue<R>, R>(
        &self,
        function: Function,
        arguments: &mut [Value],
        result: RetV,
    ) -> R {
        unsafe {
            let ret = LLVMBuildCall(
                self.inner_builder,
                function.value,
                arguments.as_mut_ptr(),
                arguments.len() as u32,
                result.get_name(),
            );
            result.store_value(self, ret)
        }
    }

    pub fn alloca(&self, tp: Type, name: LLVMString) -> Value {
        unsafe { LLVMBuildAlloca(self.inner_builder, tp.inner_type, name) }
    }

    pub fn load<V: LoadValue>(&self, ptr_source: V, name: LLVMString) -> Value {
        unsafe { LLVMBuildLoad(self.inner_builder, ptr_source.load_value(self), name) }
    }

    pub fn store<V: LoadValue, PV: LoadValue>(&self, value: V, ptr_dest: PV) -> Value {
        unsafe {
            LLVMBuildStore(
                self.inner_builder,
                value.load_value(self),
                ptr_dest.load_value(self),
            )
        }
    }

    pub fn getelementptr<PV: LoadValue, IV: LoadValue, RetV: StoreValue<R>, R>(
        &self,
        pointer: PV,
        index: IV,
        result: RetV,
    ) -> R {
        let mut indeces = vec![index.load_value(self)];
        unsafe {
            let ret = LLVMBuildGEP(
                self.inner_builder,
                pointer.load_value(self),
                indeces.as_mut_ptr(),
                indeces.len() as u32,
                result.get_name(),
            );
            result.store_value(self, ret)
        }
    }

    pub fn icmp<LV: LoadValue, RV: LoadValue, RetV: StoreValue<R>, R>(
        &self,
        op: LLVMIntPredicate,
        lhs: LV,
        rhs: RV,
        result: RetV,
    ) -> R {
        unsafe {
            let ret = LLVMBuildICmp(
                self.inner_builder,
                op,
                lhs.load_value(self),
                rhs.load_value(self),
                result.get_name(),
            );
            result.store_value(self, ret)
        }
    }

    pub fn ret<V: LoadValue>(&self, value: V) {
        unsafe {
            LLVMBuildRet(self.inner_builder, value.load_value(self));
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

    pub fn cond_br<V: LoadValue>(
        &self,
        if_value: V,
        then_block: BasicBlock,
        else_block: BasicBlock,
    ) -> Value {
        unsafe {
            LLVMBuildCondBr(
                self.inner_builder,
                if_value.load_value(self),
                then_block,
                else_block,
            )
        }
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
}

impl PhiNode {
    pub fn add_incoming(&self, incoming_value: Value, incoming_block: BasicBlock) {
        let mut values = vec![incoming_value];
        let mut block = vec![incoming_block];
        unsafe { LLVMAddIncoming(self.value, values.as_mut_ptr(), block.as_mut_ptr(), 1) }
    }
}

#[derive(Clone, Copy)]
pub struct Var {
    value_ptr: Value,
}

impl Var {
    pub fn alloc(builder: &Builder, tp: Type, value: Value, name: LLVMString) -> Self {
        let value_ptr = builder.alloca(tp, name);
        let result = Var { value_ptr };
        result.store(builder, value);
        return result;
    }

    pub fn load(&self, builder: &Builder) -> Value {
        builder.load(self.value_ptr, llvm_str!(b"value\0"))
    }

    pub fn store(&self, builder: &Builder, value: Value) {
        builder.store(value, self.value_ptr);
    }
}

pub trait LoadValue {
    fn load_value(&self, builder: &Builder) -> Value;
}

impl LoadValue for Value {
    fn load_value(&self, _builder: &Builder) -> Value {
        *self
    }
}

impl LoadValue for Var {
    fn load_value(&self, builder: &Builder) -> Value {
        self.load(builder)
    }
}

pub trait StoreValue<Result> {
    fn get_name(&self) -> LLVMString;
    fn store_value<V: LoadValue>(&self, builder: &Builder, value: V) -> Result;
}

impl StoreValue<Value> for LLVMString {
    fn get_name(&self) -> LLVMString {
        *self
    }
    fn store_value<V: LoadValue>(&self, builder: &Builder, value: V) -> Value {
        value.load_value(&builder)
    }
}

impl StoreValue<()> for Var {
    fn get_name(&self) -> LLVMString {
        llvm_str!(b"var_val\0")
    }
    fn store_value<V: LoadValue>(&self, builder: &Builder, value: V) -> () {
        self.store(&builder, value.load_value(&builder));
    }
}

impl StoreValue<()> for () {
    fn get_name(&self) -> LLVMString {
        llvm_str!(b"\0")
    }
    fn store_value<V: LoadValue>(&self, _builder: &Builder, _value: V) -> () {
        ()
    }
}
