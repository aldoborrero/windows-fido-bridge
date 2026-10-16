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
- `authdata` stays empty on enrollment.
- Enrollment user id placeholder `(null)`, RP name `OpenSSH via windows-fido-bridge`,
  default user name `SSH user`, `user` option overrides it.
- `ssh:windows-fido-bridge-verify-required` application forces user verification.
- Frame header parse rejects sizes > 32768 bytes.

**Not kept: `sk_enroll_response.flags`.** Upstream leaves it 0, but OpenSSH stores
`resp->flags` as the new key's `sk_flags` (`ssh-sk.c: sshsk_enroll` →
`sshsk_key_from_response`), and its own `sk-usbhid.c` sets `response->flags = flags` (the
request flags). With 0 the key file loses `SSH_SK_USER_PRESENCE_REQD` and
`SSH_SK_USER_VERIFICATION_REQD`: a key enrolled with `-O verify-required` no longer asks for
verification when signing, and every key looks as if enrolled with `-O no-touch-required`.
The rewrite returns the request flags, as `sk-usbhid.c` does (security fix; deviation listed in
R12).

## R11. Toolchain and build

- **Decision**: rust-overlay toolchain from `rust-toolchain.toml`, pinned to the exact release
  (`channel = "1.99.1"`, the stable release at the time of writing) so the devshell and every Nix
  build use the same compiler; edition 2024, crane; Windows exe via `x86_64-pc-windows-gnu` +
  nixpkgs mingw-w64.
- **Nix layout**: `nix/lib` holds the shared crane arguments (`commonArgs`, `strictDeps`) and
  one `buildDepsOnly` per target (`cargoArtifacts` for Linux, `windowsArtifacts` for the
  exe). The middleware build (`doCheck = false`), `checks.tests` (the only test run) and
  `checks.clippy` share the Linux one; the exe build and `checks.clippy-windows` (lints
  `webauthn.rs`/`window.rs` with `-D warnings`) share the Windows one, so `nix flake check`
  evaluates exactly two `-deps` derivations. The source filter keeps `*.hex` only under
  `crates/`.

## R12. Deviations from upstream

Every intentional behaviour difference; valid upstream traffic behaves identically unless noted.

**Signing and enrollment**

- **Enroll flags** (security): `sk_enroll_response.flags` = request flags instead of 0 (R10).
- **Short ECDSA integers**: DER encodes r/s minimally, so about 1 signature in 128 has a
  31-byte (or shorter) INTEGER and a SEQUENCE shorter than 68 bytes. Upstream rejects those and
  the signature fails at random; the rewrite accepts INTEGER lengths 1–33 and left-pads to 32
  bytes.
- **Strict DER**: upstream only checks tags and lengths 68–70 / 32–33 and ignores what follows.
  The rewrite also requires the SEQUENCE length to equal the remaining bytes (no trailing data),
  INTEGERs to be minimal (no superfluous leading `0x00`), positive (top bit clear unless padded)
  and non-zero. Upstream would pass such malformed values through to OpenSSH, where the
  signature fails to verify; no conforming authenticator emits them.
- **P-256 coordinates**: COSE EC2 `x`/`y` must be exactly 32 bytes; upstream copied whatever
  length it got into the public key.
- **Ed25519 signature length**: as upstream, the first 64 bytes are used and anything after is
  ignored; fewer than 64 bytes is an error.
- **Missing `sig_r` on success** is an error in the library; upstream hands OpenSSH a NULL
  `sig_r`, which OpenSSH rejects anyway (`sshsk_ed25519_sig`/`sshsk_ecdsa_sig`), so the outcome
  (failure) is the same.

**C ABI (sk-middleware)**

- **NULL buffer with non-zero length** from OpenSSH is an error instead of an empty buffer.
- **Invalid UTF-8** in `application` or option names/values is an error (CBOR text must be
  UTF-8); upstream copied the raw bytes into a CBOR text string. OpenSSH applications are ASCII
  (`ssh:` prefix).
- **Empty buffers** in responses are returned as NULL with length 0; upstream returned
  `calloc(1, 0)` (a non-NULL pointer) with length 0. OpenSSH treats both the same for
  `attestation_cert`/`authdata`; an empty `sig_r` fails either way.
- **Memory on error paths**: freed instead of leaked (R9).

**Process bridge**

- **Request size**: requests over 32768 bytes are rejected before spawning the exe. Upstream
  wrote them anyway; the exe refused the header and exited, and the next write raised SIGPIPE,
  killing `ssh-keygen`/`ssh-sk-helper`.
- **Inherited descriptors**: the child marks every fd ≥ 3 close-on-exec
  (`close_range(3, ~0, CLOSE_RANGE_CLOEXEC)` in `pre_exec`) so host descriptors such as
  `ssh-sk-helper`'s socket do not leak into the bridge; upstream's `fork`/`execl` leaked them.
  Close-on-exec instead of close keeps std's exec-error pipe working.
- **`WSLENV`** is handled as raw bytes (`OsString`); upstream also passed bytes through.
- **Console subsystem**: the exe is a console application like upstream's (its `WinMain` is
  only mingw's entry point; CMake builds it without `WIN32`), so stderr diagnostics reach the
  WSL terminal. (An earlier rewrite draft used the GUI subsystem, which dropped them.)

