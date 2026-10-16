# Research: Rust rewrite of windows-fido-bridge

Phase 0 output for [plan.md](plan.md). Every decision cites the upstream behaviour it preserves
(`.scratch/windows-fido-bridge-cpp/src/...`).

## R1. CBOR library

- **Decision**: `ciborium` 0.2 with `serde` 1 (derive) and `serde_bytes` 0.11 for byte strings.
- **Rationale**: the wire format is small CBOR maps with text keys, unsigned/negative integers,
  byte strings, text strings and one array (`common/src/cbor/*`). Serde structs make the schema
  explicit and testable in a few lines. `ciborium` is the de-facto serde CBOR crate (successor of
  `serde_cbor`). `serde_bytes` is required because serde encodes `Vec<u8>` as a CBOR *array* by
  default, while upstream uses CBOR byte strings (major type 2).
- **Alternatives considered**:
  - `minicbor` 2.x (derive) — fast and no_std, but its derive keys fields by integer index by
    default; matching upstream's text keys needs per-field attributes. More ceremony, no gain.
  - Building `ciborium::Value` by hand like the C++ — fewer derives but stringly-typed code and
    no compile-time schema.
  - Hand-written CBOR (like upstream's ~1k lines) — rejected by principle III.
- **Notes**: upstream has no CBOR boolean; `sk_options[].required` is the integer 0/1 and MUST
  stay an integer. Map key order is irrelevant to upstream's decoder (lookup by key), but fields
  are declared in lexicographic order so encodings match upstream byte-for-byte where possible
  (useful for fixture tests).

## R2. Windows WebAuthn bindings

- **Decision**: `windows-sys` 0.61 with features `Win32_Foundation`,
  `Win32_Networking_WindowsWebServices`, `Win32_UI_WindowsAndMessaging`,
  `Win32_System_LibraryLoader`, `Win32_System_Threading` — as a `[target.'cfg(windows)'.dependencies]`
  of `webauthn-bridge` only.
- **Rationale**: confirmed that `windows_sys::Win32::Networking::WindowsWebServices` exposes
  `WebAuthNAuthenticatorMakeCredential`, `WebAuthNAuthenticatorGetAssertion`,
  `WebAuthNFreeAssertion`, `WebAuthNFreeCredentialAttestation`, `WebAuthNGetErrorName` and the
  `WEBAUTHN_*` structs. The calls are raw FFI either way and every one is `unsafe`; the thin,
  zero-cost `-sys` bindings compile much faster than `windows` and match principle II (unsafe at
  the FFI boundary, written explicitly).
- **Alternatives considered**: `windows` 0.62 (safe-ish wrappers, `HRESULT`/`Result` helpers) —
  heavier build, still needs unsafe for these APIs; hand-written `extern "system"` declarations
  with `LoadLibrary` like upstream — more unsafe surface to maintain.
- **Linking**: upstream loads `webauthn.dll` with `LoadLibraryA` at start-up. `windows-sys`
  links it as an import (raw-dylib), which fails to start only on Windows < 10 1903, already
  outside the supported range. Accept the static import; no dynamic loading.

## R3. C allocation for OpenSSH

- **Decision**: `libc` 0.2 (`calloc`, `malloc`, `free`, `dladdr`), dependency of `sk-middleware`
  only.
- **Rationale**: OpenSSH frees `sk_enroll_response`, `sk_sign_response` and their buffers with
  `free()` (constitution II). `libc` also provides `dladdr`, needed to find the library's own
  path (upstream `linux_middleware/src/bridge.cpp: get_library_path`).
- **Alternatives considered**: `std::alloc` with the system allocator — not guaranteed to be the
  same allocator as OpenSSH's `free`. Rejected.

## R4. Shared protocol crate

- **Decision**: add `crates/protocol` (package `wfb-protocol`, lib), used by both binaries.
- **Rationale**: both sides must agree on the message types, the length-prefixed framing and the
  `sk-api.h` constants; duplicating them invites drift. The crate is pure, platform-independent
  Rust and carries the unit tests demanded by principle IV. It holds only shared things; WebAuthn
  conversion stays in `webauthn-bridge` (only Windows needs it).
- **Alternatives considered**: a module duplicated in each crate (drift risk); putting everything
  in one crate with `cfg` (mixes cdylib and bin targets, awkward for crane).

## R5. Where WebAuthn → OpenSSH conversion lives and how it is tested

- **Decision**: `crates/webauthn-bridge/src/convert.rs` (authenticator data parsing, COSE public
  key → OpenSSH public key, DER ECDSA → r/s, attestation object handling) is plain Rust with no
  `cfg(windows)`; only `webauthn.rs`/`window.rs` are `#[cfg(windows)]`. On Linux the binary's
  `main` is a stub that exits non-zero, so `cargo test --workspace` runs the conversion tests on
  Linux.
- **Rationale**: principle IV; Windows binaries cannot run in the Nix sandbox.
- **Nix impact**: the current `package.nix` only builds/tests `--package sk-middleware` on Linux.
  Add a `nix/checks/tests.nix` (crane `cargoTest` with `--workspace`) so `nix flake check` runs
  every unit test.

## R6. Spawning the Windows executable

- **Decision**: `std::process::Command` with `stdin`/`stdout` piped and `stderr` inherited;
  set `WSLENV` to the existing value plus
  `WINDOWS_FIDO_BRIDGE_DEBUG:WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` (colon-joined, as
  upstream); executable path = directory of the library (via `dladdr`) +
  `windowsfidobridge.exe`; wait for exit and treat non-zero status as failure.
- **Rationale**: same observable behaviour as upstream's `fork`/`dup2`/`execl`, without unsafe
  process code. Running a `.exe` from a Linux path works through WSL interop (verified with the
  C++ build on 2026-10-16).
- **Alternatives considered**: `fork`+`execl` via `libc` (more unsafe, no benefit).

## R7. Logging

- **Decision**: a ~20-line module in `wfb-protocol`: level from `WINDOWS_FIDO_BRIDGE_DEBUG`
  (set → debug, unset → warn), output `[<timestamp>] [<side>] [<level>] message` to stderr,
  plus a hex-dump helper in upstream's `  | 0000: …` layout. No logging crate.
- **Rationale**: principle III; upstream's spdlog usage is limited to these two levels and the
  hex dump. Exact log text is not part of parity (stderr is diagnostic), but the content (what is
  logged, PIN never logged) is.
- **Alternatives considered**: `log` + `env_logger` (two crates for what stdlib does).

## R8. Window and threading on Windows

- **Decision**: replicate upstream `window.cpp`/`run_with_win32_window`: create an invisible
  `WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, `WS_POPUPWINDOW` window; run the WebAuthn call on a
  `std::thread`; pump the message loop on the main thread until the worker finishes
  (`MsgWaitForMultipleObjects` on the thread handle), then close the window.
- **Rationale**: FR-009 — upstream found that `GetConsoleWindow` does not work as parent in
  Windows Terminal / VS Code.

## R9. Error-code mapping and panics

- **Decision**: every `extern "C"` entry point wraps its body in `std::panic::catch_unwind` and
  maps `Err`/panic to `SSH_SK_ERR_GENERAL` (-1). Any non-zero `return_code` from the Windows
  side → -1 (upstream parity). Windows side maps unsupported algorithm on sign to
  `SSH_SK_ERR_UNSUPPORTED` (-2) in its reply, other failures to -1. Release profile keeps
  `panic = "unwind"` so `catch_unwind` works in the cdylib.
- **Rationale**: FR-012, constitution II. Upstream leaked the partially built response on error
  (its deleters are empty); the rewrite frees it (owning wrapper with `Drop` calling
  `libc::free` until ownership is released to OpenSSH).

## R10. Upstream quirks kept for parity (documented, not "fixed")

- PIN from OpenSSH is never sent to Windows.
- `sk_enroll_response.flags` is left 0 and `authdata` empty on enrollment.
- Enrollment user id placeholder `(null)`, RP name `OpenSSH via windows-fido-bridge`,
  default user name `SSH user`, `user` option overrides it.
- `ssh:windows-fido-bridge-verify-required` application forces user verification.
- Frame header parse rejects sizes > 32768 bytes.

## R11. Toolchain and build

- **Decision**: no change — rust-overlay stable from `rust-toolchain.toml` (1.99.1 at time of
  writing), edition 2024, crane; Windows exe via `x86_64-pc-windows-gnu` + nixpkgs mingw-w64
  (already working in the scaffold).
