---

description: "Task list for the Rust rewrite of windows-fido-bridge"
---

# Tasks: Rust rewrite of windows-fido-bridge with upstream parity

**Input**: Design documents from `/specs/001-rust-rewrite/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included. Constitution principle IV and SC-004 require unit tests for every
platform-independent conversion and message format, using the golden fixtures in
`crates/protocol/tests/fixtures/` (see its README for their origin).

**Organization**: Tasks are grouped by user story (US1 sign, US2 enroll, US3 diagnostics).

**Reference**: upstream C++ in `.scratch/windows-fido-bridge-cpp/src/` (run `just upstream` if
missing). File references below such as `linux_middleware/src/bridge.cpp` are relative to that
directory.

**Before starting**: `main` needs an initial commit (signed by the user). Then work in a
worktree: `just worktree 001-rust-rewrite` and `cd .worktrees/001-rust-rewrite`. `git add` new
files before any `nix` command.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)

## Path Conventions

Cargo workspace: `crates/protocol/` (package `wfb-protocol`), `crates/sk-middleware/` (cdylib
`windowsfidobridge`), `crates/webauthn-bridge/` (bin `windowsfidobridge`); Nix under `nix/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace, dependencies and test plumbing

- [X] T001 Create crate `crates/protocol/` with `Cargo.toml` (package `wfb-protocol`, workspace version/edition/license, deps `ciborium = "0.2"`, `serde = { version = "1", features = ["derive"] }`, `serde_bytes = "0.11"`) and `src/lib.rs` with `#![forbid(unsafe_code)]` declaring modules `sk`, `message`, `frame`, `log` (create the four files empty)
- [X] T002 [P] Add dependencies to `crates/sk-middleware/Cargo.toml`: `wfb-protocol = { path = "../protocol" }`, `libc = "0.2"`
- [X] T003 [P] Add dependencies to `crates/webauthn-bridge/Cargo.toml`: `wfb-protocol = { path = "../protocol" }`, `ciborium` (for the attestation object; `serde`/`serde_bytes` were dropped as unused in review), and `[target.'cfg(windows)'.dependencies] windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_Networking_WindowsWebServices", "Win32_UI_WindowsAndMessaging", "Win32_System_LibraryLoader", "Win32_System_Threading"] }`
- [X] T004 [P] Put the golden fixtures `sign-request.hex` and `sign-response.hex` in `crates/protocol/tests/fixtures/` (their only copy; specs reference that path)
- [X] T005 [P] Add `nix/checks/tests.nix`: crane `cargoTest` over `craneLib.cleanCargoSource flake` with `cargoArtifacts = craneLib.buildDepsOnly { … }` and `cargoTestExtraArgs = "--workspace"`, mirroring `nix/checks/clippy.nix` (must include the `.hex` fixtures in the source filter: extend `cleanCargoSource` with a filter keeping `*.hex`)
- [X] T006 Run `cargo generate-lockfile`/`cargo build --workspace`, `git add -A`, and confirm `nix flake check` passes with empty modules; commit `Cargo.lock`

**Checkpoint**: workspace builds with all three crates; `checks.x86_64-linux` lists `tests`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared protocol, C ABI plumbing, process bridge and Windows scaffolding used by all
stories. No story work starts before this phase is done.

### Tests (write first, see them fail)

- [X] T007 [P] Unit tests in `crates/protocol/src/message.rs`: decode `tests/fixtures/sign-request.hex` into `Request::Sign` (`alg` 1, `application` "ssh:5c-github", `data` 95 bytes, `flags` 33, `key_handle` 48 bytes, empty `sk_options`), re-encode and compare byte-for-byte; same for `sign-response.hex` (`return_code` 0, `counter` 689, `flags` 1, `sig_r` 64 bytes, no `sig_s`); assert byte strings encode as CBOR major type 2 and `SkOption.required` as an unsigned integer `0`/`1` ("never a CBOR boolean"); failure response `{"return_code": -1}` has no `response_parameters`
- [X] T008 [P] Unit tests in `crates/protocol/src/frame.rs`: round-trip `"<len>|<payload>"`; reject a non-digit before `|`; reject a declared size "> 32768"; accept exactly 32768; error on EOF before `|` and on short payload

### Implementation

