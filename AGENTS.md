# AGENTS.md

Rust rewrite of windows-fido-bridge: an OpenSSH security key middleware that lets OpenSSH in WSL sign with a FIDO key attached to Windows, via the Windows WebAuthn API.

## Layout

| Path | What |
| --- | --- |
| `crates/protocol/` | lib `wfb-protocol`: `sk-api.h` constants, CBOR messages (`message.rs`), length-prefixed framing (`frame.rs`), stderr logger; golden fixtures (only copy, provenance in its README) in `tests/fixtures/` |
| `crates/sk-middleware/` | `cdylib` → `libwindowsfidobridge.so`; C ABI from OpenSSH `sk-api.h` (API v10): `lib.rs` entry points, `ffi.rs` structs + calloc, `bridge.rs` spawns the exe |
| `crates/webauthn-bridge/` | bin → `windowsfidobridge.exe` (x86_64-pc-windows-gnu); `convert.rs` (WebAuthn → OpenSSH, tested on Linux), `serial.rs` (`--list-serials` APDUs and parsing, tested on Linux), `webauthn.rs` + `window.rs` + `pcsc.rs` (Windows only) |
| `nix/` | blueprint prefix: `devshell.nix`, `formatter.nix`, `modules/{nixos,home}/windows-fido-bridge.nix` (set `SSH_SK_PROVIDER` + ssh-agent `-P`), `lib/` (toolchain, crane args and the shared Linux/Windows `-deps` builds), `packages/`, `checks/` (`clippy.nix`, `clippy-windows.nix` = Windows-target clippy, `tests.nix` = `cargo test --workspace`, the only test run, `nixos-module.nix` = module eval check) |
| `rust-toolchain.toml` | single source of truth for the Rust toolchain, pinned to an exact release (devshell and Nix builds) |
| `specs/`, `.specify/` | Spec Kit specs, plans and tasks |
| `.scratch/windows-fido-bridge-cpp/` | C++ reference at the pinned commit (`just upstream`), gitignored |
| `.worktrees/` | feature-branch worktrees (`just worktree <branch>`), gitignored |

## Active work

Feature `001-rust-rewrite`: spec, plan, contracts and dependency-ordered tasks in `specs/001-rust-rewrite/` (start with `tasks.md`; principles in `.specify/memory/constitution.md`). Implemented on branch `001-rust-rewrite` (`.worktrees/001-rust-rewrite`); hardware validation tasks in `tasks.md` remain.

## Commands

| Task | Command |
| --- | --- |
| list recipes | `just` |
| tests | `just test` |
| lint | `just lint` |
| format | `just fmt` |
| everything CI runs | `just check` (`nix flake check`) |
| build `.so` + `.exe` | `just build` → `result/lib/` |
| end-to-end signing test | `just try-sign [key]` (key attached to Windows, needs a touch) |

## Invariants

- The upstream C++ in `.scratch/windows-fido-bridge-cpp/` (commit pinned in the `justfile`) is the behavioural reference. Port its wire handling (CBOR request/response, WebAuthn → OpenSSH signature conversion) faithfully; the end-to-end test against a real key is `just try-sign`.
- The library finds the executable next to itself (`dladdr`); the Nix package installs both into `lib/`. Keep that contract.
- Memory returned to OpenSSH (`sk_enroll_response`, `sk_sign_response`, resident keys) is released by OpenSSH with `free()`: allocate it with `libc::calloc`/`malloc`, never Rust's allocator.
- SK API version is `0x000a0000` (OpenSSH ≥ 9.1). If OpenSSH bumps it, update `SSH_SK_VERSION_MAJOR` and the structs together.
- Windows binaries cannot run in the build sandbox: the exe build has `doCheck = false`; put testable logic in code that also compiles for Linux (`convert.rs`, `wfb-protocol`).
- `windows-sys` links DLLs via raw-dylib, so the exe build needs mingw's `dlltool` on `PATH` (`nativeBuildInputs` in `package.nix`; the devshell has it).
- Flakes see only tracked files: `git add` new files before any `nix` command.
- `windowsfidobridge.exe --list-serials` (used by gfh) is an addition beyond upstream (research R14): YubiKey serials on stdout, one decimal per line, exit 0 even when none; it must not read stdin or show UI. Keep that output contract.
- Work on features in `.worktrees/<branch>` (`just worktree <branch>`), not on `main` directly.
