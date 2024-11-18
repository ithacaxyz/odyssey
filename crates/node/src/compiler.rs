use alloy_primitives::B256;
use reth_revm::{
    handler::register::EvmHandler,
    interpreter::{InterpreterAction, SharedMemory, EMPTY_SHARED_MEMORY},
    Context as RevmContext, Database, Frame,
};
use revmc::EvmCompilerFn;
use std::sync::Arc;
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
};

#[derive(Debug)]
pub struct ExternalContext {
    cache: HashMap<B256, EvmCompilerFn>,
}

impl ExternalContext {
    pub fn new() -> Self {
        let cache = HashMap::new();
        Self { cache }
    }

    pub fn get_compiled_fn(&self, hash: B256) -> Option<EvmCompilerFn> {
        self.cache.get(&hash).cloned()
    }
}

pub fn register_compiler_handler<DB>(handler: &mut EvmHandler<'_, ExternalContext, DB>)
where
    DB: Database,
{
    let f = handler.execution.execute_frame.clone();

    handler.execution.execute_frame = Arc::new(move |frame, memory, table, context| {
        let Some(action) = execute_frame(frame, memory, context) else {
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

    let f = context.external.get_compiled_fn(hash)?;

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
