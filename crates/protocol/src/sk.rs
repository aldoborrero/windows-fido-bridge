//! Constants from OpenSSH `sk-api.h` (API version 10) that the bridge uses.

/// `SSH_SK_VERSION_MAJOR`; unchanged since OpenSSH 9.1.
pub const SSH_SK_VERSION_MAJOR: u32 = 0x000a_0000;

/// `SSH_SK_USER_VERIFICATION_REQD`: the key must verify the user (PIN or biometrics).
pub const SSH_SK_USER_VERIFICATION_REQD: u8 = 0x04;

/// `SSH_SK_ECDSA`: ECDSA P-256 key.
pub const SSH_SK_ECDSA: u32 = 0x00;
/// `SSH_SK_ED25519`: Ed25519 key.
pub const SSH_SK_ED25519: u32 = 0x01;

/// `SSH_SK_ERR_GENERAL`.
pub const SSH_SK_ERR_GENERAL: i32 = -1;
/// `SSH_SK_ERR_UNSUPPORTED`.
pub const SSH_SK_ERR_UNSUPPORTED: i32 = -2;