- [X] T009 [P] Implement `crates/protocol/src/sk.rs`: `SSH_SK_VERSION_MAJOR = 0x000a0000`, flag `USER_VERIFICATION_REQD 0x04`, algs `ECDSA 0x00`, `ED25519 0x01`, errors `GENERAL -1`, `UNSUPPORTED -2` (values from `contracts/sk-api.md`; the unused constants were removed in review)
- [X] T010 Implement `crates/protocol/src/message.rs` per `data-model.md`: `Request` tagged by `request_type` (`"sk_enroll"` | `"sk_sign"`) with `request_parameters`; `EnrollParams { alg, application, challenge, flags, sk_options }`; `SignParams { alg, application, data, flags, key_handle, sk_options }`; `SkOption { name, required: u8, value }`; `Response { return_code: i32, response_parameters: Option<…> }` with `EnrollResult { attestation_cert, authdata, key_handle, public_key, signature }` and `SignResult { counter: u32, flags: u8, sig_r: Option<bytes>, sig_s: Option<bytes> }`; fields declared in lexicographic order, `#[serde(with = "serde_bytes")]` on all byte fields, `skip_serializing_if = "Option::is_none"` on optional ones; `encode`/`decode` helpers over ciborium (makes T007 pass)
- [X] T011 [P] Implement `crates/protocol/src/frame.rs`: `write(w, payload)` and `read(r) -> Vec<u8>` per `contracts/wire-protocol.md` (makes T008 pass)
- [X] T012 [P] Implement `crates/protocol/src/log.rs`: `init(side: &'static str)` reading `WINDOWS_FIDO_BRIDGE_DEBUG` (set → debug, unset → warn); `debug!`/`warn!`/`error!`-style functions or macros writing `[<timestamp>] [<side>] [<level>] msg` to stderr; `hex_dump(bytes, indent)` in upstream's `  | 0000: …` layout (see `common/src/util.cpp`)
- [X] T013 Implement `crates/sk-middleware/src/ffi.rs`: `#[repr(C)]` `SkOption`, `SkEnrollResponse`, `SkSignResponse` exactly as in `contracts/sk-api.md`; safe helpers `c_str_or_empty(*const c_char) -> String` (NULL → empty), `slice_or_empty(ptr, len)` (len 0 or NULL → empty), `parse_options(*const *const SkOption) -> Vec<wfb_protocol::SkOption>` (NULL-terminated, NULL array → empty); `calloc_bytes(&[u8]) -> (*mut u8, usize)` (empty → null/0) and an owning guard that `libc::free`s every buffer and the struct on drop until `into_raw()`; every `unsafe` block with `// SAFETY:`
- [X] T014 [P] Unit tests in `crates/sk-middleware/src/ffi.rs`: option parsing from a hand-built NULL-terminated array, NULL handling, guard frees on drop (run under the test allocator, no leaks/crash), `into_raw` hands ownership and fields match the input bytes
- [X] T015 Implement `crates/sk-middleware/src/bridge.rs`: `library_dir()` via `libc::dladdr` on a function in this crate (upstream `linux_middleware/src/bridge.cpp: get_library_path`); `invoke(payload: &[u8]) -> io::Result<Vec<u8>>` spawning `<library_dir>/windowsfidobridge.exe` with `Command` (stdin/stdout piped, stderr inherited), `WSLENV` set by `wslenv_value(existing: Option<&str>) -> String`, write frame, read frame, `wait()` and error on non-zero exit; split as `invoke_exe(exe: &Path, payload)` so a unit test can assert that a missing executable returns `Err` (spec edge case, SC-003)
- [X] T016 Implement `crates/sk-middleware/src/lib.rs` entry points scaffold: `sk_api_version` (keep), `sk_load_resident_keys` returning `SSH_SK_ERR_UNSUPPORTED` (-2), and a private `guard(f: impl FnOnce() -> Result<i32, Error>) -> c_int` that wraps `std::panic::catch_unwind` and maps errors/panics to `-1`
- [X] T017 Implement `crates/webauthn-bridge/src/main.rs` skeleton: on `cfg(windows)` init log with side `win32-bridge`, `frame::read(stdin)`, decode `Request`, dispatch to `handle_enroll`/`handle_sign` (stubs returning `return_code -1`), encode and `frame::write(stdout)`, exit 0; unknown `request_type` → log + exit 1; on `cfg(not(windows))` print an error and exit 1 so the crate builds and tests on Linux
- [X] T018 Implement `crates/webauthn-bridge/src/window.rs` (`#[cfg(windows)]`): `run<T: Send>(f: impl FnOnce(HWND) -> T + Send) -> T` creating an invisible `WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, `WS_POPUPWINDOW` window (upstream `win32_middleware_common/src/window.cpp`), running `f` on a `std::thread` while pumping messages (`MsgWaitForMultipleObjects` on the thread handle), then `WM_CLOSE` and drain
- [X] T019 [P] Unit tests + implementation in `crates/webauthn-bridge/src/convert.rs` (`#![deny(unsafe_code)]`, no `cfg(windows)`): `AuthenticatorData::parse(&[u8])` → `rp_id_hash[32]`, `flags`, `sign_count` (u32 big-endian), optional `AttestedCredential { aaguid[16], id, cose_key_bytes }` when flag `0x40` is set (WebAuthn §6.1, upstream `webauthn.cpp: authenticator_data::parse`); tests with the sign fixture values (`flags` 1, counter 689) and a synthetic attested-credential buffer; errors on truncated input
- [X] T020 Unit tests + implementation in `crates/webauthn-bridge/src/convert.rs`: `user_verification_required(application: &str, flags: u8, force_env: bool) -> bool` = `force_env || application == "ssh:windows-fido-bridge-verify-required" || flags & 0x04 != 0` (upstream `openssh_sk_middleware.cpp: is_user_verification_required`)
- [X] T021 Add `nix flake check` gate: `git add -A && nix flake check` green (tests, clippy `-D warnings`, treefmt)

