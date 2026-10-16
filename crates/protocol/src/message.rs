//! CBOR messages exchanged between the WSL library and the Windows executable.
//!
//! Field names and types follow upstream (`linux_middleware/src/dll_entry_points.cpp`,
//! `win32_middleware_exe/src/main.cpp`). Fields are declared in lexicographic order, the order
//! upstream's sorted maps use, so encodings match upstream byte for byte.

use std::fmt::Write as _;
use std::io;

use ciborium::Value;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};

/// A request, tagged on the wire by `request_type` with its `request_parameters`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "request_type", content = "request_parameters")]
pub enum Request {
    /// `sk_enroll`: create a credential.
    #[serde(rename = "sk_enroll")]
    Enroll(EnrollParams),
    /// `sk_sign`: sign with an existing credential.
    #[serde(rename = "sk_sign")]
    Sign(SignParams),
}

impl Serialize for Request {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Hand-written so `request_parameters` comes before `request_type`, as upstream.
        let mut map = s.serialize_map(Some(2))?;
        match self {
            Request::Enroll(p) => {
                map.serialize_entry("request_parameters", p)?;
                map.serialize_entry("request_type", "sk_enroll")?;
            }
            Request::Sign(p) => {
                map.serialize_entry("request_parameters", p)?;
                map.serialize_entry("request_type", "sk_sign")?;
            }
        }
        map.end()
    }
}

/// Arguments of OpenSSH's `sk_enroll` (the PIN is never sent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // fields mirror the `sk_enroll` parameters of the same name
pub struct EnrollParams {
    pub alg: u32,
    pub application: String,
    #[serde(with = "serde_bytes")]
    pub challenge: Vec<u8>,
    pub flags: u8,
    pub sk_options: Vec<SkOption>,
}

/// Arguments of OpenSSH's `sk_sign` (the PIN is never sent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // fields mirror the `sk_sign` parameters of the same name
pub struct SignParams {
    pub alg: u32,
    pub application: String,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    pub flags: u8,
    #[serde(with = "serde_bytes")]
    pub key_handle: Vec<u8>,
    pub sk_options: Vec<SkOption>,
}

/// An OpenSSH `sk_option`. `required` is the integer 0 or 1, never a CBOR boolean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // fields mirror `struct sk_option`
pub struct SkOption {
    pub name: String,
    pub required: u8,
    pub value: String,
}

/// A reply; `response_parameters` is present only when `return_code` is 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response<T> {
    /// The result; only on success.
    #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
    pub response_parameters: Option<T>,
    /// `0` on success, otherwise an `SSH_SK_ERR_*` code.
    pub return_code: i32,
}

impl<T> Response<T> {
    /// A successful reply carrying `params`.
    pub fn ok(params: T) -> Self {
        Self {
            response_parameters: Some(params),
            return_code: 0,
        }
    }

    /// A failed reply without parameters.
    pub fn error(return_code: i32) -> Self {
        Self {
            response_parameters: None,
            return_code,
        }
    }
}

/// Result of an enrollment, the wire form of `struct sk_enroll_response` (minus `flags`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // fields mirror `struct sk_enroll_response`
pub struct EnrollResult {
    #[serde(with = "serde_bytes")]
    pub attestation_cert: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub authdata: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub key_handle: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

/// Result of a signature, the wire form of `struct sk_sign_response`.
///
/// `sig_r`/`sig_s` may be absent, but an explicit CBOR null is rejected, as upstream (its
/// `try_at<cbor_byte_string>` throws on a null value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // fields mirror `struct sk_sign_response`
pub struct SignResult {
    pub counter: u32,
    pub flags: u8,
    #[serde(
        default,
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "present_bytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub sig_r: Option<Vec<u8>>,
    #[serde(
        default,
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "present_bytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub sig_s: Option<Vec<u8>>,
}

/// Only called when the key is present, so a null value is a type error instead of `None`.
fn present_bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
    serde_bytes::ByteBuf::deserialize(d).map(|b| Some(b.into_vec()))
}

/// Encodes `value` as CBOR.
pub fn encode<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).map_err(invalid)?;
    Ok(out)
}

/// Decodes one CBOR value; errors are [`io::ErrorKind::InvalidData`].
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    ciborium::from_reader(bytes).map_err(invalid)
}

fn invalid(err: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("CBOR: {err}"))
}

