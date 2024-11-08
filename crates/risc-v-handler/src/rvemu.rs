use std::ops::Range;

use eth_riscv_syscalls::Syscall;
use reth_revm::{
    interpreter::{
        CallInputs, CallScheme, CallValue, Host, InstructionResult, Interpreter, InterpreterAction,
        InterpreterResult, SharedMemory, StateLoad,
    },
    primitives::{Address, Bytes, U256},
};
use rvemu::{emulator::Emulator, exception::Exception};

use super::RiscVError;

/// RISC-V emulator
#[derive(Debug)]
pub(crate) struct RVEmu {
    /// The emulator, that holds a RISC-V CPU
    pub(crate) emu: Emulator,
    /// Range to get regarded RISC-V DRAM memory slice and set it with
    /// shared memory data on frame execution handler
    pub(crate) returned_data_destiny: Option<Range<u64>>,
}

impl RVEmu {
    /// Creates a new [`RVEmu`]
    pub(crate) const fn new(emu: Emulator) -> Self {
        Self { emu, returned_data_destiny: None }
    }

    /// Handles memory operations between shared memory and RISC-V DRAM
    pub(crate) fn handle_shared_memory(
        &mut self,
        shared_memory: &mut SharedMemory,
    ) -> Result<(), RiscVError> {
        if let Some(destiny) = std::mem::take(&mut self.returned_data_destiny) {
            let data = self.emu.cpu.bus.get_dram_slice(destiny)?;
            data.copy_from_slice(shared_memory.slice(0, data.len()));
            tracing::trace!("Copied {} bytes to DRAM range", data.len());
        }

        Ok(())
    }

    /// Handles a system call based on the value on RISC-V CPU's integer register
    pub(crate) fn handle_syscall(
        &mut self,
        interpreter: &mut Interpreter,
        host: &mut dyn Host,
    ) -> Result<InterpreterAction, RiscVError> {
        let emu = &mut self.emu;
        let returned_data_destiny = &mut self.returned_data_destiny;

        // Run emulator and capture ecalls
        loop {
            let run_result = emu.start();
            match run_result {
                Err(Exception::EnvironmentCallFromMMode) => {
                    let t0 = emu.cpu.xregs.read(5) as u32;
                    let syscall =
                        Syscall::try_from(t0).map_err(|_| RiscVError::UnhandledSyscall(t0))?;
                    match syscall {
                        Syscall::Return => {
                            let ret_offset: u64 = emu.cpu.xregs.read(10);
                            let ret_size: u64 = emu.cpu.xregs.read(11);
                            let data_bytes = if ret_size != 0 {
                                emu.cpu
                                    .bus
                                    .get_dram_slice(ret_offset..(ret_offset + ret_size))
                                    .unwrap()
                            } else {
                                &mut []
                            };
                            return Ok(InterpreterAction::Return {
                                result: InterpreterResult {
                                    result: InstructionResult::Return,
                                    output: data_bytes.to_vec().into(),
                                    gas: interpreter.gas,
                                },
                            });
                        }
                        Syscall::SLoad => {
                            let key: u64 = emu.cpu.xregs.read(10);
                            match host.sload(interpreter.contract.target_address, U256::from(key)) {
                                Some(StateLoad { data, is_cold: _ }) => {
                                    emu.cpu.xregs.write(10, data.as_limbs()[0]);
                                }
                                _ => {
                                    return return_revert(interpreter);
                                }
                            }
                        }
                        Syscall::SStore => {
                            let key: u64 = emu.cpu.xregs.read(10);
                            let value: u64 = emu.cpu.xregs.read(11);
                            host.sstore(
                                interpreter.contract.target_address,
                                U256::from(key),
                                U256::from(value),
                            );
                        }
                        Syscall::Call => {
                            let a0: u64 = emu.cpu.xregs.read(10);
                            let address = Address::from_slice(
                                emu.cpu.bus.get_dram_slice(a0..(a0 + 20)).unwrap(),
                            );
                            let value: u64 = emu.cpu.xregs.read(11);
                            let args_offset: u64 = emu.cpu.xregs.read(12);
                            let args_size: u64 = emu.cpu.xregs.read(13);
                            let ret_offset = emu.cpu.xregs.read(14);
                            let ret_size = emu.cpu.xregs.read(15);

                            *returned_data_destiny = Some(ret_offset..(ret_offset + ret_size));

                            let tx = &host.env().tx;
                            return Ok(InterpreterAction::Call {
                                inputs: Box::new(CallInputs {
                                    input: emu
                                        .cpu
                                        .bus
                                        .get_dram_slice(args_offset..(args_offset + args_size))
                                        .unwrap()
                                        .to_vec()
                                        .into(),
                                    gas_limit: tx.gas_limit,
                                    target_address: address,
                                    bytecode_address: address,
                                    caller: interpreter.contract.target_address,
                                    value: CallValue::Transfer(U256::from_le_bytes(
                                        value.to_le_bytes(),
                                    )),
                                    scheme: CallScheme::Call,
                                    is_static: false,
                                    is_eof: false,
                                    return_memory_offset: 0..ret_size as usize,
                                }),
                            });
                        }
                        Syscall::Revert => {
                            return Ok(InterpreterAction::Return {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::from(0u32.to_le_bytes()),
                                    gas: interpreter.gas,
                                },
                            });
                        }
                        Syscall::Caller => {
                            let caller = interpreter.contract.caller;
                            // Break address into 3 u64s and write to registers
                            let caller_bytes = caller.as_slice();
                            let first_u64 =
                                u64::from_be_bytes(caller_bytes[0..8].try_into().unwrap());
                            emu.cpu.xregs.write(10, first_u64);
                            let second_u64 =
                                u64::from_be_bytes(caller_bytes[8..16].try_into().unwrap());
                            emu.cpu.xregs.write(11, second_u64);
                            let mut padded_bytes = [0u8; 8];
                            padded_bytes[..4].copy_from_slice(&caller_bytes[16..20]);
                            let third_u64 = u64::from_be_bytes(padded_bytes);
                            emu.cpu.xregs.write(12, third_u64);
                        }
                    }
                }
                _ => {
                    return return_revert(interpreter);
                }
            }
        }
    }
}

/// Helper function to create a revert action
fn return_revert(interpreter: &mut Interpreter) -> Result<InterpreterAction, RiscVError> {
    Ok(InterpreterAction::Return {
        result: InterpreterResult {
            result: InstructionResult::Revert,
            output: Bytes::new(),
            gas: interpreter.gas,
        },
    })
}
