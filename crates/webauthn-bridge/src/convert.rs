//! WebAuthn → OpenSSH conversions (upstream `win32_middleware_common/src/webauthn.cpp` and
//! `openssh_sk_middleware.cpp`). Platform-independent so it is unit-tested on Linux.
#![deny(unsafe_code)]
// Outside Windows only the tests call into this module.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fmt::Write as _;
use std::io;

use ciborium::Value;
use wfb_protocol::debug;
use wfb_protocol::message::{EnrollResult, SignResult, SkOption};
use wfb_protocol::sk::{SSH_SK_ECDSA, SSH_SK_ED25519, SSH_SK_USER_VERIFICATION_REQD};

/// Application that makes the bridge require user verification without OpenSSH knowing.
const VERIFY_REQUIRED_APPLICATION: &str = "ssh:windows-fido-bridge-verify-required";

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;
const FLAG_ED: u8 = 0x80;

/// Authenticator data, WebAuthn §6.1.
#[derive(Debug, PartialEq, Eq)]
pub struct AuthenticatorData {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub sign_count: u32,
    pub attested_credential: Option<AttestedCredential>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AttestedCredential {
    pub aaguid: [u8; 16],
    pub id: Vec<u8>,
    /// The COSE public key, possibly followed by extension data (only the first CBOR item is
    /// read).
    pub cose_key: Vec<u8>,
}

impl AuthenticatorData {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut r = Reader(bytes);
        let rp_id_hash = r.array()?;
        let [flags] = r.array()?;
        let sign_count = u32::from_be_bytes(r.array()?);
        let attested_credential = if flags & FLAG_AT != 0 {
            let aaguid = r.array()?;
            let id_len = u16::from_be_bytes(r.array()?);
            let id = r.take(usize::from(id_len))?.to_vec();
            Some(AttestedCredential {
                aaguid,
                id,
                cose_key: r.0.to_vec(),
            })
        } else {
            None
        };
        Ok(Self {
            rp_id_hash,
            flags,
            sign_count,
            attested_credential,
        })
    }

    /// Multi-line description for debug logs (upstream `authenticator_data::dump_debug`).
    pub fn describe(&self) -> String {
        let flag = |bit: u8| self.flags & bit != 0;
        let mut s = format!("Relying party ID hash: 0x{}\n", hex(&self.rp_id_hash));
        _ = writeln!(s, "Flags: 0b{:08b}", self.flags);
        _ = writeln!(s, "    User present result: {}", flag(FLAG_UP));
        _ = writeln!(s, "    User verified result: {}", flag(FLAG_UV));
        _ = writeln!(
            s,
            "    Attested credential data included: {}",
            flag(FLAG_AT)
        );
        _ = writeln!(s, "    Extension data included: {}", flag(FLAG_ED));
        _ = writeln!(s, "Signature count: {}", self.sign_count);
        match &self.attested_credential {
            Some(c) => {
                _ = writeln!(s, "Authenticator attestation GUID: {}", hex(&c.aaguid));
                _ = write!(s, "Credential ID ({} bytes): 0x{}", c.id.len(), hex(&c.id));
            }
            None => s.push_str("No attested credential data included"),
        }
        s
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(invalid("authenticator data is truncated"));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
}

/// Upstream `is_user_verification_required`.
pub fn user_verification_required(application: &str, flags: u8, force_env: bool) -> bool {
    force_env
        || application == VERIFY_REQUIRED_APPLICATION
        || flags & SSH_SK_USER_VERIFICATION_REQD != 0
}

/// The `user` option, or "SSH user" (upstream `sk_enroll_safe`).
pub fn enroll_user_name(options: &[SkOption]) -> &str {
    options
        .iter()
        .find(|o| o.name == "user")
        .map_or("SSH user", |o| o.value.as_str())
}

/// Ed25519 signatures go to OpenSSH unchanged as `sig_r`.
pub fn ed25519_signature(sig: &[u8]) -> io::Result<Vec<u8>> {
    // Upstream reads the first 64 bytes and ignores anything after them.
    sig.get(..64)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| invalid("Ed25519 signature is shorter than 64 bytes"))
}

