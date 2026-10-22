# Implementation Plan: age-plugin-yubikey through Windows from WSL

**Branch**: `002-age-yubikey-bridge` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

## Summary

Cross-compile the upstream `age-plugin-yubikey` (str4d, tag `v0.5.1`) to
`x86_64-pc-windows-gnu` with the crane and mingw-w64 setup that already builds
`windowsfidobridge.exe`, and wrap it in a one-line Linux shim of the same name that `exec`s the
`.exe` through WSL interop. A new package `age-plugin-yubikey-wsl` holds both
(`bin/age-plugin-yubikey`, `lib/age-plugin-yubikey.exe`); both modules gain
`programs.windows-fido-bridge.age.{enable,package}`.

No Rust is added to this workspace: the plugin already speaks the age plugin protocol over
stdin/stdout and reaches the YubiKey with WinSCard on Windows; WSL interop already relays a
Windows process's standard streams and exit status. The only glue is the shim.

## Technical Context

**Language/Version**: Nix; POSIX shell (bash) for the shim; upstream Rust (edition 2021,
MSRV 1.67) built with this repo's pinned 1.99.1 toolchain

**Primary Dependencies**: upstream `Cargo.lock` as published (`yubikey` 0.8.0-pre.0, `pcsc` 2.8,
`pcsc-sys` linking `winscard`, `sysinfo` 0.29, `age-plugin` 0.5); mingw-w64 import libraries

**Testing**: `checks.age-shim` runs the shim against a stand-in executable in the sandbox (argv,
stdin, stdout, stderr, exit status, `WSLENV`); `checks.nixos-module` evaluates the option;
`just try-age` on hardware

**Target Platform**: WSL 2 (shim) + Windows 10/11 x86_64 (plugin)

## Research

**R1. From source, not the release asset.** The upstream crate builds unchanged for
`x86_64-pc-windows-gnu`: its native dependency is `pcsc-sys`, which on Windows only emits
`rustc-link-lib=winscard` (mingw ships `libwinscard.a`); `sysinfo` and `i18n-embed` use
`winapi`/`windows-sys`. The lockfile pins `time` 0.3.36, which compiles on current Rust. So the
fallback (`fetchurl` of the signed MSVC release zip) is not needed.

**R2. Nothing in the decrypt path needs a Windows console** (read in `src/plugin.rs`,
`src/key.rs` at `v0.5.1`):

- `--age-plugin=identity-v1` runs `age_plugin::run_state_machine`, which reads and writes the
  protocol on stdin/stdout only.
- The PIN is requested with `callbacks.request_secret` (`Connection::request_pin_if_necessary`);
  the age client prompts on the WSL terminal and returns it over stdin.
- A missing YubiKey is reported with `callbacks.confirm` (falling back to `callbacks.message`
  and a 15 s poll).
- There is no touch message in plugin mode: `unwrap_file_key` blocks in the PIV `DECRYPT`
  until the key is touched (it blinks). Clients print nothing meanwhile. This is upstream
  behaviour, not a bridge limitation.
- `dialoguer` (console prompts) is only used by the CLI flows (`--generate` without flags and the
  default-PIN change in `manage`).
- Diagnostics go to stderr, which interop relays.

**R3. Shim shape.** `exec "<out>/lib/age-plugin-yubikey.exe" "$@"` replaces the shell with the
interop launcher, which relays the three streams and returns the Windows exit code, exactly as
the middleware's `Command::new(exe)` relies on. A Rust shim would add nothing. The `.exe` path
is substituted at build time and points into the same output, mirroring the
"executable next to the middleware" contract. `RUST_LOG` is appended to `WSLENV` so
`RUST_LOG=debug` reaches the plugin's `env_logger`; nothing else needs forwarding.

**R4. PATH precedence.** age clients have no plugin search path besides `PATH`
(`exec.LookPath("age-plugin-yubikey")` in filippo.io/age, used by sops). A Linux
`age-plugin-yubikey` earlier on `PATH` silently wins and fails with no reader. The modules add
the package with `lib.hiPrio` so it wins collisions in the system/home profile; shells that
prepend their own (e.g. `nix-shell -p age-plugin-yubikey`) must drop it on WSL.

**R5. PIN cache across processes.** Each age invocation spawns a new plugin process. The plugin
disconnects with `SCARD_LEAVE_CARD` (`disconnect_without_reset`), so a `once` PIN policy should
survive between invocations, but whether WinSCard / the YubiKey minidriver resets the card
between processes is to be observed with `just try-age` run twice.

## Constitution Check

| Principle | Gate | Result |
| --- | --- | --- |
| I. Upstream parity | No change to the FIDO bridge or its wire format | ✅ the plugin is a separate executable; the middleware is untouched |
| II. Unsafe at FFI only | No new Rust in this workspace | ✅ |
| III. Simplicity | Smallest glue | ✅ one shell line plus a crane derivation; upstream plugin unmodified. Feature is beyond upstream, added explicitly by this spec |
| IV. Tested core | Testable logic tested on Linux | ✅ the shim's passthrough is tested in the sandbox with a stand-in `.exe`; the `.exe` itself has `doCheck = false` |
| V. Nix build of record | Built by Nix, pinned | ✅ tag + hash pinned, upstream `Cargo.lock` with `--locked`, same toolchain |

## Project Structure

```text
nix/lib/default.nix                       windowsCrossArgs: shared Windows crane args
nix/packages/age-plugin-yubikey-wsl/      package.nix (exe + shim), shim.sh, default.nix
nix/modules/{nixos,home}/windows-fido-bridge.nix   age.enable / age.package
nix/checks/age-shim.nix                   shim passthrough test
nix/checks/nixos-module.nix               asserts the shim is in systemPackages with high priority
justfile                                  try-age
```

## Complexity Tracking

None.
