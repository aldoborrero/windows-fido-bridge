//! OpenSSH security key middleware (`SSH_SK_PROVIDER`).
//!
//! Implements the C ABI from OpenSSH's `sk-api.h` and forwards each request to
//! `windowsfidobridge.exe`, which must sit next to this library.

mod bridge;
mod ffi;

use std::ffi::{c_char, c_int};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ffi::{SkEnrollResponse, SkOption, SkResidentKey, SkSignResponse};
use serde::de::DeserializeOwned;
use wfb_protocol::message::{self, EnrollParams, Request, Response, SignParams, SkOption as Opt};
use wfb_protocol::sk::{SSH_SK_ERR_GENERAL, SSH_SK_ERR_UNSUPPORTED, SSH_SK_VERSION_MAJOR};
use wfb_protocol::{debug, error, log};

const LOG_NAME: &str = "linux-middleware";

#[unsafe(no_mangle)]
pub extern "C" fn sk_api_version() -> u32 {
    SSH_SK_VERSION_MAJOR
}

/// # Safety
/// Called by OpenSSH with the `sk-api.h` contract: `challenge` holds `challenge_len` bytes (or
/// is NULL with length 0), strings are NULL or NUL-terminated, `options` is NULL or a
/// NULL-terminated array, and `enroll_response` is a valid out-pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_enroll(
    alg: u32,
    challenge: *const u8,
    challenge_len: usize,
    application: *const c_char,
    flags: u8,
    pin: *const c_char,
    options: *const *const SkOption,
    enroll_response: *mut *mut SkEnrollResponse,
) -> c_int {
    guard(|| {
        if enroll_response.is_null() {
            return Err(io::Error::other("enroll_response is NULL"));
        }
        // SAFETY: pointer arguments follow the sk-api contract stated above.
        let (params, pin_present) = unsafe {
            let params = EnrollParams {
                alg,
                application: ffi::c_str_or_empty(application)?,
                challenge: ffi::slice_or_empty(challenge, challenge_len)?.to_vec(),
                flags,
                sk_options: ffi::parse_options(options)?,
            };
            (params, !pin.is_null() && *pin != 0)
        };
        debug!("Parameters from OpenSSH:");
        debug!("    Algorithm: {alg}");
        debug!(
            "    Challenge:\n{}",
            log::hex_dump(&params.challenge, "      | ")
        );
        debug!("    Application: \"{}\"", params.application);
        log_common(flags, pin_present, &params.sk_options);

        let Some(result) = call(&Request::Enroll(params))? else {
            return Ok(SSH_SK_ERR_GENERAL);
        };
        let out = ffi::enroll_response(&result, flags)?;
        // SAFETY: non-null out-pointer from OpenSSH; ownership of `out` moves to OpenSSH.
        unsafe { *enroll_response = out };
        Ok(0)
    })
}

