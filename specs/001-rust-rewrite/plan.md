# Implementation Plan: Rust rewrite of windows-fido-bridge with upstream parity

**Branch**: `001-rust-rewrite` | **Date**: 2026-10-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-rust-rewrite/spec.md`

## Summary

Reimplement the upstream C++ OpenSSH security key middleware in Rust with identical observable
behaviour: a Linux `cdylib` (`libwindowsfidobridge.so`) implementing the `sk-api.h` v10 C ABI
forwards each enroll/sign request, as length-prefixed CBOR over stdin/stdout, to a Windows
executable (`windowsfidobridge.exe`, cross-compiled) that calls the Windows WebAuthn API and
converts its results into OpenSSH's formats. A small shared crate holds the wire types, framing,
`sk-api` constants and logging; all conversion logic is platform-independent and unit-tested on
Linux, with golden fixtures captured from the upstream build. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust stable 1.99.1 (edition 2024), pinned by `rust-toolchain.toml`

**Primary Dependencies**: `ciborium` 0.2, `serde` 1 (derive), `serde_bytes` 0.11 (protocol);
`libc` 0.2 (sk-middleware); `windows-sys` 0.61 (webauthn-bridge, Windows target only)

**Storage**: N/A

**Testing**: `cargo test --workspace` on x86_64 Linux (unit tests with golden fixtures), run in
`nix flake check` via crane; manual hardware validation with `just try-sign` (see
[quickstart.md](quickstart.md))

**Target Platform**: WSL 2 x86_64 Linux (`x86_64-unknown-linux-gnu` cdylib) + Windows 10 1903+ /
11 x86_64 (`x86_64-pc-windows-gnu` exe via mingw-w64)

**Project Type**: native plug-in library + helper executable (OpenSSH `SSH_SK_PROVIDER`)

**Performance Goals**: bridge overhead < 2 s per operation excluding the user's touch (SC-002);
in practice dominated by WSL interop process start (~175 ms observed with upstream)

**Constraints**: no unwinding across FFI; OpenSSH-freed memory from `libc` allocator; messages
≤ 32768 bytes; wire format byte-compatible with upstream; no runtime DLLs beyond Windows system
DLLs

**Scale/Scope**: one request per process; ~1–2k lines of Rust across three crates

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Pre-research | Post-design |
| --- | --- | --- | --- |
| I. Upstream parity | Wire format, conversions, exe-next-to-.so, SK API v10, env vars match upstream; deviations documented | ✅ spec FR-001…FR-011 | ✅ contracts mirror upstream; golden fixtures from an upstream run; quirks kept (research R10); only deviation = freeing memory on error paths where upstream leaked (R9) |
| II. Unsafe at FFI only | unsafe limited to `extern "C"` entry points + `libc` calls in sk-middleware and `windows-sys` calls in webauthn-bridge; SAFETY comments; calloc'd responses; `catch_unwind` | ✅ | ✅ `lib.rs`/`ffi.rs`/`bridge.rs` (dladdr) in sk-middleware and `webauthn.rs`/`window.rs` in the bridge are the only unsafe modules; `wfb-protocol` and `convert.rs` use `#![forbid(unsafe_code)]` / `#[deny(unsafe_code)]` |
| III. Simplicity | stdlib first; few crates; no speculative abstractions | ✅ | ✅ 5 external crates (ciborium, serde, serde_bytes, libc, windows-sys); no logging crate; one shared crate justified in R4 |
| IV. Tested core | platform-independent logic unit-tested on Linux; Windows code thin; `just try-sign` on hardware | ✅ | ✅ `convert.rs` + `wfb-protocol` tested on Linux; new `nix/checks/tests.nix` runs `cargo test --workspace` |
| V. Nix build of record | `nix flake check` green; toolchain from `rust-toolchain.toml`; `--locked` | ✅ | ✅ packaging unchanged, plus the tests check |

