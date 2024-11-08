//! Odyssey's RISC-V EVM handler
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::{cell::RefCell, rc::Rc, sync::Arc};

use eth_riscv_interpreter::setup_from_elf;
use reth_revm::{
    handler::register::EvmHandler,
    interpreter::{Host, Interpreter, InterpreterAction, SharedMemory},
    Database, Frame, FrameOrResult,
};

mod error;
use error::RiscVError;

mod rvemu;
use rvemu::RVEmu;

/// RISC-V magic bytes
const RISC_V_MAGIC: &[u8] = &[0xFF];

/// RISC-V EVM handler register
pub fn risc_v_handle_register<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
    let call_stack = Rc::<RefCell<Vec<_>>>::new(RefCell::new(Vec::new()));

    // create a riscv context on call frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.call.clone();
    handler.execution.call = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(riscv_context(frame));
        }
        result
    });

    // create a riscv context on create frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.create.clone();
    handler.execution.create = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(riscv_context(frame));
        }
        result
    });

    // execute riscv context or old logic.
    let old_handle = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, instraction_table, ctx| {
        let result = if let Some(Some(riscv_context)) = call_stack.borrow_mut().first_mut() {
            execute_riscv(riscv_context, frame.interpreter_mut(), memory, ctx)?
        } else {
            old_handle(frame, memory, instraction_table, ctx)?
        };

        // if it is return pop the stack.
        if result.is_return() {
            call_stack.borrow_mut().pop();
        }
        Ok(result)
    });
}

/// Setup RISC-V execution context if bytecode starts with [`RISC_V_MAGIC`].
///
/// Load and parse the ELF (Electronic Linker Format), then
/// - allocates contract input size and data to emulator's CPU memory.
/// - allocates currently run bytecode to emulator's CPU memory
///
/// # Note:
/// By default it preallocated 1Mb for RISC-V DRAM data
fn riscv_context(frame: &Frame) -> Option<RVEmu> {
    let interpreter = frame.interpreter();

    let Some((RISC_V_MAGIC, bytecode)) = interpreter.bytecode.split_at_checked(RISC_V_MAGIC.len())
    else {
        return None;
    };

    let emu = setup_from_elf(bytecode, &interpreter.contract.input);
    Some(RVEmu::new(emu))
}

/// Executes frame in the RISC-V context
///
/// FIXME: gas is not correct on interpreter return.
fn execute_riscv(
    rvemu: &mut RVEmu,
    interpreter: &mut Interpreter,
    shared_memory: &mut SharedMemory,
    host: &mut dyn Host,
) -> Result<InterpreterAction, RiscVError> {
    rvemu.handle_shared_memory(shared_memory)?;
    rvemu.handle_syscall(interpreter, host)
}
