//! Shared between the WSL library and the Windows executable: `sk-api.h`
//! constants, the CBOR messages, their framing and the stderr logger.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod frame;
pub mod log;
pub mod message;
pub mod sk;