**Checkpoint**: protocol round-trips upstream fixtures; the .so can spawn the .exe and exchange a
frame; the .exe builds for Windows and answers `-1` to everything.

---

## Phase 3: User Story 1 - Sign with an existing security key (Priority: P1) 🎯 MVP

**Goal**: `ssh-keygen -Y sign` / `ssh` succeed with an existing libfido2-enrolled credential
through Windows WebAuthn.

**Independent Test**: key detached from WSL; `just try-sign ~/.ssh/id_ed25519_sk_5c-github` →
`Good "git" signature` (quickstart §4).

### Tests for User Story 1

- [X] T022 [P] [US1] Unit tests in `crates/webauthn-bridge/src/convert.rs` for `ed25519_signature(&[u8]) -> Vec<u8>`: exactly 64 bytes accepted as `sig_r` (use the `sig_r` bytes from `sign-response.hex`), other lengths rejected
- [X] T023 [US1] Unit tests in `crates/webauthn-bridge/src/convert.rs` for `ecdsa_signature(&[u8]) -> (r, s)`: DER with 32/32, 33/32 (leading `0x00`), 32/33 and 33/33 integers → 32-byte r and s; reject wrong tag, "SEQUENCE length between 68 and 70", INTEGER length not 32/33, 33-byte INTEGER without leading `0x00` (upstream `webauthn.cpp: parse_ecdsa_sk_signature`)
- [X] T024 [P] [US1] Unit test in `crates/sk-middleware/src/lib.rs` (or `ffi.rs`): building an `SkSignResponse` from `SignResult { counter: 689, flags: 1, sig_r: 64 bytes, sig_s: None }` sets `sig_s` null/0 and copies `sig_r`

### Implementation for User Story 1

