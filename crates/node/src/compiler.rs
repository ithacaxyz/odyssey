/*! The compiler module is responsible for compiling EVM bytecode to machine code using LLVM. */

use alloy_primitives::{hex, Bytes, B256};
use core::panic;
use reth_revm::{
    handler::register::EvmHandler,
    interpreter::{InterpreterAction, SharedMemory},
    Context as RevmContext, Database, Frame,
};
use revmc::{
    llvm::Context as LlvmContext, primitives::SpecId, EvmCompiler, EvmCompilerFn, EvmLlvmBackend,
    OptimizationLevel,
};
use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
};

/// The [Compiler] struct is a client for passing functions to the compiler thread. It also contains a cache of compiled functions
#[derive(Debug, Clone)]
pub struct Compiler {
    sender: Sender<(SpecId, B256, Bytes)>,
    fn_cache: Arc<Mutex<HashMap<B256, Option<EvmCompilerFn>>>>,
}

// TODO: probably shouldn't have a default for something that spawns a thread?
impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    /// Create a new compiler instance. This spawns a new compiler thread and the returned struct contains a [Sender](std::sync::mpsc::Sender) for sending functions to the compiler thread,
    /// as well as a cache to compiled functions
    pub fn new() -> Self {
        let fn_cache = Arc::new(Mutex::new(HashMap::new()));
        let (sender, receiver) = std::sync::mpsc::channel();

        // TODO: graceful shutdown
        thread::spawn({
            let fn_cache = fn_cache.clone();

            move || {
                let ctx = LlvmContext::create();
                // let mut compilers = Vec::new();

                while let Ok((spec_id, hash, code)) = receiver.recv() {
                    fn_cache.lock().unwrap().insert(hash, None);

                    // TODO: fail properly here.
                    let backend =
                        EvmLlvmBackend::new(&ctx, false, OptimizationLevel::Aggressive).unwrap();
                    let compiler = Box::leak(Box::new(EvmCompiler::new(backend)));

                    // Do we have to allocate here? Not sure there's a better option
                    let name = hex::encode(hash);
                    dbg!("compiled", &name);

                    let result =
                        unsafe { compiler.jit(&name, &code, spec_id) }.expect("catastrophe");

                    fn_cache.lock().unwrap().insert(hash, Some(result));

                    // compilers.push(compiler);
                }
            }
        });

        Self { sender, fn_cache }
    }

    // TODO:
    // For safety, we should also borrow the EvmCompiler that holds the actual module with code to
    // make sure that it's not dropped while before or during the function call.
    fn get_compiled_fn(&self, spec_id: SpecId, hash: B256, code: Bytes) -> Option<EvmCompilerFn> {
        match self.fn_cache.lock().unwrap().get(&hash) {
            Some(maybe_f) => *maybe_f,
            None => {
                // TODO: put rules here for whether or not to compile the function
                self.sender.send((spec_id, hash, code)).unwrap();
                None
            }
        }
    }
}

/// The [ExternalContext] struct is a container for the [Compiler] struct.
#[derive(Debug)]
pub struct ExternalContext {
    compiler: Compiler,
}

impl ExternalContext {
    /// Create a new [ExternalContext] instance from a given [Compiler] instance.
    pub const fn new(compiler: Compiler) -> Self {
        Self { compiler }
    }

    /// Get a compiled function if one exists, otherwise send the bytecode to the compiler to be compiled.
    pub fn get_compiled_fn(
        &self,
        spec_id: SpecId,
        hash: B256,
        code: Bytes,
    ) -> Option<EvmCompilerFn> {
        self.compiler.get_compiled_fn(spec_id, hash, code)
    }
}

/// Registers the compiler handler with the EVM handler.
pub fn register_compiler_handler<DB>(handler: &mut EvmHandler<'_, ExternalContext, DB>)
where
    DB: Database,
{
    let f = handler.execution.execute_frame.clone();
    let spec_id = handler.cfg.spec_id;

    handler.execution.execute_frame = Arc::new(move |frame, memory, table, context| {
        let Some(action) = execute_frame(spec_id, frame, memory, context) else {
            dbg!("fallback");
            return f(frame, memory, table, context);
        };

        Ok(action)
    });
}

fn execute_frame<DB: Database>(
    spec_id: SpecId,
    frame: &mut Frame,
    memory: &mut SharedMemory,
    context: &mut RevmContext<ExternalContext, DB>,
) -> Option<InterpreterAction> {
    // let library = context.external.get_or_load_library(context.evm.spec_id())?;
    let interpreter = frame.interpreter_mut();

    let hash = match interpreter.contract.hash {
        Some(hash) => hash,
        // TODO: is this an issue with EOF?
        None => unreachable_no_hash(),
    };

    // should be cheap enough to clone because it's backed by bytes::Bytes
    let code = interpreter.contract.bytecode.bytes();

    let f = context.external.get_compiled_fn(spec_id, hash, code)?;

    // Safety: as long as the function is still in the cache, this is safe to call
    let result = unsafe { f.call_with_interpreter_and_memory(interpreter, memory, context) };

    dbg!("EXECUTED", &hash);

    Some(result)
}

#[cold]
#[inline(never)]
const fn unreachable_no_hash() -> ! {
    panic!("unreachable: bytecode hash is not set in the interpreter")
}