/// Splits a DER `SEQUENCE { INTEGER r, INTEGER s }` into 32-byte r and s
/// (upstream `parse_ecdsa_sk_signature`, which hardcodes the format rather than parsing ASN.1).
/// Unlike upstream, INTEGERs shorter than 32 bytes (about 1 signature in 128) are accepted and
/// left-padded instead of failing the signature, and the DER is checked strictly: lengths match
/// the content, no trailing bytes, minimal positive non-zero INTEGERs of at most 256 bits.
pub fn ecdsa_signature(sig: &[u8]) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let bad = || invalid("invalid or unrecognized signature received from authenticator");
    let [0x30, len, content @ ..] = sig else {
        return Err(bad());
    };
    if usize::from(*len) != content.len() {
        return Err(bad());
    }
    let mut r = Reader(content);
    let mut integer = || -> io::Result<Vec<u8>> {
        let [0x02, len] = r.array().map_err(|_| bad())? else {
            return Err(bad());
        };
        let value = match r.take(usize::from(len)).map_err(|_| bad())? {
            // A leading zero only to keep the top bit from reading as a sign bit.
            [0, rest @ ..] if rest.first().is_some_and(|b| b & 0x80 != 0) => rest,
            // Empty, zero, negative, or a superfluous leading zero.
            [] | [0, ..] => return Err(bad()),
            [first, ..] if first & 0x80 != 0 => return Err(bad()),
            value => value,
        };
        if value.len() > 32 {
            return Err(bad());
        }
        // DER drops leading zero bytes; OpenSSH wants r and s as 32 bytes each.
        let mut out = vec![0; 32 - value.len()];
        out.extend_from_slice(value);
        Ok(out)
    };
    let sig_r = integer()?;
    let sig_s = integer()?;
    if !r.0.is_empty() {
        return Err(bad());
    }
    Ok((sig_r, sig_s))
}

/// Builds the sign reply from WebAuthn's authenticator data and signature.
pub fn sign_result(
    alg: u32,
    authenticator_data: &[u8],
    signature: &[u8],
) -> io::Result<SignResult> {
    let auth = AuthenticatorData::parse(authenticator_data)?;
    debug!("Parsed authenticator data:\n{}", indent(&auth.describe()));
    let (sig_r, sig_s) = match alg {
        SSH_SK_ED25519 => (ed25519_signature(signature)?, None),
        SSH_SK_ECDSA => {
            let (r, s) = ecdsa_signature(signature)?;
            (r, Some(s))
        }
        _ => return Err(invalid(&format!("unrecognized OpenSSH algorithm {alg}"))),
    };
    Ok(SignResult {
        counter: auth.sign_count,
        flags: auth.flags,
        sig_r: Some(sig_r),
        sig_s,
    })
}

// COSE key parameters and values (RFC 8152 §7.1, §13).
const COSE_KTY: i128 = 1;
const COSE_ALG: i128 = 3;
const COSE_CRV: i128 = -1;
const COSE_X: i128 = -2;
const COSE_Y: i128 = -3;
const KTY_OKP: i128 = 1;
const KTY_EC2: i128 = 2;
const ALG_ES256: i128 = -7;
const ALG_EDDSA: i128 = -8;
const CRV_P256: i128 = 1;
const CRV_ED25519: i128 = 6;

