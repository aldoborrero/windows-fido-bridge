# Feature Specification: Rust rewrite of windows-fido-bridge with upstream parity

**Feature Branch**: `001-rust-rewrite`

**Created**: 2026-10-16

**Status**: Draft

**Input**: User description: "Rust rewrite of windows-fido-bridge reaching parity with the C++ implementation: sk_api_version/sk_enroll/sk_sign/sk_load_resident_keys over the CBOR stdin/stdout protocol to windowsfidobridge.exe via WSL interop; Ed25519 and ECDSA P-256; PIN / user-verification flags; debug logging via WINDOWS_FIDO_BRIDGE_DEBUG and WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION; existing credentials enrolled via libfido2 must keep working."

## Context

A developer works in WSL 2 and authenticates with FIDO security keys (YubiKeys) holding
`ed25519-sk` / `ecdsa-sk` OpenSSH credentials. Passing the key into WSL over USB/IP is
unreliable, so the key stays attached to Windows and OpenSSH inside WSL reaches it through a
security key middleware that forwards each operation to Windows' WebAuthn service. The upstream
C++ middleware (pinned commit in the `justfile`, copy in `.scratch/windows-fido-bridge-cpp/`) does
this today and has been verified on the developer's machine: it signed with the existing key
`id_ed25519_sk_5c-github` (application `ssh:5c-github`, flags `0x21`). This feature replaces it
with an equivalent, maintainable implementation that behaves the same from OpenSSH's and the
user's point of view.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Sign with an existing security key (Priority: P1)

The developer points OpenSSH at the middleware and signs (git commit signing via
`ssh-keygen -Y sign`, or SSH authentication) with a key they enrolled earlier through libfido2.
Windows shows its own "touch your security key" prompt; after the touch the signature is
produced and verifies against the existing public key.

**Why this priority**: Signing with existing credentials is the daily use and the reason the
project exists; it delivers value without enrollment.

**Independent Test**: With the key attached to Windows (detached from WSL), run
`just try-sign` with `~/.ssh/id_ed25519_sk_5c-github`; the result is a `Good "git" signature`.

**Acceptance Scenarios**:

1. **Given** an `ed25519-sk` key handle enrolled through libfido2 and the key attached to
   Windows, **When** OpenSSH requests a signature through the middleware and the user touches
   the key, **Then** OpenSSH receives a signature, counter and flags that verify against the
   existing public key.
2. **Given** an `ecdsa-sk` (P-256) key handle, **When** a signature is requested and the key is
   touched, **Then** OpenSSH receives the signature split into its r and s components and it
   verifies.
3. **Given** the request carries the "user verification required" flag, **When** signing,
   **Then** Windows asks for the key's PIN or biometric before signing.
4. **Given** the user cancels the Windows prompt or the key is absent, **When** signing,
   **Then** OpenSSH receives a failure result and no signature, and the WSL process does not
   hang or crash.

---

### User Story 2 - Enroll a new security key credential (Priority: P2)

The developer runs `ssh-keygen -t ed25519-sk` (or `ecdsa-sk`) with the middleware. Windows asks
them to touch (and if required verify) the key, and OpenSSH writes a new key pair whose private
file contains a key handle usable for later signing (Story 1).

**Why this priority**: Needed to create new credentials without USB/IP, but enrollment is rare
compared to signing.

**Independent Test**: `ssh-keygen -t ed25519-sk -w <middleware> -f /tmp/k` creates a key; signing
with it via Story 1 produces a verifying signature.

**Acceptance Scenarios**:

1. **Given** the key attached to Windows, **When** enrolling an Ed25519 credential, **Then**
   OpenSSH receives the public key, key handle, attestation signature and (when the
   authenticator provides one) attestation certificate, and saves a usable key pair.
2. **Given** an ECDSA P-256 enrollment, **When** it completes, **Then** the public key returned
   to OpenSSH is the uncompressed point form.
3. **Given** the OpenSSH option `user=<name>` is passed, **When** enrolling, **Then** that name is
   the user name shown by Windows; otherwise "SSH user" is used.
4. **Given** the authenticator uses self-attestation ("packed" format without certificate),
   **When** enrolling, **Then** enrollment succeeds without an attestation certificate.

---

### User Story 3 - Diagnose problems and force user verification (Priority: P3)

When something fails, the developer sets `WINDOWS_FIDO_BRIDGE_DEBUG` and gets detailed logs on
standard error from both the WSL and Windows sides (parameters received, messages exchanged,
parsed authenticator data). Setting `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` makes every
operation require user verification regardless of OpenSSH flags.

**Why this priority**: Operational support; the bridge works without it.

**Independent Test**: Run `just try-sign` with `WINDOWS_FIDO_BRIDGE_DEBUG=1` and observe debug
lines from both sides; run with `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION=1` and observe the
PIN/biometric prompt.

**Acceptance Scenarios**:

1. **Given** `WINDOWS_FIDO_BRIDGE_DEBUG` is set to any value, **When** an operation runs,
   **Then** both sides emit debug output to standard error and never print the PIN value.
2. **Given** the variable is unset, **When** an operation runs, **Then** only warnings and
   errors are printed.
3. **Given** `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` is set, or the application is
   `ssh:windows-fido-bridge-verify-required`, **When** signing or enrolling, **Then** user
   verification is required.

---

### Edge Cases

- The Windows executable is missing next to the library or cannot be started: the operation
  fails with a general error and a message on standard error; OpenSSH does not hang.
- The Windows side exits with a non-zero status or closes its output early: general error.
- A message exceeds the maximum frame size (32768 bytes) or has a malformed length header:
  rejected with a general error.
- An algorithm other than Ed25519 or ECDSA P-256 is requested: the Windows side answers
  "unsupported" for signing and fails enrollment; as upstream, the WSL side reports any non-zero
  answer to OpenSSH as a general error.
