# AGENTS.md

Rust rewrite of windows-fido-bridge: an OpenSSH security key middleware that lets OpenSSH in WSL sign with a FIDO key attached to Windows, via the Windows WebAuthn API.

## Layout

| Path | What |
| --- | --- |
| `crates/protocol/` | lib `wfb-protocol`: `sk-api.h` constants, CBOR messages (`message.rs`), length-prefixed framing (`frame.rs`), stderr logger; golden fixtures (only copy, provenance in its README) in `tests/fixtures/` |
| `crates/sk-middleware/` | `cdylib` → `libwindowsfidobridge.so`; C ABI from OpenSSH `sk-api.h` (API v10): `lib.rs` entry points, `ffi.rs` structs + calloc, `bridge.rs` spawns the exe |
| `crates/webauthn-bridge/` | bin → `windowsfidobridge.exe` (x86_64-pc-windows-gnu); `convert.rs` (WebAuthn → OpenSSH, tested on Linux), `serial.rs` (`--list-serials` APDUs and parsing, tested on Linux), `webauthn.rs` + `window.rs` + `pcsc.rs` (Windows only) |
| `nix/` | blueprint prefix: `devshell.nix`, `formatter.nix`, `modules/{nixos,home}/windows-fido-bridge.nix` (set `SSH_SK_PROVIDER` + ssh-agent `-P`; `age.enable` puts the age plugin shim on `PATH`), `lib/` (toolchain, crane args incl. `windowsCrossArgs`, and the shared Linux/Windows `-deps` builds), `packages/` (`windows-fido-bridge`; `age-plugin-yubikey-wsl` = upstream plugin built for Windows + `shim.sh`), `checks/` (`clippy.nix`, `clippy-windows.nix` = Windows-target clippy, `tests.nix` = `cargo test --workspace`, the only Rust test run, `age-shim.nix` = shim passthrough against a stand-in `.exe`, `nixos-module.nix` = module eval check) |
| `rust-toolchain.toml` | single source of truth for the Rust toolchain, pinned to an exact release (devshell and Nix builds) |
| `specs/`, `.specify/` | Spec Kit specs, plans and tasks |
| `.scratch/windows-fido-bridge-cpp/` | C++ reference at the pinned commit (`just upstream`), gitignored |
| `.worktrees/` | feature-branch worktrees (`just worktree <branch>`), gitignored |

## Active work

Feature `002-age-yubikey-bridge`: `age-plugin-yubikey` through Windows (`specs/002-age-yubikey-bridge/`, branch `002-age-yubikey-bridge`); hardware validation (`just try-age`, T007) remains.

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
| end-to-end age decrypt test | `just try-age [--serial N]` (key attached to Windows, PIN/touch per PIV policy) |

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
- `age-plugin-yubikey-wsl` builds upstream `age-plugin-yubikey` unmodified from a pinned tag (`--locked`, its own `Cargo.lock`); bump `version` and `hash` together in `nix/packages/age-plugin-yubikey-wsl/package.nix`. Its `bin/age-plugin-yubikey` shim `exec`s `lib/age-plugin-yubikey.exe` from the same output: argv, stdin/stdout/stderr and exit status pass through untouched (age's plugin protocol runs over them), only `RUST_LOG` is added to `WSLENV`. `checks.age-shim` guards that contract.
- age finds plugins only by name on `PATH`, so the modules install the shim with `lib.hiPrio`; a Linux `age-plugin-yubikey` earlier on `PATH` shadows it.