- [X] T025 [US1] Implement `ed25519_signature` and `ecdsa_signature` in `crates/webauthn-bridge/src/convert.rs` (makes T022/T023 pass)
- [X] T026 [US1] Implement `get_assertion(hwnd, application, data, key_handle, uv_required) -> Assertion { authenticator_data, signature }` in `crates/webauthn-bridge/src/webauthn.rs` (`#[cfg(windows)]`): UTF-16 RP id, `WEBAUTHN_CLIENT_DATA` with the data bytes and `WEBAUTHN_HASH_ALGORITHM_SHA_256`, one `WEBAUTHN_CREDENTIAL` (public-key type) from the key handle, UV `REQUIRED`/`DISCOURAGED`, copy results and `WebAuthNFreeAssertion`; on non-`S_OK` return an error with `WebAuthNGetErrorName` text (upstream `webauthn.cpp: create_windows_webauthn_assertion`)
- [X] T027 [US1] Implement `handle_sign` in `crates/webauthn-bridge/src/main.rs`: `alg` not 0/1 → `return_code -2`; UV via `convert::user_verification_required` with `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` presence; `window::run(|h| webauthn::get_assertion(...))`; parse authenticator data → `flags`, `counter`; alg 1 → `sig_r` = 64-byte signature, alg 0 → `sig_r`/`sig_s` from DER; any error → `return_code -1` with an error log
- [X] T028 [US1] Implement `sk_sign` in `crates/sk-middleware/src/lib.rs`: parse inputs via `ffi.rs`, build `Request::Sign`, `bridge::invoke`, decode `Response`; non-zero `return_code` → -1; otherwise calloc `SkSignResponse` (`flags`, `counter`, `sig_r`, optional `sig_s`) in a guard and write `*sign_response`; PIN ignored (never sent)
- [X] T029 [US1] `just build` and run quickstart §3–4 on hardware: `just try-sign ~/.ssh/id_ed25519_sk_5c-github` → `Good "git" signature`; cancel the Windows dialog once → prompt failure, no hang (acceptance US1-1, US1-4); time the run with `WINDOWS_FIDO_BRIDGE_DEBUG=1` timestamps and check the bridge overhead after the touch is < 2 s (SC-002)
- [X] T030 [US1] If an `ecdsa-sk` credential is available (or after US2), run `just try-sign <ecdsa key>` (acceptance US1-2); otherwise note it as pending in the PR

**Checkpoint**: MVP — daily signing works with the Rust bridge.

---

## Phase 4: User Story 2 - Enroll a new security key credential (Priority: P2)

**Goal**: `ssh-keygen -t ed25519-sk|ecdsa-sk -w <lib>` creates usable keys through Windows.

**Independent Test**: quickstart §5 — both key types enroll after a touch and sign with
`just try-sign`.

### Tests for User Story 2

- [X] T031 [P] [US2] Unit tests in `crates/webauthn-bridge/src/convert.rs` for `cose_public_key(&[u8]) -> Vec<u8>`: OKP/EdDSA/Ed25519 with 32-byte `-2` → x; EC2/ES256/P-256 with byte-string `-2`, `-3` and exactly 5 entries → `0x04 ‖ x ‖ y`; reject other kty/alg/crv, missing params, Ed25519 x not 32 bytes, `-3` not a byte string (compressed form), EC2 with extra entries (upstream `webauthn.cpp: parse_attested_credential_*_public_key`)
- [X] T032 [US2] Unit tests in `crates/webauthn-bridge/src/convert.rs` for `parse_attestation(&[u8]) -> Enrollment { public_key, key_handle, signature, attestation_cert }`: `fmt` must be `"packed"` or `"fido-u2f"`; `attStmt.sig` required; first `x5c` certificate used when present; missing `x5c` allowed only for `"packed"` (empty cert); `authData` must contain attested credential data
- [X] T033 [P] [US2] Unit test in `crates/sk-middleware`: building an `SkEnrollResponse` from an `EnrollResult` with empty `attestation_cert` and empty `authdata` yields null/0 for both; `flags` are the request flags (review fix, research R10)

### Implementation for User Story 2

- [X] T034 [US2] Implement `cose_public_key` and `parse_attestation` in `crates/webauthn-bridge/src/convert.rs` (makes T031/T032 pass)
- [X] T035 [US2] Implement `make_credential(hwnd, alg, application, user, challenge, uv_required) -> Vec<u8>` (attestation object bytes) in `crates/webauthn-bridge/src/webauthn.rs` (`#[cfg(windows)]`): RP `{ id: application, name: "OpenSSH via windows-fido-bridge" }`, user `{ id: b"(null)", name/display name: user }`, one COSE parameter (alg -7 for `alg` 0, -8 for `alg` 1, else error), client data = challenge with SHA-256, UV required/discouraged, free with `WebAuthNFreeCredentialAttestation` (upstream `webauthn.cpp: create_windows_webauthn_credential`)
- [X] T036 [US2] Implement `handle_enroll` in `crates/webauthn-bridge/src/main.rs`: user name from the `user` option or `"SSH user"`; UV decision; `window::run(make_credential)`; `convert::parse_attestation`; reply `EnrollResult { public_key, key_handle, signature, attestation_cert, authdata: empty }`; errors → `-1`
- [X] T037 [US2] Implement `sk_enroll` in `crates/sk-middleware/src/lib.rs`: build `Request::Enroll`, invoke, non-zero → -1, else calloc `SkEnrollResponse` (request flags, five buffers, empty → null/0) in a guard and write `*enroll_response`
- [X] T038 [US2] Hardware validation (quickstart §5): enroll `ed25519-sk` and `ecdsa-sk`, then `just try-sign` each; enroll once with `-O user=test` and check the Windows dialog shows `test` (acceptance US2-1…US2-4)