- The ECDSA signature from the authenticator is not the expected DER structure (sequence of two
  32/33-byte integers): general error rather than a malformed signature.
- The enrollment public key uses an unsupported COSE key type, algorithm or curve, compressed EC
  point form, or extra parameters: enrollment fails.
- A "fido-u2f" attestation without certificate: enrollment fails.
- OpenSSH asks to load resident keys: the middleware reports "unsupported".
- A PIN supplied by OpenSSH: it is not forwarded; Windows collects it in its own dialog.
- Internal failures never terminate or corrupt the calling OpenSSH process.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The middleware MUST report OpenSSH security key API version `0x000a0000`
  (OpenSSH 9.1 and later).
- **FR-002**: The middleware MUST implement signing for Ed25519 and ECDSA P-256 credentials,
  returning flags, signature counter and signature components (Ed25519: one 64-byte value;
  ECDSA: 32-byte r and s) as OpenSSH expects.
- **FR-003**: The middleware MUST implement enrollment for Ed25519 and ECDSA P-256, returning
  public key, key handle, attestation signature, attestation certificate when present, and
  authenticator data (empty, as upstream).
- **FR-004**: Loading resident keys MUST return "unsupported", as upstream. Any non-zero answer
  from the Windows side MUST be reported to OpenSSH as a general error, as upstream.
- **FR-005**: Each operation MUST be forwarded to a Windows executable named
  `windowsfidobridge.exe` located in the same directory as the middleware library, started
  through WSL interop, exchanging exactly one request and one response.
- **FR-006**: Request and response messages MUST use the upstream wire format: a decimal ASCII
  length followed by `|` and that many bytes of CBOR, with the upstream field names and types
  (see plan contracts), so either side remains interoperable with the upstream counterpart.
- **FR-007**: The Windows side MUST perform operations through the Windows WebAuthn service,
  using the OpenSSH application string as relying-party identifier and the OpenSSH data as the
  client data hashed with SHA-256, so credentials enrolled through libfido2 keep working.
- **FR-008**: User verification MUST be required when OpenSSH sets the
  user-verification-required flag, when `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` is set,
  or when the application is `ssh:windows-fido-bridge-verify-required`; otherwise it MUST be
  discouraged.
- **FR-009**: The WebAuthn prompts MUST appear in front of other windows (upstream uses an
  invisible always-on-top owner window), including when started from Windows Terminal.
- **FR-010**: `WINDOWS_FIDO_BRIDGE_DEBUG` and `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION`
  MUST be propagated from WSL to the Windows process (via `WSLENV`, preserving existing entries).
- **FR-011**: Debug logging MUST be enabled by `WINDOWS_FIDO_BRIDGE_DEBUG` (any value), go to
  standard error, and default to warnings and errors only; PIN values MUST never be logged.
- **FR-012**: Any failure (bad input, missing executable, malformed message, WebAuthn error,
  cancellation) MUST be reported to OpenSSH as an error code; the middleware MUST NOT crash,
  abort or hang the calling process, and MUST NOT leak memory it allocated on error paths.
- **FR-013**: Memory handed to OpenSSH MUST be releasable by OpenSSH with the C allocator.
- **FR-014**: The deliverable MUST be buildable reproducibly from this repository with one
  command, producing both files side by side in one output directory.

### Key Entities

- **Request**: operation type (`sk_enroll` or `sk_sign`) and its parameters: algorithm,
  application, flags, options, and either challenge (enroll) or data plus key handle (sign).
- **Response**: return code and, on success, enrollment results (public key, key handle,
  signature, attestation certificate, authenticator data) or signing results (flags, counter,
  signature r, optional s).
- **Option**: OpenSSH security key option with name, value and required flag (e.g. `user`).
- **Authenticator data**: relying-party hash, flags, signature counter and, on enrollment, the
  attested credential (identifier and COSE public key).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Signing with the existing `id_ed25519_sk_5c-github` credential succeeds and
  verifies on the first attempt, with no re-enrollment and no change on the server side.
- **SC-002**: A signing operation completes within 2 seconds of the user's touch (excluding the
  time the user takes to touch).
- **SC-003**: 100% of failure scenarios listed in Edge Cases end with an error reported to the
  caller and no crashed or hung process.
- **SC-004**: Every message format and signature/key conversion has automated checks that run
  on every change, using fixtures derived from upstream behaviour.
- **SC-005**: The rewrite is a drop-in replacement: substituting it for the upstream files in
  the same directory requires no other configuration change.
- **SC-006**: The rewritten code base is smaller than the upstream implementation (≈3,700 lines
  of code excluding vendored headers).

## Assumptions

- Target environment is WSL 2 on x86_64 Windows 10 1903+ / Windows 11 with OpenSSH ≥ 9.1 inside
  WSL; Cygwin support (upstream's Windows-side library variant) is out of scope.
- Older OpenSSH API versions (5, 7, 9) are out of scope; only version 10 is supported.
- The PIN passed by OpenSSH is ignored, as upstream: Windows collects PIN or biometrics itself.
- `sk_options` are forwarded; only `user` has an effect (enrollment user name), as upstream.
- Enrollment authenticator data returned to OpenSSH stays empty, as upstream; filling it is a
  possible later improvement, not part of parity.
- The upstream "user id" placeholder `(null)` and relying-party name
  `OpenSSH via windows-fido-bridge` are kept for enrollment.
- Hardware tests (`just try-sign`, enrollment) are run manually by the developer; automated
  checks cover everything that does not need a key or Windows.
- No feature git branch is created yet because the repository has no initial commit; work starts
  in `.worktrees/001-rust-rewrite` (`just worktree 001-rust-rewrite`) once `main` has a commit.
