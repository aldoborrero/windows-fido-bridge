//! `sk-api.h` structs and the conversions between them and Rust values.
//!
//! Everything handed to OpenSSH is allocated with `libc::calloc`, because OpenSSH releases it
//! with `free()`.

use std::ffi::{CStr, c_char};
use std::{io, mem, ptr, slice};

use wfb_protocol::message::{EnrollResult, SignResult, SkOption as WireOption};

#[repr(C)]
pub struct SkEnrollResponse {
    pub flags: u8,
    pub public_key: *mut u8,
    pub public_key_len: usize,
    pub key_handle: *mut u8,
    pub key_handle_len: usize,
    pub signature: *mut u8,
    pub signature_len: usize,
    pub attestation_cert: *mut u8,
    pub attestation_cert_len: usize,
    pub authdata: *mut u8,
    pub authdata_len: usize,
}

#[repr(C)]
pub struct SkSignResponse {
    pub flags: u8,
    pub counter: u32,
    pub sig_r: *mut u8,
    pub sig_r_len: usize,
    pub sig_s: *mut u8,
    pub sig_s_len: usize,
}

#[repr(C)]
pub struct SkOption {
    pub name: *mut c_char,
    pub value: *mut c_char,
    pub required: u8,
}

// The layouts OpenSSH's `sk-api.h` has on x86_64 (the only supported target).
const _: () = {
    use mem::{offset_of, size_of};
    assert!(size_of::<SkEnrollResponse>() == 88);
    assert!(offset_of!(SkEnrollResponse, public_key) == 8);
    assert!(offset_of!(SkEnrollResponse, key_handle) == 24);
    assert!(offset_of!(SkEnrollResponse, signature) == 40);
    assert!(offset_of!(SkEnrollResponse, attestation_cert) == 56);
    assert!(offset_of!(SkEnrollResponse, authdata) == 72);
    assert!(offset_of!(SkEnrollResponse, authdata_len) == 80);
    assert!(size_of::<SkSignResponse>() == 40);
    assert!(offset_of!(SkSignResponse, counter) == 4);
    assert!(offset_of!(SkSignResponse, sig_r) == 8);
    assert!(offset_of!(SkSignResponse, sig_s) == 24);
    assert!(offset_of!(SkSignResponse, sig_s_len) == 32);
    assert!(size_of::<SkOption>() == 24);
    assert!(offset_of!(SkOption, value) == 8);
    assert!(offset_of!(SkOption, required) == 16);
};

/// Opaque: resident keys are unsupported, so the struct is never touched.
#[repr(C)]
pub struct SkResidentKey {
    _private: [u8; 0],
}

/// NULL becomes the empty string; invalid UTF-8 is an error (the wire format carries CBOR text,
/// which must be UTF-8).
///
/// # Safety
/// `p` is NULL or points to a NUL-terminated string valid for the duration of the call.
pub unsafe fn c_str_or_empty(p: *const c_char) -> io::Result<String> {
    if p.is_null() {
        return Ok(String::new());
    }
    // SAFETY: non-null and NUL-terminated per the caller contract.
    let s = unsafe { CStr::from_ptr(p) };
    s.to_str().map(str::to_owned).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("string is not UTF-8: {e}"),
        )
    })
}

/// Zero length becomes the empty slice (the pointer may then be NULL); NULL with a non-zero
/// length is an error.
///
/// # Safety
/// If `p` is non-null and `len > 0`, `p` points to `len` readable bytes that outlive `'a`.
pub unsafe fn slice_or_empty<'a>(p: *const u8, len: usize) -> io::Result<&'a [u8]> {
    if len == 0 {
        return Ok(&[]);
    }
    if p.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NULL buffer with non-zero length",
        ));
    }
    // SAFETY: non-null, `len` readable bytes per the caller contract; u8 has alignment 1.
    Ok(unsafe { slice::from_raw_parts(p, len) })
}