**Wire protocol decoding** (both directions; encodings are byte-identical to upstream, see
`crates/protocol/tests/fixtures/README.md`)

- **Frame header** at most 5 digits (the width of 32768): `000005|` is rejected and an endless
  `000…` stream errors instead of being read forever. Upstream accepts any number of leading
  zeros; its own writer never sends them.
- **Duplicate map keys** are an error (serde); upstream's `std::map::emplace` kept the first.
- **Invalid UTF-8 in text strings** is an error; upstream stored the bytes.
- **Indefinite-length items and tags** are accepted by ciborium (tags ignored); upstream throws
  on both. Neither side emits them.
- **CBOR null** for `sig_r`/`sig_s` is an error, as upstream (its `try_at<cbor_byte_string>`
  throws on null); an absent key is allowed on decode, as upstream.
- **Unknown map keys** are ignored, as upstream (lookup by key).
- **Debug notation** (`message::diagnostic`) prints upstream's `dump_debug` format (`b"…"` hex
  byte strings, `(null)`), so logs compare line by line.

**Windows UI**

- **Owner window class**: the window uses the predefined `STATIC` class instead of registering
  a custom one (same styles and position; upstream's window procedure only posts `WM_QUIT`,
  which the rewrite does not need because it destroys the window itself after the worker
  thread ends). The VS Code `SetParent` fallback is kept.

## R13. Test fixtures: what comes from upstream and what is synthetic

- From upstream (`crates/protocol/tests/fixtures/`, provenance in its README): the captured
  sign request and response; the authenticator data of that signature (rebuilt from the
  captured values and verified with `openssl pkeyutl -verify` against the key's public key, so
  it is exactly what the key signed); the enroll response encoded by upstream's own CBOR
  encoder. `convert.rs` turns the authenticator data and `sig_r` into a reply byte-identical to
  the captured one, and the middleware maps that reply to the OpenSSH struct.
- Synthetic (no upstream capture exists, and producing one needs hardware runs that are tracked
  as T030/T038): ECDSA signatures and their DER variants, COSE keys, attestation objects
  (`packed`/`fido-u2f`, with/without `x5c`, with extension data) and attested-credential
  authenticator data. They follow the WebAuthn/COSE specifications and upstream's parser rules;
  replace them with captures when an `ecdsa-sk` sign and an enrollment are run with
  `WINDOWS_FIDO_BRIDGE_DEBUG=1`.
- No-panic loops (xorshift, fixed seeds) feed random and mutated inputs to `frame::read`,
  `message::decode`, `AuthenticatorData::parse`, `ecdsa_signature`, `cose_public_key` and
  `parse_attestation`.

## R14. `--list-serials`: an additive extension beyond upstream

- **Why**: gfh maps YubiKey serials to SSH keys. Under WSL the keys stay attached to Windows, so
  gfh cannot see them over HID or PC/SC in Linux; it runs `windowsfidobridge.exe --list-serials`
  (next to `$SSH_SK_PROVIDER`) through WSL interop instead. Upstream has no such mode, and
  without the flag the executable behaves exactly as before (Principle I is unaffected).
- **Contract**: stdout carries only the serials of the YubiKeys attached to Windows, one decimal
  per line (`\n`), deduplicated, in reader order; empty when none. Exit 0, including when the
  smart card service is absent or stopped or there are no readers (`SCARD_E_NO_SERVICE`,
  `SCARD_E_SERVICE_STOPPED`, `SCARD_E_NO_READERS_AVAILABLE`); any other PC/SC failure exits
  non-zero with a message on stderr. A key that cannot be read is skipped with a warning. stdin
  is not read and no window or WebAuthn UI is shown.
- **Why PC/SC**: WinSCard needs no administrator rights, unlike raw FIDO HID access on Windows,
  and YubiKeys with CCID enabled appear as readers named `… YubiKey …`.
- **APDUs** (`crates/webauthn-bridge/src/serial.rs`), verified against yubikit
  (yubikey-manager 5.9.2 `management.py` `read_device_info`, `yubiotp.py` `get_serial`,
  `core/smartcard`) and the `yubikey` crate 0.6 (`transaction.rs` `get_serial`, used by gfh):
  1. SELECT Management `00 A4 04 00 08 A0 00 00 05 27 47 11 17`, then READ CONFIG page 0
     `00 1D 00 00 00` (YubiKey ≥ 4.1): a length byte then TLVs; tag `0x02` is the serial,
     big-endian.
  2. Fallback (YubiKey NEO/4 before 4.1, or Management unavailable): SELECT OTP
     `00 A4 04 00 07 A0 00 00 05 27 20 01`, then `00 01 10 00 00` (`INS_CONFIG`, slot
     `DEVICE_SERIAL`): a 4-byte big-endian serial.
  Only SW `90 00` is success; `61 xx` is followed by GET RESPONSE `00 C0 00 00 00`, as yubikit's
  `ResponseChainingProcessor`. A serial of 0 or a missing tag means "no serial" (yubikit), as
  with the serial-visibility flag off.