/// COSE public key → OpenSSH public key: Ed25519 → x; P-256 → `0x04 ‖ x ‖ y`.
pub fn cose_public_key(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let Value::Map(map) = ciborium::from_reader(bytes).map_err(|e| invalid(&e.to_string()))? else {
        return Err(invalid("COSE key is not a map"));
    };
    let get = |key: i128| {
        map.iter()
            .find(|(k, _)| k.as_integer().map(i128::from) == Some(key))
            .map(|(_, v)| v)
    };
    let int = |key: i128, what: &str| {
        get(key)
            .and_then(Value::as_integer)
            .map(i128::from)
            .ok_or_else(|| invalid(&format!("missing {what}")))
    };
    let bytes = |key: i128, what: &str| {
        get(key)
            .and_then(Value::as_bytes)
            .ok_or_else(|| invalid(&format!("missing {what}")))
    };
    let kty = int(COSE_KTY, "key type")?;
    let alg = int(COSE_ALG, "key algorithm")?;
    let crv = int(COSE_CRV, "curve type")?;
    debug!("Public key type {kty}, algorithm {alg}, curve {crv}");
    match kty {
        KTY_OKP => {
            if alg != ALG_EDDSA {
                return Err(invalid(&format!(
                    "public key algorithm {alg} is not supported"
                )));
            }
            if crv != CRV_ED25519 {
                return Err(invalid(&format!("OKP curve type {crv} is not supported")));
            }
            let x = bytes(COSE_X, "OKP curve X coordinate")?;
            if x.len() != 32 {
                return Err(invalid("Ed25519 X coordinate is not 32 bytes"));
            }
            Ok(x.clone())
        }
        KTY_EC2 => {
            if alg != ALG_ES256 {
                return Err(invalid(&format!(
                    "public key algorithm {alg} is not supported"
                )));
            }
            if crv != CRV_P256 {
                return Err(invalid(&format!("EC2 curve type {crv} is not supported")));
            }
            let x = bytes(COSE_X, "EC2 curve X coordinate")?;
            let y = get(COSE_Y).ok_or_else(|| invalid("missing EC2 curve Y coordinate"))?;
            let y = y
                .as_bytes()
                .ok_or_else(|| invalid("EC2 compressed point form is not supported"))?;
            if map.len() != 5 {
                return Err(invalid(
                    "optional parameters in the public key are not permitted",
                ));
            }
            if x.len() != 32 || y.len() != 32 {
                return Err(invalid("P-256 coordinates are not 32 bytes each"));
            }
            Ok([&[0x04][..], x, y].concat())
        }
        _ => Err(invalid(&format!("public key type {kty} is not supported"))),
    }
}