/// Reads OpenSSH's NULL-terminated `sk_option*` array; a NULL array means no options.
///
/// # Safety
/// `options` is NULL or a NULL-terminated array of pointers to valid `sk_option`s whose
/// strings satisfy [`c_str_or_empty`].
pub unsafe fn parse_options(options: *const *const SkOption) -> io::Result<Vec<WireOption>> {
    let mut parsed = Vec::new();
    if options.is_null() {
        return Ok(parsed);
    }
    for i in 0.. {
        // SAFETY: the array is NULL-terminated, so every index up to the terminator is in bounds.
        let option = unsafe { *options.add(i) };
        if option.is_null() {
            break;
        }
        // SAFETY: non-null entries point to valid `sk_option`s per the caller contract.
        let option = unsafe { &*option };
        parsed.push(WireOption {
            // SAFETY: the option's strings are NULL or NUL-terminated per the caller contract.
            name: unsafe { c_str_or_empty(option.name) }?,
            // SAFETY: as above.
            value: unsafe { c_str_or_empty(option.value) }?,
            required: u8::from(option.required != 0),
        });
    }
    Ok(parsed)
}

/// A `calloc`ed copy of some bytes, freed on drop unless released with [`CBuf::into_raw`].
/// Empty input is represented as NULL with length 0.
pub struct CBuf {
    ptr: *mut u8,
    len: usize,
}

impl CBuf {
    pub fn new(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty() {
            return Ok(Self {
                ptr: ptr::null_mut(),
                len: 0,
            });
        }
        // SAFETY: calloc has no preconditions; the result is checked for NULL below.
        let ptr = unsafe { libc::calloc(1, bytes.len()) }.cast::<u8>();
        if ptr.is_null() {
            return Err(io::Error::from(io::ErrorKind::OutOfMemory));
        }
        // SAFETY: `ptr` is a fresh allocation of `bytes.len()` bytes that cannot overlap `bytes`.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
        Ok(Self {
            ptr,
            len: bytes.len(),
        })
    }

    /// Hands the buffer over; the receiver must `free()` it.
    pub fn into_raw(self) -> (*mut u8, usize) {
        let raw = (self.ptr, self.len);
        mem::forget(self);
        raw
    }
}

impl Drop for CBuf {
    fn drop(&mut self) {
        // SAFETY: `ptr` is NULL or came from `calloc` and has not been released.
        unsafe { libc::free(self.ptr.cast()) };
    }
}

/// `calloc`s a `T` and moves `value` into it. Called last, after every buffer is allocated, so
/// nothing can fail after it and no guard is needed for the struct itself.
fn calloc_struct<T>(value: T) -> io::Result<*mut T> {
    // SAFETY: calloc has no preconditions; the result is checked for NULL below.
    let ptr = unsafe { libc::calloc(1, mem::size_of::<T>()) }.cast::<T>();
    if ptr.is_null() {
        return Err(io::Error::from(io::ErrorKind::OutOfMemory));
    }
    // SAFETY: calloc returns memory suitably aligned for any C type and large enough for `T`.
    unsafe { ptr.write(value) };
    Ok(ptr)
}