**Checkpoint**: enrollment and signing both work end to end.

---

## Phase 5: User Story 3 - Diagnose problems and force user verification (Priority: P3)

**Goal**: debug output on both sides and forced UV, as upstream.

**Independent Test**: quickstart §4 with `WINDOWS_FIDO_BRIDGE_DEBUG=1` and with
`WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION=1`.

### Tests for User Story 3

- [X] T039 [P] [US3] Unit tests in `crates/sk-middleware/src/bridge.rs` for `wslenv_value`: `None` → `"WINDOWS_FIDO_BRIDGE_DEBUG:WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION"`; `Some("FOO/u")` → `"FOO/u:WINDOWS_FIDO_BRIDGE_DEBUG:WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION"`; `Some("")` → no leading colon
- [X] T040 [P] [US3] Unit test in `crates/protocol/src/log.rs`: `hex_dump` output format for 0, 15, 16 and 17 bytes matches upstream layout

### Implementation for User Story 3

- [X] T041 [US3] Add debug logging in `crates/sk-middleware/src/lib.rs` and `bridge.rs` mirroring upstream `dump_sk_*_inputs` and `invoke_windows_bridge`: algorithm, challenge/data hex dump, application, key handle hex dump, flags as `0b{:08b}`, `PIN: (present)|(not present)` (never the value), options, request/response CBOR in diagnostic form, exe path, `WSLENV`, child exit status
- [X] T042 [US3] Add debug logging in `crates/webauthn-bridge/src/main.rs`/`convert.rs` mirroring upstream: received request, parsed parameters, parsed authenticator data (`Relying party ID hash`, flags with UP/UV/AT/ED, signature count), attestation format and certificate presence, response sent; warnings/errors always on
- [X] T043 [US3] Hardware validation (quickstart §4): `WINDOWS_FIDO_BRIDGE_DEBUG=1 just try-sign` shows lines from `linux-middleware` and `win32-bridge`; without it only warnings/errors; `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION=1 just try-sign` triggers PIN/biometric (acceptance US3-1…US3-3)

**Checkpoint**: all stories independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T044 [P] Audit `unsafe` (use the `unsafe-checker` and `rust-ffi` skills): only in `crates/sk-middleware/src/{lib,ffi,bridge}.rs` and `crates/webauthn-bridge/src/{webauthn,window}.rs`, each block with `// SAFETY:`; `wfb-protocol` keeps `#![forbid(unsafe_code)]`
- [X] T045 [P] Add `[profile.release]` to `Cargo.toml` (`lto = true`, `codegen-units = 1`, keep `panic = "unwind"`) and confirm `objdump -p result/lib/windowsfidobridge.exe | rg 'DLL Name'` lists only Windows system DLLs
- [X] T046 [P] Update `README.md` (remove "does not sign yet" status, document enrollment) and `AGENTS.md` (layout table with `crates/protocol`, `nix/checks/tests.nix`)
- [X] T047 Interop check (quickstart §6, optional): Rust `.so` + upstream `.exe` and upstream `.so` + Rust `.exe` both sign successfully
- [X] T048 Measure SC-006: `tokei crates` (or `wc -l`) total < upstream ≈3,700 lines; record in the PR description
- [X] T049 Final gate: `git add -A && nix flake check` green; run the `rust-review` and `ponytail-review` skills over the diff and address findings

---

## Phase 7: Code review fixes and follow-ups

