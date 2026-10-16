<!--
Sync Impact Report
- Version change: (template) → 1.0.0
- Modified principles: placeholders → I. Upstream Behavioural Parity, II. Unsafe Confined to the FFI
  Boundary, III. Simplicity First (YAGNI), IV. Platform-Independent, Tested Core,
  V. Nix Is the Build of Record
- Added sections: Technical Constraints, Development Workflow, Governance
- Removed sections: none
- Templates requiring updates: none (plan/spec/tasks templates read this file at runtime) ✅
- Follow-up TODOs: none
-->

# windows-fido-bridge Constitution

## Core Principles

### I. Upstream Behavioural Parity

The Rust rewrite MUST behave exactly like the upstream C++ implementation pinned in the `justfile`
(checked out in `.scratch/windows-fido-bridge-cpp/`) unless a deviation is written down in the
feature spec with its reason. Parity covers:

- the CBOR request/response wire format between the Linux library and the Windows executable;
- the conversion of WebAuthn results into OpenSSH `sk_enroll_response` / `sk_sign_response`;
- the OpenSSH security key API version (`SSH_SK_VERSION_MAJOR` = `0x000a0000`, OpenSSH ≥ 9.1);
- the deployment contract: `windowsfidobridge.exe` lives in the same directory as
  `libwindowsfidobridge.so`, which locates it through its own path;
- the debug and behaviour switches `WINDOWS_FIDO_BRIDGE_DEBUG` and
  `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION`.

Rationale: the C++ is proven against the user's real keys; parity lets existing credentials keep
working and gives every behaviour question a single, checkable answer.

### II. Unsafe Confined to the FFI Boundary

- `unsafe` code MUST appear only where Rust meets a foreign ABI: the `extern "C"` entry points
  OpenSSH calls, and the `webauthn.dll` / Win32 calls on Windows.
- Every `unsafe` block MUST carry a `// SAFETY:` comment stating the invariant it relies on.
- Every pointer handed to OpenSSH that OpenSSH later releases with `free()` MUST be allocated with
  `libc::calloc` / `libc::malloc`, never with Rust's global allocator.
- Panics MUST NOT unwind across an `extern "C"` boundary; entry points convert failures into
  OpenSSH error codes.

Rationale: the whole value of the rewrite is memory safety; it is lost if unsafe spreads.

### III. Simplicity First (YAGNI)

- Prefer the standard library; add a crate only when it replaces substantial hand-written code
  (e.g. `ciborium`/`serde` for CBOR, `windows` for WebAuthn bindings, `libc` for allocation).
- No speculative abstractions, traits or configuration that the spec does not require.
- Features absent from upstream are out of scope unless a spec adds them explicitly.

Rationale: a small, obvious codebase is what makes an unmaintained upstream safe to own.

### IV. Platform-Independent, Tested Core

- Logic that can run on Linux (CBOR encoding/decoding, request validation, WebAuthn authenticator
  data parsing, signature format conversion) MUST live in code that compiles and is unit-tested on
  `x86_64-unknown-linux-gnu`.
- Windows-only code MUST stay a thin adapter over `webauthn.dll`.
- Tests for conversions MUST use fixtures taken from upstream behaviour (e.g. its debug output).
- The end-to-end check is `just try-sign` against real hardware with the key attached to Windows;
  it MUST pass before a feature touching signing or enrollment is considered done.

Rationale: Windows binaries cannot run in the Nix build sandbox, so correctness must be provable
where tests can run.

### V. Nix Is the Build of Record

- `nix flake check` (crane build + tests, clippy with `--deny warnings`, treefmt check) MUST pass
  on every change.
- The Rust toolchain comes only from `rust-toolchain.toml`; the Windows executable is
  cross-compiled to `x86_64-pc-windows-gnu` with nixpkgs' mingw-w64.
- `Cargo.lock` is committed and builds use `--locked`.

Rationale: one reproducible build path for the developer, CI and the NixOS configuration that
consumes the package.

## Technical Constraints

- Targets: Linux library for WSL 2 (`x86_64-unknown-linux-gnu`, `cdylib` named
  `windowsfidobridge`) and Windows executable (`x86_64-pc-windows-gnu`, binary
  `windowsfidobridge.exe`).
- Supported algorithms: Ed25519 and ECDSA P-256, as upstream.
- Licence: Apache-2.0, with upstream attribution kept in `NOTICE`.

## Development Workflow

- Features are developed on branches checked out under `.worktrees/` (`just worktree <branch>`),
  never directly on `main`.
- Specs, plans and tasks live under `specs/` and follow the Spec Kit flow
  (specify → plan → tasks → implement).
- New files MUST be `git add`-ed before running any `nix` command (flakes only see tracked files).
- Commits are signed by the user's own key; agents do not bypass signing configuration.

## Governance

This constitution supersedes other practice documents; `AGENTS.md` carries the day-to-day
commands and MUST stay consistent with it. Amendments are made through `/speckit-constitution`,
recorded with a semantic version bump (MAJOR: principle removed or redefined; MINOR: principle or
section added or materially expanded; PATCH: wording) and a Sync Impact Report. Every plan's
Constitution Check and every review MUST verify compliance; any deviation MUST be justified in the
plan's Complexity Tracking table.

**Version**: 1.0.0 | **Ratified**: 2026-10-16 | **Last Amended**: 2026-10-16
