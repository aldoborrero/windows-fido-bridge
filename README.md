# windows-fido-bridge

Use a FIDO security key plugged into Windows for OpenSSH inside WSL — `ssh`, `ssh-agent` and `git` commit signing with `ed25519-sk` / `ecdsa-sk` keys, without passing the USB device through to Linux.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/aldoborrero/windows-fido-bridge/ci.yml)](https://github.com/aldoborrero/windows-fido-bridge/actions)

> **Status:** Rust rewrite of the [upstream C++ implementation](https://github.com/socram8888/windows-fido-bridge) with the same behaviour: signing, enrollment and diagnostics. It speaks the same wire protocol, so either half can be swapped with its upstream counterpart.

## About

Passing a YubiKey into WSL with `usbipd` makes FIDO fragile: `pcscd` and libfido2 fight over the device, signatures time out, and the key disappears from Windows while it is attached. This project leaves the key on Windows and asks for each signature through the Windows WebAuthn API, so Windows shows its own "touch your security key" prompt.

It is an [OpenSSH security key middleware](https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.u2f) in two pieces:

| Piece | Runs on | Role |
| --- | --- | --- |
| `libwindowsfidobridge.so` | WSL (Linux) | Loaded by OpenSSH through `SSH_SK_PROVIDER`; implements `sk-api.h` |
| `windowsfidobridge.exe` | Windows | Started by the library through WSL interop; calls `webauthn.dll` |

Keys enrolled with libfido2 over USB/IP keep working — no re-enrollment needed.

## Requirements

- WSL 2 on Windows 10 1903+ or Windows 11
- OpenSSH ≥ 9.1 in WSL (security key API v10)
- A FIDO2/U2F key that supports Ed25519 or ECDSA

## Installation

With Nix:

```bash
nix build github:aldoborrero/windows-fido-bridge
```

Both files land in `result/lib/`. The library expects the `.exe` next to it.

### NixOS / home-manager

Add the flake as an input and enable the module. It sets `SSH_SK_PROVIDER` and allows the middleware in `ssh-agent` (`-P`):

```nix
# flake.nix
inputs.windows-fido-bridge.url = "github:aldoborrero/windows-fido-bridge";

# NixOS (e.g. a NixOS-WSL host; uses programs.ssh.startAgent)
imports = [ inputs.windows-fido-bridge.nixosModules.windows-fido-bridge ];
programs.windows-fido-bridge.enable = true;

# or home-manager (uses services.ssh-agent)
imports = [ inputs.windows-fido-bridge.homeModules.windows-fido-bridge ];
programs.windows-fido-bridge.enable = true;
```

The key stays attached to Windows; no `usbipd` passthrough is needed.

## Usage

```bash
export SSH_SK_PROVIDER=/path/to/lib/libwindowsfidobridge.so

ssh -i ~/.ssh/id_ed25519_sk user@host
ssh-keygen -Y sign -n git -f ~/.ssh/id_ed25519_sk message
```

With `ssh-agent`, allow the provider path explicitly (the default allow-list only covers `/usr/lib*`):

```bash
ssh-agent -P '/nix/store/*'
ssh-add -S "$SSH_SK_PROVIDER" ~/.ssh/id_ed25519_sk
```

Enroll a new key (Windows asks for a touch):

```bash
ssh-keygen -t ed25519-sk -w "$SSH_SK_PROVIDER" -f ~/.ssh/id_ed25519_sk   # or -t ecdsa-sk
ssh-keygen -t ed25519-sk -w "$SSH_SK_PROVIDER" -O user=alice              # user name shown by Windows
```

Environment variables (forwarded to the Windows side through `WSLENV`):

| Variable | Effect |
| --- | --- |
| `WINDOWS_FIDO_BRIDGE_DEBUG` | any value: debug logs from both sides on stderr (the PIN is never logged) |
| `WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION` | any value: always require PIN or biometric verification |

Using the application `ssh:windows-fido-bridge-verify-required` (`ssh-keygen -O application=...`) also requires user verification for that key. Resident keys (`ssh-keygen -K`) are not supported.

List the serial numbers of the YubiKeys attached to Windows (one decimal per line, nothing if none; read over PC/SC, no window or touch). [gfh](https://github.com/aldoborrero/gfh) runs this to pick a signing key under WSL:

```bash
"$(dirname "$SSH_SK_PROVIDER")/windowsfidobridge.exe" --list-serials
```

## Development

```bash
direnv allow        # or: nix develop
just                # list recipes
just test           # cargo test
just build          # nix build: .so + .exe into ./result/lib
just try-sign       # sign a test message through the bridge (key attached to Windows)
just upstream       # clone the C++ reference into .scratch/
just worktree feat  # new branch in .worktrees/feat
```

Rust is built with [crane](https://github.com/ipetkov/crane); the toolchain comes from `rust-toolchain.toml` via [rust-overlay](https://github.com/oxalica/rust-overlay), and the Windows executable is cross-compiled to `x86_64-pc-windows-gnu` with nixpkgs' mingw-w64. Specs and plans live under `specs/` ([Spec Kit](https://github.com/github/spec-kit)).

`nix flake check` runs everything CI runs: build, tests, clippy and formatting.

## Acknowledgments

Rewrite of [windows-fido-bridge](https://github.com/mgbowen/windows-fido-bridge) by Matthew Bowen and contributors, continued by [socram8888](https://github.com/socram8888/windows-fido-bridge). See [NOTICE](NOTICE).

## License

Apache License 2.0 — see [LICENSE](LICENSE).