- [X] T050 Review fixes: enroll returns the request flags (R10); requests over 32768 bytes rejected before spawning (no SIGPIPE); host fds marked close-on-exec in the child; console subsystem for the exe; null `sig_r`/`sig_s` rejected and success requires `sig_r`; invalid UTF-8 strings rejected; `WSLENV` kept as `OsString`; frame header capped at 5 digits; strict DER and 32-byte P-256 coordinates; `repr(C)` layout asserts; reply mapping extracted to `parse_reply` and tested; golden `enroll-response.hex` from upstream's encoder and `sign-authdata.hex` verified against the real signature; deterministic no-panic loops; workspace lints; shared crane deps and a Windows clippy check (deviations in research R12)
- [ ] T051 `-O resident` on enroll: map `SSH_SK_RESIDENT_KEY` to `bRequireResidentKey`, or reply `SSH_SK_ERR_UNSUPPORTED` instead of silently creating a non-resident credential
- [X] T052 Restrict enrollment to cross-platform authenticators (`dwAuthenticatorAttachment`) and decide on the attestation conveyance preference, so Windows Hello cannot be picked for an `-sk` key
- [ ] T053 Cancellation and timeout: set `dwTimeoutMilliseconds` and map a cancelled dialog (`NTE_USER_CANCELLED`) to a clean error; consider killing the child when OpenSSH gives up
- [ ] T054 Pass OpenSSH error codes `-5..-1` through from the bridge (e.g. `SSH_SK_ERR_DEVICE_NOT_FOUND`, `SSH_SK_ERR_PIN_REQUIRED`) instead of collapsing every non-zero `return_code` to `-1` (re-add the needed `sk.rs` constants then)
- [ ] T055 `library_dir`: canonicalize the `dli_fname` path (a relative `SSH_SK_PROVIDER` resolved against a later `chdir` breaks `path::absolute`)
- [ ] T056 `child.wait()` failing with `ECHILD` (host process sets `SIGCHLD` to `SIG_IGN`): treat as "exit status unknown" and trust a well-formed reply, or document
- [ ] T057 RAII guards for `WEBAUTHN_ASSERTION` / `WEBAUTHN_CREDENTIAL_ATTESTATION` in `webauthn.rs` instead of manual free calls on each path
- [ ] T058 `window::run` with `std::thread::scope` so the request fields are borrowed rather than cloned into the worker
- [ ] T059 Block `SIGPIPE` around the request write (`pthread_sigmask`, then drain with `sigtimedwait`) so a bridge that dies before reading stdin cannot kill the host process (only the > 32768 byte case is handled today)
- [X] T060 `windowsfidobridge.exe --list-serials` for gfh (research R14): `serial.rs` (APDUs, response chaining, DeviceInfo TLV and OTP serial parsing, reader filter, dedup; unit-tested on Linux) and `pcsc.rs` (WinSCard with RAII context/card guards); README and AGENTS.md updated
- [X] T061 Hardware validation: run `result/lib/windowsfidobridge.exe --list-serials` with the key attached to Windows; it prints the serial `ykman list --serials` shows on Windows (empty output and exit 0 with no key)

---

## Dependencies & Execution Order

### Phase Dependencies

- Setup (Phase 1) → Foundational (Phase 2) → US1 (Phase 3) → US2 (Phase 4) → US3 (Phase 5) → Polish
- US2 depends on Foundational only (it reuses `convert::AuthenticatorData` and the UV function
  from Phase 2); it can start in parallel with US1 if staffed.
- US3 depends on US1 and US2 code paths existing (it adds logging to them); T039/T040 can start
  right after Phase 2.

### Within Each Story

- Tests first (they must fail), then conversions, then Windows calls, then handlers, then C
  entry points, then hardware validation.

### Parallel Opportunities

- Phase 1: T002, T003, T004, T005 in parallel after T001.
- Phase 2: T007/T008 (tests) in parallel; T009, T011, T012 in parallel; T014 and T019 in
  parallel once their modules exist (T020 after T019: same file).
- US1: T022 and T024 in parallel (T023 after T022: same file).
- US2: T031 and T033 in parallel (T032 after T031: same file).
- US3: T039, T040 in parallel.

## Parallel Example: User Story 1

```text
Task: "T022 [US1] ed25519_signature tests in crates/webauthn-bridge/src/convert.rs"
Task: "T024 [US1] SkSignResponse construction test in crates/sk-middleware"
```

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 + Phase 2.
2. Phase 3 (sign). Stop and validate with `just try-sign` on the real key.
3. The bridge is usable for daily git signing at this point.

### Incremental Delivery

1. + US2 → enrollment without USB/IP.
2. + US3 → diagnostics parity.
3. Polish → release profile, docs, interop and size checks.