/// Builds the enroll reply from a WebAuthn attestation object (upstream `sk_enroll_safe`).
pub fn parse_attestation(bytes: &[u8]) -> io::Result<EnrollResult> {
    let Value::Map(object) = ciborium::from_reader(bytes).map_err(|e| invalid(&e.to_string()))?
    else {
        return Err(invalid("attestation object is not a map"));
    };
    let field = |name: &str| {
        object
            .iter()
            .find(|(k, _)| k.as_text() == Some(name))
            .map(|(_, v)| v)
    };
    let format = field("fmt")
        .and_then(Value::as_text)
        .ok_or_else(|| invalid("missing attestation object format"))?;
    debug!("Attestation object format: {format}");
    if format != "packed" && format != "fido-u2f" {
        return Err(invalid(&format!(
            "unknown attestation object format {format:?}"
        )));
    }

    let auth_data = field("authData")
        .and_then(Value::as_bytes)
        .ok_or_else(|| invalid("missing authenticator data in attestation object"))?;
    let auth = AuthenticatorData::parse(auth_data)?;
    debug!("Parsed authenticator data:\n{}", indent(&auth.describe()));
    let credential = auth
        .attested_credential
        .ok_or_else(|| invalid("authenticator data has no attested credential"))?;
    let public_key = cose_public_key(&credential.cose_key)?;
    debug!(
        "Public key ({} bytes): 0x{}",
        public_key.len(),
        hex(&public_key)
    );

    let Some(Value::Map(statement)) = field("attStmt") else {
        return Err(invalid(
            "missing attestation statement in attestation object",
        ));
    };
    let statement_field = |name: &str| {
        statement
            .iter()
            .find(|(k, _)| k.as_text() == Some(name))
            .map(|(_, v)| v)
    };
    let signature = statement_field("sig")
        .and_then(Value::as_bytes)
        .ok_or_else(|| invalid("missing signature in attestation statement"))?;
    let attestation_cert = match statement_field("x5c") {
        Some(x5c) => {
            debug!("Certificate array is present in attestation statement");
            x5c.as_array()
                .and_then(|certs| certs.first())
                .and_then(Value::as_bytes)
                .ok_or_else(|| invalid("attestation statement has no usable certificate"))?
                .clone()
        }
        // "packed" allows self-attestation without a certificate; "fido-u2f" does not.
        None if format == "packed" => {
            debug!("Certificate array is missing, assuming self-attestation");
            Vec::new()
        }
        None => {
            return Err(invalid(
                "missing certificate array, but self-attestation is not permitted",
            ));
        }
    };

    Ok(EnrollResult {
        attestation_cert,
        authdata: Vec::new(), // upstream parity: never filled
        key_handle: credential.id,
        public_key,
        signature: signature.clone(),
    })
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_owned())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        _ = write!(s, "{b:02x}");
        s
    })
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("  | {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wfb_protocol::message::{Response, encode};

    const BAD_SIG: &str = "invalid or unrecognized signature received from authenticator";

    fn fixture(hex: &str) -> Vec<u8> {
        hex.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    /// Authenticator data of the captured upstream signature (see the fixtures README).
    fn fixture_authdata() -> Vec<u8> {
        fixture(include_str!(
            "../../protocol/tests/fixtures/sign-authdata.hex"
        ))
    }

    fn fixture_response() -> Vec<u8> {
        fixture(include_str!(
            "../../protocol/tests/fixtures/sign-response.hex"
        ))
    }

    /// `sig_r` of the captured response: the Ed25519 signature WebAuthn returned.
    fn fixture_sig() -> Vec<u8> {
        let raw = fixture_response();
        // The reply ends with sig_r, then `6b "return_code" 00` (13 bytes).
        raw[raw.len() - 13 - 64..raw.len() - 13].to_vec()
    }

    fn assert_invalid<T: std::fmt::Debug>(result: io::Result<T>, needle: &str) {
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
        assert!(err.to_string().contains(needle), "{err}");
    }

    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    fn cbor(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(v, &mut out).unwrap();
        out
    }

    fn int(i: i64) -> Value {
        Value::Integer(i.into())
    }

    fn auth_data(flags: u8, count: u32, credential: Option<(&[u8], &[u8])>) -> Vec<u8> {
        let mut out = vec![0xab; 32];
        out.push(flags);
        out.extend(count.to_be_bytes());
        if let Some((id, key)) = credential {
            out.extend([0x11; 16]);
            out.extend((id.len() as u16).to_be_bytes());
            out.extend(id);
            out.extend(key);
        }
        out
    }

    fn ed25519_key() -> Value {
        Value::Map(vec![
            (int(1), int(1)),
            (int(3), int(-8)),
            (int(-1), int(6)),
            (int(-2), Value::Bytes(vec![0x22; 32])),
        ])
    }

    fn p256_key() -> Vec<(Value, Value)> {
        vec![
            (int(1), int(2)),
            (int(3), int(-7)),
            (int(-1), int(1)),
            (int(-2), Value::Bytes(vec![0x33; 32])),
            (int(-3), Value::Bytes(vec![0x44; 32])),
        ]
    }

    fn der(r: &[u8], s: &[u8]) -> Vec<u8> {
        let mut out = vec![0x30, (r.len() + s.len() + 4) as u8, 0x02, r.len() as u8];
        out.extend(r);
        out.extend([0x02, s.len() as u8]);
        out.extend(s);
        out
    }

    // --- authenticator data ---

    #[test]
    fn parses_upstream_fixture_authenticator_data() {
        let auth = AuthenticatorData::parse(&fixture_authdata()).unwrap();
        assert_eq!((auth.flags, auth.sign_count), (1, 689));
        // SHA-256("ssh:5c-github")
        assert_eq!(auth.rp_id_hash[..4], [0x2c, 0xb9, 0x54, 0x4a]);
        assert_eq!(auth.attested_credential, None);
        assert!(auth.describe().contains("Signature count: 689"));
    }

    #[test]
    fn parses_attested_credential() {
        let key = cbor(&ed25519_key());
        let auth = AuthenticatorData::parse(&auth_data(0x41, 0, Some((&[9; 48], &key)))).unwrap();
        let c = auth.attested_credential.unwrap();
        assert_eq!((c.aaguid, c.id, c.cose_key), ([0x11; 16], vec![9; 48], key));
    }

    #[test]
    fn rejects_truncated_authenticator_data() {
        assert_invalid(AuthenticatorData::parse(&[0; 36]), "truncated");
        let mut bytes = auth_data(0x41, 0, Some((&[9; 48], &[])));
        bytes.truncate(bytes.len() - 1);
        assert_invalid(AuthenticatorData::parse(&bytes), "truncated");
    }

    // --- user verification and options ---

    #[test]
    fn user_verification_rules() {
        assert!(!user_verification_required("ssh:", 0x01, false));
        assert!(user_verification_required("ssh:", 0x05, false));
        assert!(user_verification_required("ssh:", 0x00, true));
        assert!(user_verification_required(
            VERIFY_REQUIRED_APPLICATION,
            0,
            false
        ));
    }

    #[test]
    fn enroll_user_from_option() {
        assert_eq!(enroll_user_name(&[]), "SSH user");
        let opt = SkOption {
            name: "user".into(),
            required: 0,
            value: "test".into(),
        };
        assert_eq!(enroll_user_name(&[opt]), "test");
    }

    // --- signatures ---

    #[test]
    fn ed25519_signature_is_passed_through() {
        let sig = fixture_sig();
        assert_eq!(ed25519_signature(&sig).unwrap(), sig);
        assert_invalid(ed25519_signature(&sig[..63]), "shorter than 64 bytes");
        assert_eq!(
            ed25519_signature(&[sig.clone(), vec![0]].concat()).unwrap(),
            sig
        );
    }

    #[test]
    fn ecdsa_signature_variants() {
        let (r, s) = ([0x11; 32], [0x22; 32]);
        let (r33, s33) = (
            [&[0][..], &[0x91; 32]].concat(),
            [&[0][..], &[0x92; 32]].concat(),
        );
        for (dr, ds) in [(&r[..], &s[..]), (&r33, &s), (&r, &s33), (&r33, &s33)] {
            let (pr, ps) = ecdsa_signature(&der(dr, ds)).unwrap();
            assert_eq!(pr, dr[dr.len() - 32..]);
            assert_eq!(ps, ds[ds.len() - 32..]);
        }
        // Minimal DER: 31-byte r or s (leading zero dropped) is left-padded.
        let (pr, ps) = ecdsa_signature(&der(&[0x11; 31], &[0x22; 32])).unwrap();
        assert_eq!(pr, [&[0][..], &[0x11; 31]].concat());
        assert_eq!(ps, [0x22; 32]);
        let (pr, ps) = ecdsa_signature(&der(&[0x11; 32], &[0x22; 30])).unwrap();
        assert_eq!(pr, [0x11; 32]);
        assert_eq!(ps, [&[0, 0][..], &[0x22; 30]].concat());
        // Smallest values: one byte each, and one needing the sign padding.
        let (pr, ps) = ecdsa_signature(&der(&[0x01], &[0x00, 0x80])).unwrap();
        assert_eq!(pr, [&[0; 31][..], &[1]].concat());
        assert_eq!(ps, [&[0; 31][..], &[0x80]].concat());
    }

    #[test]
    fn ecdsa_signature_rejects_malformed() {
        let good = der(&[0x11; 32], &[0x22; 32]);
        let with = |at: usize, byte: u8| {
            let mut sig = good.clone();
            sig[at] = byte;
            sig
        };
        let cases = [
            ("wrong sequence tag", with(0, 0x31)),
            ("sequence shorter than content", with(1, 67)),
            ("sequence longer than content", with(1, 69)),
            (
                "long-form length",
                [&[0x30, 0x81, 68][..], &good[2..]].concat(),
            ),
            ("trailing byte", [&good[..], &[0]].concat()),
            ("extra byte inside the sequence", {
                let mut sig = [&good[..], &[0]].concat();
                sig[1] += 1;
                sig
            }),
            ("wrong integer tag", with(2, 0x03)),
            ("34-byte integer", der(&[0x11; 34], &[0x22; 30])),
            ("33 bytes without padding", der(&[0x01; 33], &[0x22; 32])),
            (
                "superfluous leading zero",
                der(&[&[0][..], &[0x11; 32]].concat(), &[0x22; 32]),
            ),
            (
                "non-minimal 32 bytes",
                der(&[&[0][..], &[0x11; 31]].concat(), &[0x22; 32]),
            ),
            ("negative r", der(&[0x81; 32], &[0x22; 32])),
            ("negative s", der(&[0x11; 32], &[0xff; 32])),
            ("zero r", der(&[0], &[0x22; 32])),
            ("empty s", der(&[0x11; 32], &[])),
            ("truncated", good[..40].to_vec()),
            ("empty", vec![]),
        ];
        for (what, sig) in cases {
            let err = ecdsa_signature(&sig).expect_err(what);
            assert_eq!(err.to_string(), BAD_SIG, "{what}");
        }
    }

    /// The captured upstream run, end to end: WebAuthn's authenticator data and signature turn
    /// into exactly the reply bytes upstream's exe sent.
    #[test]
    fn sign_result_reproduces_upstream_reply() {
        let r = sign_result(SSH_SK_ED25519, &fixture_authdata(), &fixture_sig()).unwrap();
        assert_eq!(encode(&Response::ok(r)).unwrap(), fixture_response());
    }

    #[test]
    fn sign_result_ecdsa_and_unknown_algorithm() {
        let r = sign_result(
            SSH_SK_ECDSA,
            &auth_data(5, 7, None),
            &der(&[1; 32], &[2; 32]),
        )
        .unwrap();
        assert_eq!((r.counter, r.flags), (7, 5));
        assert_eq!((r.sig_r, r.sig_s), (Some(vec![1; 32]), Some(vec![2; 32])));
        assert_invalid(
            sign_result(2, &auth_data(1, 1, None), &[0; 64]),
            "unrecognized OpenSSH algorithm 2",
        );
    }

    // --- COSE keys ---

    #[test]
    fn cose_ed25519() {
        assert_eq!(
            cose_public_key(&cbor(&ed25519_key())).unwrap(),
            vec![0x22; 32]
        );
    }

    #[test]
    fn cose_p256_uncompressed() {
        let key = cose_public_key(&cbor(&Value::Map(p256_key()))).unwrap();
        assert_eq!(key, [&[4][..], &[0x33; 32], &[0x44; 32]].concat());
    }

    #[test]
    fn cose_rejects_unsupported_keys() {
        let with = |mut m: Vec<(Value, Value)>, k: i64, v: Option<Value>| {
            m.retain(|(key, _)| key != &int(k));
            m.extend(v.map(|v| (int(k), v)));
            cbor(&Value::Map(m))
        };
        let Value::Map(ed) = ed25519_key() else {
            unreachable!()
        };
        let bytes = |n| Some(Value::Bytes(vec![0x33; n]));
        let cases = [
            (with(ed.clone(), 1, Some(int(3))), "public key type 3"),
            (with(ed.clone(), 3, Some(int(-7))), "algorithm -7 is not"),
            (with(ed.clone(), -1, Some(int(4))), "OKP curve type 4"),
            (with(ed.clone(), -2, None), "missing OKP curve X"),
            (with(ed.clone(), -2, bytes(31)), "not 32 bytes"),
            (with(p256_key(), 3, Some(int(-35))), "algorithm -35 is not"),
            (with(p256_key(), -1, Some(int(2))), "EC2 curve type 2"),
            (with(p256_key(), -2, None), "missing EC2 curve X"),
            (with(p256_key(), -3, None), "missing EC2 curve Y"),
            (with(p256_key(), -3, Some(Value::Bool(true))), "compressed"),
            (with(p256_key(), 7, Some(int(0))), "optional parameters"),
            (with(p256_key(), -2, bytes(31)), "coordinates are not 32"),
            (with(p256_key(), -3, bytes(33)), "coordinates are not 32"),
            (with(ed, 1, None), "missing key type"),
            (cbor(&int(1)), "not a map"),
        ];
        for (case, needle) in cases {
            assert_invalid(cose_public_key(&case), needle);
        }
    }

    // --- attestation objects ---

    fn attestation(fmt: &str, statement: Vec<(&str, Value)>, flags: u8) -> Vec<u8> {
        let text = |s: &str| Value::Text(s.into());
        let mut key = cbor(&ed25519_key());
        if flags & FLAG_ED != 0 {
            // Extension data follows the COSE key (e.g. keys that set credProtect by default).
            key.extend(cbor(&Value::Map(vec![(text("credProtect"), int(1))])));
        }
        cbor(&Value::Map(vec![
            (text("fmt"), text(fmt)),
            (
                text("attStmt"),
                Value::Map(statement.into_iter().map(|(k, v)| (text(k), v)).collect()),
            ),
            (
                text("authData"),
                Value::Bytes(auth_data(flags, 0, Some((&[9; 64], &key)))),
            ),
        ]))
    }

    fn sig() -> (&'static str, Value) {
        ("sig", Value::Bytes(vec![0x55; 70]))
    }

    fn x5c() -> (&'static str, Value) {
        let certs = vec![Value::Bytes(vec![0xce; 300]), Value::Bytes(vec![0xcf; 10])];
        ("x5c", Value::Array(certs))
    }

    #[test]
    fn attestation_with_certificate() {
        for fmt in ["packed", "fido-u2f"] {
            let r = parse_attestation(&attestation(
                fmt,
                vec![("alg", int(-8)), sig(), x5c()],
                0x41,
            ))
            .unwrap();
            assert_eq!(
                r,
                EnrollResult {
                    attestation_cert: vec![0xce; 300],
                    authdata: vec![],
                    key_handle: vec![9; 64],
                    public_key: vec![0x22; 32],
                    signature: vec![0x55; 70],
                }
            );
        }
    }

    #[test]
    fn packed_self_attestation() {
        let r = parse_attestation(&attestation("packed", vec![sig()], 0x41)).unwrap();
        assert!(r.attestation_cert.is_empty());
    }

    #[test]
    fn attestation_with_extension_data() {
        let r = parse_attestation(&attestation("packed", vec![sig()], 0xc1)).unwrap();
        assert_eq!(r.public_key, vec![0x22; 32]);
    }

    #[test]
    fn attestation_rejects_invalid() {
        let cases = [
            (
                attestation("fido-u2f", vec![sig()], 0x41),
                "self-attestation is not permitted",
            ),
            (
                attestation("none", vec![sig(), x5c()], 0x41),
                "unknown attestation object format",
            ),
            (
                attestation("packed", vec![x5c()], 0x41),
                "missing signature",
            ),
            (
                attestation("packed", vec![sig(), ("x5c", Value::Array(vec![]))], 0x41),
                "no usable certificate",
            ),
            (
                attestation("packed", vec![sig()], 0x01),
                "no attested credential",
            ),
            (cbor(&int(1)), "not a map"),
        ];
        for (case, needle) in cases {
            assert_invalid(parse_attestation(&case), needle);
        }
    }

    // --- robustness ---

    #[test]
    fn random_input_never_panics() {
        let mut state = 0x853c_49e6_748f_ea9b;
        let seeds = [
            fixture_authdata(),
            der(&[0x11; 32], &[0x91; 32]),
            cbor(&Value::Map(p256_key())),
            attestation("packed", vec![sig(), x5c()], 0xc1),
        ];
        for i in 0..20_000 {
            // Random bytes, or a seed with one byte changed or truncated.
            let bytes: Vec<u8> = match i % 3 {
                0 => {
                    let len = (xorshift(&mut state) % 80) as usize;
                    (0..len).map(|_| xorshift(&mut state) as u8).collect()
                }
                n => {
                    let mut b = seeds[i % seeds.len()].clone();
                    let at = (xorshift(&mut state) as usize) % b.len();
                    if n == 1 {
                        b.truncate(at);
                    } else {
                        b[at] = xorshift(&mut state) as u8;
                    }
                    b
                }
            };
            if let Ok((r, s)) = ecdsa_signature(&bytes) {
                assert_eq!((r.len(), s.len()), (32, 32));
            }
            if let Ok(auth) = AuthenticatorData::parse(&bytes) {
                _ = auth.describe();
            }
            _ = cose_public_key(&bytes);
            _ = parse_attestation(&bytes);
        }
    }
}