/// `bytes` in upstream's `dump_debug` notation (`b"…"` hex byte strings, `"…"` text with only
/// `"` escaped, `(null)`), so debug logs compare line by line with upstream's.
pub fn diagnostic(bytes: &[u8]) -> String {
    let mut out = String::new();
    match decode::<Value>(bytes) {
        Ok(value) => write_diagnostic(&mut out, &value),
        Err(err) => out.push_str(&format!("<{err}>")),
    }
    out
}

fn write_diagnostic(out: &mut String, value: &Value) {
    match value {
        Value::Integer(i) => _ = write!(out, "{}", i128::from(*i)),
        Value::Bytes(b) => {
            out.push_str("b\"");
            b.iter().for_each(|byte| _ = write!(out, "{byte:02x}"));
            out.push('"');
        }
        Value::Text(t) => _ = write!(out, "\"{}\"", t.replace('"', "\\\"")),
        Value::Null => out.push_str("(null)"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_diagnostic(out, item);
            }
            out.push(']');
        }
        Value::Map(entries) => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_diagnostic(out, k);
                out.push_str(": ");
                write_diagnostic(out, v);
            }
            out.push('}');
        }
        // Types upstream has no counterpart for (booleans, floats, tags).
        other => _ = write!(out, "{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::tests::xorshift;

    fn fixture(hex: &str) -> Vec<u8> {
        hex.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    fn sign_request() -> Vec<u8> {
        fixture(include_str!("../tests/fixtures/sign-request.hex"))
    }

    fn sign_response() -> Vec<u8> {
        fixture(include_str!("../tests/fixtures/sign-response.hex"))
    }

    fn assert_invalid(err: io::Error, needle: &str) {
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
        assert!(err.to_string().contains(needle), "{err}");
    }

    #[test]
    fn sign_request_round_trips() {
        let raw = sign_request();
        let Request::Sign(p) = decode::<Request>(&raw).unwrap() else {
            panic!("expected a sign request");
        };
        assert_eq!(p.alg, 1);
        assert_eq!(p.application, "ssh:5c-github");
        assert_eq!(p.data.len(), 95);
        assert_eq!(p.flags, 33);
        assert_eq!(p.key_handle.len(), 48);
        assert!(p.sk_options.is_empty());
        assert_eq!(encode(&Request::Sign(p)).unwrap(), raw);
    }

    #[test]
    fn sign_response_round_trips() {
        let raw = sign_response();
        let r: Response<SignResult> = decode(&raw).unwrap();
        assert_eq!(r.return_code, 0);
        let p = r.response_parameters.clone().unwrap();
        assert_eq!(p.counter, 689);
        assert_eq!(p.flags, 1);
        assert_eq!(p.sig_r.as_ref().map(Vec::len), Some(64));
        assert_eq!(p.sig_s, None);
        assert_eq!(encode(&r).unwrap(), raw);
    }

    #[test]
    fn enroll_request_round_trips() {
        let req = Request::Enroll(EnrollParams {
            alg: 0,
            application: "ssh:".into(),
            challenge: vec![1, 2, 3],
            flags: 1,
            sk_options: vec![SkOption {
                name: "user".into(),
                required: 1,
                value: "te\"st".into(),
            }],
        });
        let raw = encode(&req).unwrap();
        assert_eq!(decode::<Request>(&raw).unwrap(), req);
        assert_eq!(
            diagnostic(&raw),
            r#"{"request_parameters": {"alg": 0, "application": "ssh:", "challenge": b"010203", "flags": 1, "sk_options": [{"name": "user", "required": 1, "value": "te\"st"}]}, "request_type": "sk_enroll"}"#
        );
    }

    #[test]
    fn diagnostic_matches_upstream_notation() {
        let raw = encode(&Value::Map(vec![
            (Value::Text("n".into()), Value::Null),
            (Value::Text("i".into()), Value::Integer((-3).into())),
        ]))
        .unwrap();
        assert_eq!(diagnostic(&raw), r#"{"n": (null), "i": -3}"#);
        assert!(diagnostic(b"\xff").starts_with('<'));
    }

    /// `Response<EnrollResult>` as upstream's exe encodes it (`win32_middleware_exe/src/main.cpp`).
    /// Expected bytes produced by upstream's own CBOR encoder, see `tests/fixtures/README.md`.
    #[test]
    fn enroll_response_matches_upstream_encoding() {
        let r = Response::ok(EnrollResult {
            attestation_cert: vec![0xce, 0xcf],
            authdata: vec![],
            key_handle: vec![0x09; 3],
            public_key: vec![0x22; 2],
            signature: vec![0x55],
        });
        let expected = fixture(include_str!("../tests/fixtures/enroll-response.hex"));
        assert_eq!(encode(&r).unwrap(), expected);
        assert_eq!(decode::<Response<EnrollResult>>(&expected).unwrap(), r);
        // Upstream's `dump_debug` of the same map, printed by the encoder harness.
        assert_eq!(
            diagnostic(&expected),
            r#"{"response_parameters": {"attestation_cert": b"cecf", "authdata": b"", "key_handle": b"090909", "public_key": b"2222", "signature": b"55"}, "return_code": 0}"#
        );
    }

    #[test]
    fn byte_strings_and_integers_use_upstream_types() {
        let raw = encode(&SkOption {
            name: "n".into(),
            required: 1,
            value: "v".into(),
        })
        .unwrap();
        // {"name": "n", "required": 1, "value": "v"}: required is uint 1 (0x01), not true (0xf5).
        assert_eq!(raw, b"\xa3\x64name\x61n\x68required\x01\x65value\x61v");

        let raw = encode(&SignResult {
            counter: 0,
            flags: 0,
            sig_r: Some(vec![0xaa]),
            sig_s: None,
        })
        .unwrap();
        // sig_r is major type 2 (0x41 = bytes, length 1), not an array (0x81).
        assert!(raw.ends_with(b"\x65sig_r\x41\xaa"));
    }

    #[test]
    fn sig_absent_is_none_but_null_is_rejected() {
        // {"counter": 1, "flags": 1}
        let r: SignResult = decode(b"\xa2\x67counter\x01\x65flags\x01").unwrap();
        assert_eq!((r.sig_r, r.sig_s), (None, None));
        // {"counter": 1, "flags": 1, "sig_r": null}
        let err =
            decode::<SignResult>(b"\xa3\x67counter\x01\x65flags\x01\x65sig_r\xf6").unwrap_err();
        assert_invalid(err, "invalid type: null");
        // ... "sig_s": null
        let err =
            decode::<SignResult>(b"\xa3\x67counter\x01\x65flags\x01\x65sig_s\xf6").unwrap_err();
        assert_invalid(err, "invalid type: null");
    }

    #[test]
    fn failure_response_has_no_parameters() {
        let raw = encode(&Response::<SignResult>::error(-1)).unwrap();
        assert_eq!(raw, b"\xa1\x6breturn_code\x20");
        let r: Response<EnrollResult> = decode(&raw).unwrap();
        assert_eq!(r, Response::error(-1));
    }

    #[test]
    fn unknown_request_type_is_rejected() {
        // The real sign request with only `request_type` changed.
        let Value::Map(mut map) = decode::<Value>(&sign_request()).unwrap() else {
            panic!("expected a map");
        };
        for (k, v) in &mut map {
            if k.as_text() == Some("request_type") {
                *v = Value::Text("sk_nope".into());
            }
        }
        let err = decode::<Request>(&encode(&Value::Map(map)).unwrap()).unwrap_err();
        assert_invalid(err, "unknown variant `sk_nope`");
    }

    #[test]
    fn random_input_never_panics() {
        let mut state = 0x2545_f491_4f6c_dd1d;
        let seeds = [sign_request(), sign_response()];
        for i in 0..5_000 {
            // Alternate random bytes with fixtures that have one byte changed or are truncated.
            let bytes: Vec<u8> = if i % 2 == 0 {
                let len = (xorshift(&mut state) % 64) as usize;
                (0..len).map(|_| xorshift(&mut state) as u8).collect()
            } else {
                let mut b = seeds[i % 4 / 2].clone();
                let at = (xorshift(&mut state) as usize) % b.len();
                if i % 3 == 0 {
                    b.truncate(at);
                } else {
                    b[at] = xorshift(&mut state) as u8;
                }
                b
            };
            _ = decode::<Request>(&bytes);
            _ = decode::<Response<SignResult>>(&bytes);
            _ = decode::<Response<EnrollResult>>(&bytes);
            _ = diagnostic(&bytes);
        }
    }
}