/// # Safety
/// Called by OpenSSH with the `sk-api.h` contract: `data` and `key_handle` hold their lengths
/// in bytes (or are NULL with length 0), strings are NULL or NUL-terminated, `options` is NULL
/// or a NULL-terminated array, and `sign_response` is a valid out-pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_sign(
    alg: u32,
    data: *const u8,
    data_len: usize,
    application: *const c_char,
    key_handle: *const u8,
    key_handle_len: usize,
    flags: u8,
    pin: *const c_char,
    options: *const *const SkOption,
    sign_response: *mut *mut SkSignResponse,
) -> c_int {
    guard(|| {
        if sign_response.is_null() {
            return Err(io::Error::other("sign_response is NULL"));
        }
        // SAFETY: pointer arguments follow the sk-api contract stated above.
        let (params, pin_present) = unsafe {
            let params = SignParams {
                alg,
                application: ffi::c_str_or_empty(application)?,
                data: ffi::slice_or_empty(data, data_len)?.to_vec(),
                flags,
                key_handle: ffi::slice_or_empty(key_handle, key_handle_len)?.to_vec(),
                sk_options: ffi::parse_options(options)?,
            };
            (params, !pin.is_null() && *pin != 0)
        };
        debug!("Parameters from OpenSSH:");
        debug!("    Algorithm: {alg}");
        debug!("    Data:\n{}", log::hex_dump(&params.data, "      | "));
        debug!("    Application: \"{}\"", params.application);
        debug!(
            "    Key handle:\n{}",
            log::hex_dump(&params.key_handle, "      | ")
        );
        log_common(flags, pin_present, &params.sk_options);

        let Some(result) = call(&Request::Sign(params))? else {
            return Ok(SSH_SK_ERR_GENERAL);
        };
        let out = ffi::sign_response(&result)?;
        // SAFETY: non-null out-pointer from OpenSSH; ownership of `out` moves to OpenSSH.
        unsafe { *sign_response = out };
        Ok(0)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn sk_load_resident_keys(
    _pin: *const c_char,
    _options: *const *const SkOption,
    _rks: *mut *mut *mut SkResidentKey,
    _nrks: *mut usize,
) -> c_int {
    SSH_SK_ERR_UNSUPPORTED
}

/// Runs an entry point body; errors and panics become `SSH_SK_ERR_GENERAL`.
fn guard(body: impl FnOnce() -> io::Result<c_int>) -> c_int {
    log::init(LOG_NAME);
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => {
            error!("{e}");
            SSH_SK_ERR_GENERAL
        }
        Err(_) => {
            error!("unexpected panic");
            SSH_SK_ERR_GENERAL
        }
    }
}

/// Sends a request to the Windows side and interprets the reply with [`parse_reply`].
fn call<T: DeserializeOwned>(request: &Request) -> io::Result<Option<T>> {
    let raw = message::encode(request)?;
    debug!("Sending CBOR to bridge: {}", message::diagnostic(&raw));
    let reply = bridge::invoke(&raw)?;
    debug!("Received CBOR from bridge: {}", message::diagnostic(&reply));
    parse_reply(&reply)
}

/// The parameters of a bridge reply. `None` means a non-zero return code, which OpenSSH sees as a
/// general error (upstream parity); a malformed reply is an error.
fn parse_reply<T: DeserializeOwned>(reply: &[u8]) -> io::Result<Option<T>> {
    let response: Response<T> = message::decode(reply)?;
    debug!("Bridge return code: {}", response.return_code);
    match (response.return_code, response.response_parameters) {
        (0, Some(params)) => Ok(Some(params)),
        (0, None) => Err(io::Error::other("bridge reply has no response_parameters")),
        _ => Ok(None),
    }
}

/// Logs the inputs shared by enroll and sign; the PIN itself is never logged.
fn log_common(flags: u8, pin_present: bool, options: &[Opt]) {
    debug!("    Flags: 0b{flags:08b}");
    let pin = if pin_present {
        "(present)"
    } else {
        "(not present)"
    };
    debug!("    PIN: {pin}");
    debug!("    Options:");
    if options.is_empty() {
        debug!("        (No options provided)");
    }
    for o in options {
        debug!(
            "        * \"{}\" = \"{}\" (required = {})",
            o.name,
            o.value,
            o.required != 0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use wfb_protocol::message::SignResult;

    #[test]
    fn reports_api_v10() {
        assert_eq!(sk_api_version(), 0x000a_0000);
    }

    #[test]
    fn resident_keys_unsupported() {
        let code =
            sk_load_resident_keys(ptr::null(), ptr::null(), ptr::null_mut(), ptr::null_mut());
        assert_eq!(code, -2);
    }

    #[test]
    fn guard_maps_errors_and_panics() {
        assert_eq!(guard(|| Ok(0)), 0);
        assert_eq!(guard(|| Err(io::Error::other("boom"))), -1);
        assert_eq!(guard(|| panic!("boom")), -1);
    }

    #[test]
    fn sign_without_bridge_fails_cleanly() {
        // The test binary has no windowsfidobridge.exe next to it.
        let mut out: *mut SkSignResponse = ptr::null_mut();
        // SAFETY: NULL inputs with zero lengths are allowed; `out` is a valid out-pointer.
        let code = unsafe {
            sk_sign(
                1,
                ptr::null(),
                0,
                ptr::null(),
                ptr::null(),
                0,
                0,
                ptr::null(),
                ptr::null(),
                &mut out,
            )
        };
        assert_eq!(code, -1);
        assert!(out.is_null());
    }

    fn unhex(hex: &str) -> Vec<u8> {
        hex.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    fn assert_err(err: io::Error, kind: io::ErrorKind, needle: &str) {
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.to_string().contains(needle), "{err}");
    }

    fn sign_result(counter: u32, sig_r: Option<Vec<u8>>) -> SignResult {
        SignResult {
            counter,
            flags: 1,
            sig_r,
            sig_s: None,
        }
    }

    #[test]
    fn reply_from_upstream_fixture_reaches_openssh() {
        let raw = unhex(include_str!(
            "../../protocol/tests/fixtures/sign-response.hex"
        ));
        let result = parse_reply::<SignResult>(&raw).unwrap().unwrap();
        let out = ffi::sign_response(&result).unwrap();
        // SAFETY: `out` is a valid response we own; its buffers hold their lengths.
        unsafe {
            let r = &*out;
            assert_eq!((r.flags, r.counter, r.sig_r_len), (1, 689, 64));
            assert_eq!(
                std::slice::from_raw_parts(r.sig_r, 4),
                [0x1a, 0x7b, 0x21, 0x11]
            );
            assert!(r.sig_s.is_null() && r.sig_s_len == 0);
            libc::free(r.sig_r.cast());
            libc::free(out.cast());
        }
    }

    #[test]
    fn reply_with_error_code_is_a_general_error() {
        // {"return_code": -2}, as upstream sends for an unsupported algorithm.
        let raw = b"\xa1\x6breturn_code\x21";
        assert_eq!(parse_reply::<SignResult>(raw).unwrap(), None);
        // Parameters next to a non-zero code are ignored, as upstream.
        let raw = message::encode(&Response {
            response_parameters: Some(sign_result(5, Some(vec![1; 64]))),
            return_code: -1,
        })
        .unwrap();
        assert_eq!(parse_reply::<SignResult>(&raw).unwrap(), None);
    }

    #[test]
    fn reply_success_without_parameters_is_an_error() {
        let raw = message::encode(&Response::<SignResult>::error(0)).unwrap();
        let err = parse_reply::<SignResult>(&raw).unwrap_err();
        assert_err(err, io::ErrorKind::Other, "no response_parameters");
    }

    #[test]
    fn reply_with_null_or_missing_sig_r_is_an_error() {
        // {"response_parameters": {"counter": 1, "flags": 1, "sig_r": null}, "return_code": 0}
        let mut raw = b"\xa2\x73response_parameters\xa3\x67counter\x01\x65flags\x01".to_vec();
        raw.extend(b"\x65sig_r\xf6\x6breturn_code\x00");
        let err = parse_reply::<SignResult>(&raw).unwrap_err();
        assert_err(err, io::ErrorKind::InvalidData, "invalid type: null");

        let raw = message::encode(&Response::ok(sign_result(1, None))).unwrap();
        let result = parse_reply::<SignResult>(&raw).unwrap().unwrap();
        let err = ffi::sign_response(&result).unwrap_err();
        assert_err(err, io::ErrorKind::InvalidData, "no sig_r");
    }

    #[test]
    fn sign_with_invalid_utf8_application_fails() {
        let mut out: *mut SkSignResponse = ptr::null_mut();
        // SAFETY: valid NUL-terminated (non-UTF-8) string; `out` is a valid out-pointer.
        let code = unsafe {
            sk_sign(
                1,
                ptr::null(),
                0,
                c"ssh:\xff".as_ptr(),
                ptr::null(),
                0,
                0,
                ptr::null(),
                ptr::null(),
                &mut out,
            )
        };
        assert_eq!(code, -1);
        assert!(out.is_null());
    }
}
