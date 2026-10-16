# AGENTS.md

Rust rewrite of windows-fido-bridge: an OpenSSH security key middleware that lets OpenSSH in WSL sign with a FIDO key attached to Windows, via the Windows WebAuthn API.

## Layout

| Path | What |
| --- | --- |
| `crates/sk-middleware/` | `cdylib` → `libwindowsfidobridge.so`; C ABI from OpenSSH `sk-api.h` (API v10) |
| `crates/webauthn-bridge/` | bin → `windowsfidobridge.exe` (x86_64-pc-windows-gnu); calls `webauthn.dll` |
| `nix/` | blueprint prefix: `devshell.nix`, `formatter.nix`, `lib/` (crane + toolchain helpers), `packages/`, `checks/` |
| `rust-toolchain.toml` | single source of truth for the Rust toolchain (devshell and Nix builds) |
| `specs/`, `.specify/` | Spec Kit specs, plans and tasks |
| `.scratch/windows-fido-bridge-cpp/` | C++ reference at the pinned commit (`just upstream`), gitignored |
| `.worktrees/` | feature-branch worktrees (`just worktree <branch>`), gitignored |

## Active work

Feature `001-rust-rewrite`: spec, plan, contracts and dependency-ordered tasks in `specs/001-rust-rewrite/` (start with `tasks.md`; principles in `.specify/memory/constitution.md`). Implement with `/speckit-implement` from a worktree: `just worktree 001-rust-rewrite`.

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
- Windows binaries cannot run in the build sandbox: the exe build has `doCheck = false`; put testable logic in code that also compiles for Linux.
- Flakes see only tracked files: `git add` new files before any `nix` command.
- Work on features in `.worktrees/<branch>` (`just worktree <branch>`), not on `main` directly.
