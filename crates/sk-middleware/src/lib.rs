//! OpenSSH security key middleware (`SSH_SK_PROVIDER`).
//!
//! Implements the C ABI from OpenSSH's `sk-api.h` and forwards each request to
//! `windowsfidobridge.exe`, which must sit next to this library.

/// `SSH_SK_VERSION_MAJOR` from `sk-api.h`; unchanged since OpenSSH 9.1.
pub const SSH_SK_VERSION_MAJOR: u32 = 0x000a_0000;

#[unsafe(no_mangle)]
pub extern "C" fn sk_api_version() -> u32 {
    SSH_SK_VERSION_MAJOR
}