No violations → Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-rust-rewrite/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R11
├── data-model.md        # Phase 1: entities, encodings, conversions
├── quickstart.md        # Phase 1: validation guide (automated + hardware)
├── contracts/
│   ├── sk-api.md        # C ABI exported to OpenSSH
│   ├── wire-protocol.md # Linux ↔ Windows framing + CBOR messages
│   └── fixtures/        # golden CBOR payloads captured from upstream
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
Cargo.toml                      # workspace: members = ["crates/*"], [profile.release]
crates/
├── protocol/                   # NEW lib `wfb-protocol` (platform-independent, no unsafe)
│   ├── src/lib.rs              # module declarations + re-exports
│   ├── src/sk.rs               # sk-api.h constants (version, flags, algs, error codes)
│   ├── src/message.rs          # Request/EnrollParams/SignParams/SkOption/Response/EnrollResult/SignResult (serde)
│   ├── src/frame.rs            # "<len>|<payload>" read/write over io::Read/Write, 32768 limit
│   ├── src/log.rs              # WINDOWS_FIDO_BRIDGE_DEBUG-gated stderr logger + hex dump
│   └── tests/fixtures/         # copies of specs/001-rust-rewrite/contracts/fixtures/*.hex
├── sk-middleware/              # cdylib `windowsfidobridge` → libwindowsfidobridge.so
│   ├── src/lib.rs              # extern "C" sk_api_version/sk_enroll/sk_sign/sk_load_resident_keys
│   ├── src/ffi.rs              # #[repr(C)] structs, C-string/option parsing, calloc'd buffers + guard
│   └── src/bridge.rs           # locate exe via dladdr, WSLENV, spawn, frame exchange, exit status
└── webauthn-bridge/            # bin `windowsfidobridge` → windowsfidobridge.exe
    ├── src/main.rs             # read request, dispatch, write response (cfg(windows)); stub elsewhere
    ├── src/convert.rs          # authdata, COSE key, attestation object, DER ECDSA → OpenSSH (no unsafe, tested on Linux)
    ├── src/webauthn.rs         # #[cfg(windows)] MakeCredential / GetAssertion via windows-sys
    └── src/window.rs           # #[cfg(windows)] topmost owner window + message loop + worker thread
nix/
├── checks/clippy.nix           # existing
├── checks/tests.nix            # NEW: crane cargoTest --workspace (runs bridge conversion tests on Linux)
└── packages/windows-fido-bridge/package.nix  # existing: .so + .exe into $out/lib
```

**Structure Decision**: keep the scaffold's two deliverable crates and add one shared library
crate, `crates/protocol` (package `wfb-protocol`), because both binaries must agree on message
types, framing, constants and logging (research R4). WebAuthn-specific conversion stays in
`webauthn-bridge` but in a platform-independent module so its tests run on Linux (R5). The Nix
package layout and the exe-next-to-.so contract are unchanged.

## Design Notes

- **Linux request flow** (`sk_sign`): `catch_unwind` → parse C inputs (`ffi.rs`) → build
  `Request::Sign` → `bridge::invoke(cbor)` (framed) → decode `Response` → if
  `return_code != 0` return -1 → allocate `sk_sign_response` + buffers with `calloc` inside a
  guard → write out-pointer, release guard → 0.
- **Windows flow**: `frame::read(stdin)` → dispatch on `request_type` →
  `window::run(|hwnd| webauthn::get_assertion(...))` → `convert::*` → `Response` →
  `frame::write(stdout)` → exit 0. Rust std handles are binary (no `_setmode` needed). Unknown
  `request_type` → log + non-zero exit (upstream aborts).
- **User verification**: required iff env `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` is set,
  or application == `ssh:windows-fido-bridge-verify-required`, or `flags & 0x04`; otherwise
  discouraged.
- **WebAuthn parameters**: RP id = application (UTF-16); client data = challenge/data bytes with
  hash alg `SHA-256`; credential list = the single key handle; enrollment user id `(null)`, user
  name = `user` option or `SSH user`, RP name `OpenSSH via windows-fido-bridge`; COSE alg -7
  (ECDSA) / -8 (Ed25519).
- **Release profile**: keep `panic = "unwind"` (needed by `catch_unwind` in the cdylib); the exe
  links statically against the mingw runtime (no extra DLLs; verified in the scaffold).
- **Unsupported on sign** (`alg` not 0/1): Windows replies `-2`; Linux maps any non-zero to -1.

## Complexity Tracking

No constitution violations to justify.