/// Builds an OpenSSH-owned `sk_enroll_response`. `flags` are the request flags, which OpenSSH
/// stores as the key's `sk_flags` (as `sk-usbhid.c` does; upstream left them 0, see research R10).
pub fn enroll_response(r: &EnrollResult, flags: u8) -> io::Result<*mut SkEnrollResponse> {
    let public_key = CBuf::new(&r.public_key)?;
    let key_handle = CBuf::new(&r.key_handle)?;
    let signature = CBuf::new(&r.signature)?;
    let attestation_cert = CBuf::new(&r.attestation_cert)?;
    let authdata = CBuf::new(&r.authdata)?;
    // Allocate the struct before releasing the buffers so a failure here still frees them.
    let out = calloc_struct(SkEnrollResponse {
        flags,
        public_key: ptr::null_mut(),
        public_key_len: 0,
        key_handle: ptr::null_mut(),
        key_handle_len: 0,
        signature: ptr::null_mut(),
        signature_len: 0,
        attestation_cert: ptr::null_mut(),
        attestation_cert_len: 0,
        authdata: ptr::null_mut(),
        authdata_len: 0,
    })?;
    // SAFETY: `out` was just allocated and initialised by `calloc_struct` and is not shared.
    let resp = unsafe { &mut *out };
    (resp.public_key, resp.public_key_len) = public_key.into_raw();
    (resp.key_handle, resp.key_handle_len) = key_handle.into_raw();
    (resp.signature, resp.signature_len) = signature.into_raw();
    (resp.attestation_cert, resp.attestation_cert_len) = attestation_cert.into_raw();
    (resp.authdata, resp.authdata_len) = authdata.into_raw();
    Ok(out)
}

