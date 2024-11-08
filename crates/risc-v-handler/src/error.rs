use reth_revm::primitives::EVMError;
use rvemu::exception::Exception;

/// Errors returned during the RISC-V context execution
#[derive(Debug, thiserror::Error)]
pub(crate) enum RiscVError {
    /// The exception kind on RISC-V [`emulator`](`Emulator`)
    #[error("Got RISC-V emulator exception: {0:?}")]
    RvEmuException(Exception),
    /// Unhandled system call
    #[error("Unhandled syscall: {0}")]
    UnhandledSyscall(u32),
}

impl<E> From<RiscVError> for EVMError<E> {
    #[inline]
    fn from(err: RiscVError) -> Self {
        EVMError::Custom(err.to_string())
    }
}

impl From<Exception> for RiscVError {
    #[inline]
    fn from(exception: Exception) -> Self {
        Self::RvEmuException(exception)
    }
}
