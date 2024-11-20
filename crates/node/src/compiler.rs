use alloy_primitives::{hex, Bytes, B256};
use core::panic;
use reth_primitives::Bytecode;
use reth_revm::{
    handler::register::EvmHandler,
    interpreter::{InterpreterAction, SharedMemory, EMPTY_SHARED_MEMORY},
    Context as RevmContext, Database, Frame,
};
use revmc::{primitives::SpecId, EvmCompilerFn, EvmLlvmBackend, OptimizationLevel};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    thread,
};

#[derive(Debug)]
pub struct ExternalContext {
    sender: Sender<(B256, Bytes)>,
    // TODO: cache shouldn't be here (and should definitely not be wrapped in a mutex)
    cache: Arc<Mutex<HashMap<B256, EvmCompilerFn>>>,
}

impl ExternalContext {
    pub fn new(spec_id: SpecId) -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let (sender, receiver) = std::sync::mpsc::channel();

        // TODO: graceful shutdown
        thread::spawn({
            let cache = cache.clone();

            move || {
                let context = Box::leak(Box::new(revmc::llvm::Context::create()));
                // TODO: fail properly here.
                let backend =
                    EvmLlvmBackend::new(context, false, OptimizationLevel::Aggressive).unwrap();
                let mut compiler = revmc::EvmCompiler::new(backend);

                while let Ok((hash, code)) = receiver.recv() {
                    // Do we have to allocate here? Not sure there's a better option
                    let name = hex::encode(hash);
                    dbg!("compiled", &name);

                    let result =
                        unsafe { compiler.jit(&name, &code, spec_id) }.expect("catastrophe");

                    cache.lock().unwrap().insert(hash, result);

                    unsafe { compiler.clear().expect("could not clear") };
                }
            }
        });

        Self { sender, cache }
    }

    pub fn get_compiled_fn(&self, hash: B256, code: Bytes) -> Option<EvmCompilerFn> {
        let Some(f) = self.cache.lock().unwrap().get(&hash).cloned() else {
            self.sender.send((hash, code)).unwrap();
            return None;
        };

        Some(f)
    }
}

pub fn register_compiler_handler<DB>(handler: &mut EvmHandler<'_, ExternalContext, DB>)
where
    DB: Database,
{
    let f = handler.execution.execute_frame.clone();

    handler.execution.execute_frame = Arc::new(move |frame, memory, table, context| {
        let Some(action) = execute_frame(frame, memory, context) else {
            dbg!("fallback");
            return f(frame, memory, table, context);
        };

        Ok(action)
    });
}

fn execute_frame<DB: Database>(
    frame: &mut Frame,
    memory: &mut SharedMemory,
    context: &mut RevmContext<ExternalContext, DB>,
) -> Option<InterpreterAction> {
    // let library = context.external.get_or_load_library(context.evm.spec_id())?;
    let interpreter = frame.interpreter_mut();

    let hash = match interpreter.contract.hash {
        Some(hash) => hash,
        None => unreachable_no_hash(),
    };

    // should be cheap enough to clone because it's backed by bytes::Bytes
    let code = interpreter.contract.bytecode.bytes();

    // TODO: put rules here for whether or not to compile the function
    let f = context.external.get_compiled_fn(hash, code)?;

    // let f = match library.get_function(hash) {
    //     Ok(Some(f)) => f,
    //     Ok(None) => return None,
    //     // Shouldn't happen.
    //     Err(err) => {
    //         unlikely_log_get_function_error(err, &hash);
    //         return None;
    //     }
    // };

    // interpreter.shared_memory =
    //     std::mem::replace(memory, reth_revm::interpreter::EMPTY_SHARED_MEMORY);
    // let result = unsafe { f.call_with_interpreter(interpreter, context) };
    // *memory = interpreter.take_memory();
    // Some(result)

    interpreter.shared_memory = std::mem::replace(memory, EMPTY_SHARED_MEMORY);

    let result = unsafe { f.call_with_interpreter(interpreter, context) };
    *memory = interpreter.take_memory();

    dbg!("executed", &hash);

    Some(result)
}

#[cold]
#[inline(never)]
const fn unreachable_no_hash() -> ! {
    panic!("unreachable: bytecode hash is not set in the interpreter")
}

#[cold]
#[inline(never)]
const fn unreachable_misconfigured() -> ! {
    panic!("unreachable: AOT EVM is misconfigured")
}

#[cold]
#[inline(never)]
fn unlikely_log_get_function_error(err: impl std::error::Error, hash: &B256) {
    tracing::error!(%err, %hash, "failed getting function from shared library");
}
