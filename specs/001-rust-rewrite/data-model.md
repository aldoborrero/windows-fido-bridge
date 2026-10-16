# Data Model: Rust rewrite of windows-fido-bridge

Entities exchanged between OpenSSH, the WSL library and the Windows executable. Wire encodings
are in [contracts/wire-protocol.md](contracts/wire-protocol.md); the C ABI in
[contracts/sk-api.md](contracts/sk-api.md).

## Request (Linux → Windows)

One per process invocation. Variant chosen by `request_type`.

| Field | Type | Notes |
| --- | --- | --- |
| `request_type` | text | `"sk_enroll"` or `"sk_sign"`; anything else → Windows side aborts with non-zero exit (upstream) |
| `request_parameters` | map | `EnrollParams` or `SignParams` |

### EnrollParams

| Field | Type | Validation |
| --- | --- | --- |
| `alg` | uint | `0` = ECDSA P-256, `1` = Ed25519; other → enrollment error |
| `application` | text | relying-party id, e.g. `ssh:` or `ssh:5c-github` |
| `challenge` | bytes | passed to WebAuthn as client data, hashed with SHA-256 by Windows |
| `flags` | uint (u8) | `SSH_SK_*` bits; `0x04` → user verification required |
| `sk_options` | array of `SkOption` | may be empty |

### SignParams

| Field | Type | Validation |
| --- | --- | --- |
| `alg` | uint | `0` or `1`; other → Windows replies `-2` |
| `application` | text | must match the enrollment application |
| `data` | bytes | message to sign (OpenSSH ≥ 8.3 passes the unhashed message) |
| `flags` | uint (u8) | as above |
| `key_handle` | bytes | WebAuthn credential id from the key file |
| `sk_options` | array of `SkOption` | |

### SkOption

| Field | Type | Notes |
| --- | --- | --- |
| `name` | text | e.g. `user`, `device` |
| `required` | uint | `0` or `1` (never a CBOR boolean) |
| `value` | text | |

## Response (Windows → Linux)

| Field | Type | Notes |
| --- | --- | --- |
| `return_code` | int | `0` success; `-1` general; `-2` unsupported |
| `response_parameters` | map | present only when `return_code == 0` |

### EnrollResult

| Field | Type | Source |
| --- | --- | --- |
| `attestation_cert` | bytes | first `x5c` certificate, or empty for self-attestation |
| `authdata` | bytes | always empty (upstream parity) |
| `key_handle` | bytes | attested credential id |
| `public_key` | bytes | Ed25519: 32-byte x; P-256: `0x04 ‖ x(32) ‖ y(32)` |
| `signature` | bytes | `attStmt.sig` |

### SignResult

| Field | Type | Source |
| --- | --- | --- |
| `counter` | uint (u32) | authenticator data signature counter (big-endian on the wire from the authenticator) |
| `flags` | uint (u8) | authenticator data flags byte |
| `sig_r` | bytes | Ed25519: whole 64-byte signature; ECDSA: r, 32 bytes. Required on success; CBOR null → decode error |
| `sig_s` | bytes, optional | ECDSA only: s, 32 bytes; may be absent, CBOR null → decode error |

## AuthenticatorData (parsed on Windows, from WebAuthn)

Layout (WebAuthn §6.1): `rp_id_hash[32] ‖ flags u8 ‖ sign_count u32 BE ‖ attested_credential?`.

- `flags` bits: `0x01` UP, `0x04` UV, `0x40` AT (attested credential included), `0x80` ED.
- `AttestedCredential` (only if AT): `aaguid[16] ‖ cred_id_len u16 BE ‖ cred_id ‖ cose_key (CBOR)`.

## CoseKey → OpenSSH public key

| kty | alg | crv | Required params | Output |
| --- | --- | --- | --- | --- |
| 1 (OKP) | -8 (EdDSA) | 6 (Ed25519) | `-2` x (32 bytes) | x |
| 2 (EC2) | -7 (ES256) | 1 (P-256) | `-2` x, `-3` y (32-byte byte strings); exactly 5 map entries | `0x04 ‖ x ‖ y` |

Anything else (other kty/alg/crv, missing params, `-3` not a byte string = compressed form,
extra params on EC2, coordinates not 32 bytes) → enrollment error.

## AttestationObject (enroll)

CBOR map with `fmt` (text: `packed` or `fido-u2f`, else error), `authData` (bytes) and `attStmt`
(map with `sig` bytes, optional `x5c` array of bytes). Missing `x5c` allowed only for `packed`.

## ECDSA signature (sign)

DER `SEQUENCE { INTEGER r, INTEGER s }` with the SEQUENCE length equal to the remaining bytes
(no trailing data). Each INTEGER is minimal, positive and non-zero, at most 32 value bytes plus
an optional `0x00` only when the top bit is set; shorter ones are left-padded (research R12).
Output r and s as 32 bytes each. Anything else → error.

## Frame

`<decimal length>|<payload>`; length digits only `0-9`, at most 5 of them; size ≤ 32768 (the
library refuses to send a larger request); payload is one CBOR map.

## C-side lifetimes (sk-middleware)

`SkEnrollResponse` / `SkSignResponse` and each buffer are allocated with `libc::calloc`. They are
owned by a Rust guard (freed on drop) until handed to OpenSSH via the out-pointer; after that
OpenSSH owns them. Zero-length buffers are returned as null pointer with length 0.
`SkEnrollResponse.flags` is the request `flags` (research R10). Layouts (x86_64): enroll 88 bytes,
sign 40, option 24, checked at compile time.