/// Builds an OpenSSH-owned `sk_sign_response`; a success without `sig_r` is an error.
pub fn sign_response(r: &SignResult) -> io::Result<*mut SkSignResponse> {
    let sig_r = r
        .sig_r
        .as_deref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bridge reply has no sig_r"))?;
    let sig_r = CBuf::new(sig_r)?;
    let sig_s = CBuf::new(r.sig_s.as_deref().unwrap_or_default())?;
    let out = calloc_struct(SkSignResponse {
        flags: r.flags,
        counter: r.counter,
        sig_r: ptr::null_mut(),
        sig_r_len: 0,
        sig_s: ptr::null_mut(),
        sig_s_len: 0,
    })?;
    // SAFETY: `out` was just allocated and initialised by `calloc_struct` and is not shared.
    let resp = unsafe { &mut *out };
    (resp.sig_r, resp.sig_r_len) = sig_r.into_raw();
    (resp.sig_s, resp.sig_s_len) = sig_s.into_raw();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Releases a response the way OpenSSH does: every buffer, then the struct.
    unsafe fn free_all(bufs: &[*mut u8], st: *mut libc::c_void) {
        for &b in bufs {
            // SAFETY: test-only; buffers come from `CBuf::into_raw`.
            unsafe { libc::free(b.cast()) };
        }
        // SAFETY: test-only; the struct comes from `calloc_struct`.
        unsafe { libc::free(st) };
    }

    #[test]
    fn parses_null_terminated_options() {
        let names = [
            CString::new("user").unwrap(),
            CString::new("device").unwrap(),
        ];
        let values = [
            CString::new("test").unwrap(),
            CString::new("/dev/x").unwrap(),
        ];
        let opts: Vec<SkOption> = (0..2)
            .map(|i| SkOption {
                name: names[i].as_ptr().cast_mut(),
                value: values[i].as_ptr().cast_mut(),
                required: [1, 0][i],
            })
            .collect();
        let array = [&opts[0] as *const SkOption, &opts[1], ptr::null()];
        // SAFETY: NULL-terminated array of valid options backed by live CStrings.
        let parsed = unsafe { parse_options(array.as_ptr()) }.unwrap();
        assert_eq!(
            parsed,
            vec![
                WireOption {
                    name: "user".into(),
                    required: 1,
                    value: "test".into()
                },
                WireOption {
                    name: "device".into(),
                    required: 0,
                    value: "/dev/x".into()
                },
            ]
        );
    }

    #[test]
    fn null_inputs_are_empty() {
        // SAFETY: NULL is explicitly allowed by each helper.
        unsafe {
            assert!(parse_options(ptr::null()).unwrap().is_empty());
            assert_eq!(c_str_or_empty(ptr::null()).unwrap(), "");
            assert!(slice_or_empty(ptr::null(), 0).unwrap().is_empty());
            assert!(slice_or_empty(ptr::null(), 5).is_err());
        }
        let opt = SkOption {
            name: ptr::null_mut(),
            value: ptr::null_mut(),
            required: 7,
        };
        let array = [&opt as *const SkOption, ptr::null()];
        // SAFETY: NULL-terminated array; NULL strings are allowed.
        let parsed = unsafe { parse_options(array.as_ptr()) }.unwrap();
        assert_eq!(parsed[0].name, "");
        assert_eq!(parsed[0].required, 1);
    }

    #[test]
    fn invalid_utf8_strings_are_rejected() {
        let bad = c"us\xffer";
        // SAFETY: a valid NUL-terminated string.
        let err = unsafe { c_str_or_empty(bad.as_ptr()) }.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().starts_with("string is not UTF-8"), "{err}");
        let opt = SkOption {
            name: c"user".as_ptr().cast_mut(),
            value: bad.as_ptr().cast_mut(),
            required: 0,
        };
        let array = [&opt as *const SkOption, ptr::null()];
        // SAFETY: NULL-terminated array of a valid option backed by static strings.
        assert!(unsafe { parse_options(array.as_ptr()) }.is_err());
    }

    #[test]
    fn cbuf_frees_on_drop_and_copies() {
        drop(CBuf::new(b"dropped without leaking").unwrap());
        let (p, len) = CBuf::new(b"abc").unwrap().into_raw();
        // SAFETY: `p` points to `len` bytes from `into_raw`.
        assert_eq!(unsafe { slice::from_raw_parts(p, len) }, b"abc");
        // SAFETY: released by `into_raw`, now ours to free.
        unsafe { libc::free(p.cast()) };
        assert_eq!(CBuf::new(&[]).unwrap().into_raw(), (ptr::null_mut(), 0));
    }

    #[test]
    fn sign_response_ed25519() {
        let sig = vec![0x5a; 64];
        let out = sign_response(&SignResult {
            counter: 689,
            flags: 1,
            sig_r: Some(sig.clone()),
            sig_s: None,
        })
        .unwrap();
        // SAFETY: `out` is a valid response we own.
        let r = unsafe { &*out };
        assert_eq!((r.flags, r.counter), (1, 689));
        // SAFETY: `sig_r` holds `sig_r_len` bytes.
        assert_eq!(unsafe { slice::from_raw_parts(r.sig_r, r.sig_r_len) }, sig);
        assert!(r.sig_s.is_null());
        assert_eq!(r.sig_s_len, 0);
        // SAFETY: everything came from calloc and is released once.
        unsafe { free_all(&[r.sig_r, r.sig_s], out.cast()) };
    }

    #[test]
    fn enroll_response_with_empty_cert_and_authdata() {
        let out = enroll_response(
            &EnrollResult {
                attestation_cert: vec![],
                authdata: vec![],
                key_handle: vec![1; 48],
                public_key: vec![2; 32],
                signature: vec![3; 70],
            },
            0x21,
        )
        .unwrap();
        // SAFETY: `out` is a valid response we own.
        let r = unsafe { &*out };
        // The request flags come back: OpenSSH stores them as the key's sk_flags.
        assert_eq!(r.flags, 0x21);
        assert!(r.attestation_cert.is_null() && r.attestation_cert_len == 0);
        assert!(r.authdata.is_null() && r.authdata_len == 0);
        // SAFETY: each pointer holds its length in bytes.
        unsafe {
            assert_eq!(
                slice::from_raw_parts(r.key_handle, r.key_handle_len),
                [1; 48]
            );
            assert_eq!(
                slice::from_raw_parts(r.public_key, r.public_key_len),
                [2; 32]
            );
            assert_eq!(slice::from_raw_parts(r.signature, r.signature_len), [3; 70]);
        }
        // SAFETY: everything came from calloc and is released once.
        unsafe {
            free_all(
                &[
                    r.public_key,
                    r.key_handle,
                    r.signature,
                    r.attestation_cert,
                    r.authdata,
                ],
                out.cast(),
            )
        };
    }
}
