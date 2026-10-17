//! Thin wrappers over `webauthn.dll` (upstream `win32_middleware_common/src/webauthn.cpp`).

use std::{io, ptr, slice};

use wfb_protocol::sk::{SSH_SK_ECDSA, SSH_SK_ED25519};
use windows_sys::Win32::Foundation::{HWND, S_OK};
use windows_sys::Win32::Networking::WindowsWebServices::*;
use windows_sys::core::{HRESULT, PCWSTR, w};

/// Not exported by windows-sys; value from the IANA COSE registry.
const WEBAUTHN_COSE_ALGORITHM_EDDSA_ED25519: i32 = -8;

/// Option struct version upstream passes for both calls; newer fields stay zero.
const OPTIONS_VERSION: u32 = 3;

/// OpenSSH does not provide a user id, so upstream uses this placeholder.
const USER_ID: &[u8] = b"(null)";

pub struct Assertion {
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
}

pub fn get_assertion(
    hwnd: HWND,
    application: &str,
    data: &[u8],
    key_handle: &[u8],
    uv_required: bool,
) -> io::Result<Assertion> {
    let rp_id = wide(application);
    // The API takes mutable pointers, so pass copies.
    let mut data = data.to_vec();
    let mut key_handle = key_handle.to_vec();
    let client_data = client_data(&mut data)?;
    let mut credential = WEBAUTHN_CREDENTIAL {
        dwVersion: WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
        cbId: len32(&key_handle)?,
        pbId: key_handle.as_mut_ptr(),
        pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
    };
    let options = WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS {
        dwVersion: OPTIONS_VERSION,
        CredentialList: WEBAUTHN_CREDENTIALS {
            cCredentials: 1,
            pCredentials: &mut credential,
        },
        dwUserVerificationRequirement: uv_requirement(uv_required),
        ..Default::default()
    };
    let mut assertion: *mut WEBAUTHN_ASSERTION = ptr::null_mut();
    // SAFETY: every pointer refers to a local that outlives the call; `assertion` is a valid
    // out-parameter.
    let hr = unsafe {
        WebAuthNAuthenticatorGetAssertion(
            hwnd,
            rp_id.as_ptr(),
            &client_data,
            &options,
            &mut assertion,
        )
    };
    if hr != S_OK || assertion.is_null() {
        if !assertion.is_null() {
            // SAFETY: returned by WebAuthNAuthenticatorGetAssertion and freed once.
            unsafe { WebAuthNFreeAssertion(assertion) };
        }
        return Err(webauthn_error(hr, "Failed to get WebAuthN assertion"));
    }
    // SAFETY: on S_OK `assertion` points to a valid WEBAUTHN_ASSERTION whose buffers hold the
    // stated lengths; we copy them out before freeing it once.
    unsafe {
        let a = &*assertion;
        let result = Assertion {
            authenticator_data: copy(a.pbAuthenticatorData, a.cbAuthenticatorData),
            signature: copy(a.pbSignature, a.cbSignature),
        };
        WebAuthNFreeAssertion(assertion);
        Ok(result)
    }
}

/// Returns the CBOR attestation object.
pub fn make_credential(
    hwnd: HWND,
    alg: u32,
    application: &str,
    user: &str,
    challenge: &[u8],
    uv_required: bool,
) -> io::Result<Vec<u8>> {
    let cose_alg = match alg {
        SSH_SK_ECDSA => WEBAUTHN_COSE_ALGORITHM_ECDSA_P256_WITH_SHA256,
        SSH_SK_ED25519 => WEBAUTHN_COSE_ALGORITHM_EDDSA_ED25519,
        _ => {
            return Err(io::Error::other(format!(
                "Unrecognized OpenSSH algorithm {alg}"
            )));
        }
    };
    let rp_id = wide(application);
    let user = wide(user);
    let mut user_id = USER_ID.to_vec();
    let mut challenge = challenge.to_vec();
    let client_data = client_data(&mut challenge)?;
    let rp = WEBAUTHN_RP_ENTITY_INFORMATION {
        dwVersion: WEBAUTHN_RP_ENTITY_INFORMATION_CURRENT_VERSION,
        pwszId: rp_id.as_ptr(),
        pwszName: w!("OpenSSH via windows-fido-bridge"),
        pwszIcon: ptr::null(),
    };
    let user_info = WEBAUTHN_USER_ENTITY_INFORMATION {
        dwVersion: WEBAUTHN_USER_ENTITY_INFORMATION_CURRENT_VERSION,
        cbId: len32(&user_id)?,
        pbId: user_id.as_mut_ptr(),
        pwszName: user.as_ptr(),
        pwszIcon: ptr::null(),
        pwszDisplayName: user.as_ptr(),
    };
    let mut param = WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
        dwVersion: WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
        pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
        lAlg: cose_alg,
    };
    let params = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS {
        cCredentialParameters: 1,
        pCredentialParameters: &mut param,
    };
    let options = WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS {
        dwVersion: OPTIONS_VERSION,
        dwUserVerificationRequirement: uv_requirement(uv_required),
        // Only roaming security keys: otherwise Windows offers to save a platform passkey
        // (Windows Hello), which is not an `-sk` key and whose attestation we reject.
        dwAuthenticatorAttachment: WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
        // Keep the authenticator's own attestation ("packed"/"fido-u2f") instead of "none".
        dwAttestationConveyancePreference: WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_DIRECT,
        ..Default::default()
    };
    let mut attestation: *mut WEBAUTHN_CREDENTIAL_ATTESTATION = ptr::null_mut();
    // SAFETY: every pointer refers to a local that outlives the call; `attestation` is a valid
    // out-parameter.
    let hr = unsafe {
        WebAuthNAuthenticatorMakeCredential(
            hwnd,
            &rp,
            &user_info,
            &params,
            &client_data,
            &options,
            &mut attestation,
        )
    };
    if hr != S_OK || attestation.is_null() {
        if !attestation.is_null() {
            // SAFETY: returned by WebAuthNAuthenticatorMakeCredential and freed once.
            unsafe { WebAuthNFreeCredentialAttestation(attestation) };
        }
        return Err(webauthn_error(hr, "Failed to make WebAuthN credential"));
    }
    // SAFETY: on S_OK `attestation` is valid and its attestation object buffer holds the stated
    // length; we copy it out before freeing it once.
    unsafe {
        let a = &*attestation;
        let object = copy(a.pbAttestationObject, a.cbAttestationObject);
        WebAuthNFreeCredentialAttestation(attestation);
        Ok(object)
    }
}

/// Client data is the raw OpenSSH challenge/message, which Windows hashes with SHA-256.
fn client_data(bytes: &mut [u8]) -> io::Result<WEBAUTHN_CLIENT_DATA> {
    Ok(WEBAUTHN_CLIENT_DATA {
        dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
        cbClientDataJSON: len32(bytes)?,
        pbClientDataJSON: bytes.as_mut_ptr(),
        pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
    })
}

fn uv_requirement(required: bool) -> u32 {
    if required {
        WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED
    } else {
        WEBAUTHN_USER_VERIFICATION_REQUIREMENT_DISCOURAGED
    }
}

fn len32(bytes: &[u8]) -> io::Result<u32> {
    u32::try_from(bytes.len()).map_err(|_| io::Error::other("buffer too large"))
}

/// NUL-terminated UTF-16.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// # Safety
/// `p` is NULL or points to `len` readable bytes.
unsafe fn copy(p: *const u8, len: u32) -> Vec<u8> {
    if p.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: per the function contract.
    unsafe { slice::from_raw_parts(p, len as usize) }.to_vec()
}

fn webauthn_error(hr: HRESULT, what: &str) -> io::Error {
    // SAFETY: WebAuthNGetErrorName returns a static NUL-terminated string (or NULL).
    let name = unsafe { from_wide(WebAuthNGetErrorName(hr)) };
    io::Error::other(format!("{what} ({name}, HRESULT {hr:#010x})"))
}

/// # Safety
/// `p` is NULL or a NUL-terminated UTF-16 string.
unsafe fn from_wide(p: PCWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: the string is NUL-terminated, so every index up to the terminator is readable.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` code units were just read.
    String::from_utf16_lossy(unsafe { slice::from_raw_parts(p, len) })
}
