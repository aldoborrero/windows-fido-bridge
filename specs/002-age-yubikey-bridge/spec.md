# Feature Specification: age-plugin-yubikey through Windows from WSL

**Feature Branch**: `002-age-yubikey-bridge`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Make age's YubiKey plugin (PIV, `age-plugin-yubikey`) work from WSL
while the YubiKey stays attached to Windows: build the plugin for Windows from source, ship a
Linux shim named `age-plugin-yubikey` that runs it through WSL interop, and put it on PATH from
the NixOS and home-manager modules."

## Context

The developer's YubiKeys stay attached to Windows (feature 001 bridges FIDO for OpenSSH). The
same keys are age recipients through `age-plugin-yubikey` (PIV slot, `age1yubikey1…`), and sops
files in the homelab are encrypted to them. In WSL there is no smart card reader, so the Linux
plugin finds no YubiKey and decryption fails with `Could not open YubiKey with serial …`.
Windows runs the Smart Card service (`SCardSvr`), which exposes the PIV applet through
WinSCard, the API the plugin's `pcsc` dependency uses on Windows.

age clients (age, rage, sops) find a plugin by running the binary `age-plugin-<name>` from `PATH`
and speak the age plugin protocol with it over stdin/stdout. A WSL binary of that name that runs
the Windows build of the plugin, with its standard streams and exit status passed through, is
therefore indistinguishable from a local plugin.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Decrypt with a YubiKey attached to Windows (Priority: P1)

The developer runs `sops -d` (or `age -d -i stubs.txt`) in WSL on a file encrypted to their
YubiKey's `age1yubikey1…` recipient. The client asks for the PIV PIN in the WSL terminal when the
key's PIN policy requires it, the YubiKey blinks for a touch when the touch policy requires it,
and the plaintext is printed.

**Why this priority**: it is the whole feature.

**Independent Test**: `just try-age` with the key attached to Windows decrypts a file encrypted
to that key's recipient.

**Acceptance Scenarios**:

1. **Given** the shim on `PATH` and an identity stub file, **When** the developer decrypts a file
   encrypted to that YubiKey, **Then** the client prompts for the PIN (policy `once`/`always`),
   the key waits for a touch (policy `cached`/`always`), and the plaintext matches.
2. **Given** the YubiKey is not attached to Windows, **When** decrypting, **Then** the plugin
   asks the client to insert it (`confirm`/`message`) and fails cleanly when skipped or timed out.

### User Story 2 - Enable it declaratively (Priority: P2)

On a NixOS-WSL host or with home-manager, setting `programs.windows-fido-bridge.age.enable` puts
the shim on `PATH`, ahead of any Linux `age-plugin-yubikey`.

**Independent Test**: `nix flake check` evaluates the NixOS module with the option set.

### Edge Cases

- A Linux `age-plugin-yubikey` earlier on `PATH` (e.g. a `nix-shell -p age-plugin-yubikey`)
  shadows the shim; age has no other plugin lookup, so the shim must come first.
- The plugin's CLI modes (`--list`, `--identity`, `--generate`) also run through the shim.
  `--list`/`--identity` print to stdout and need no input. `--generate` without flags prompts
  with `dialoguer`, which needs a console: run from a WSL terminal, interop gives the Windows
  process a pseudo console, so it is expected to work but is untested; the flag form
  (`--generate --serial … --slot …`) avoids the prompts.

## Requirements *(mandatory)*

- **FR-001**: The flake MUST build `age-plugin-yubikey` from source, at a pinned release tag, for
  `x86_64-pc-windows-gnu`, with the repository's toolchain and cross setup.
- **FR-002**: The flake MUST provide a Linux executable named `age-plugin-yubikey` that runs that
  Windows executable through WSL interop, passing argv, stdin, stdout and stderr unchanged and
  returning its exit status.
- **FR-003**: The executable MUST live in the same Nix output as the shim.
- **FR-004**: The shim MUST forward `RUST_LOG` (the plugin's log filter) to Windows via `WSLENV`.
- **FR-005**: The NixOS and home-manager modules MUST offer `programs.windows-fido-bridge.age.enable`
  (and `age.package`), independent of `enable`, that put the shim on `PATH` with high priority.
- **FR-006**: PIN entry and touch/insert prompts MUST go through the age plugin protocol
  (`request-secret`, `confirm`, `msg`) so the WSL client handles them; nothing in the decrypt
  path may need a Windows console.

## Success Criteria *(mandatory)*

- **SC-001**: `just try-age` decrypts on the developer's machine with the key attached to Windows.
- **SC-002**: `nix flake check` passes, including a sandbox test of the shim's passthrough and
  the module evaluation.
- **SC-003**: homelab `sops -d` works from WSL with `SOPS_AGE_KEY_FILE` pointing at the YubiKey
  identity stubs, once the shim comes first on `PATH`.

## Assumptions

- WSL interop (`binfmt_misc` `WSLInterop`) is enabled, as it is for the FIDO bridge.
- The Windows Smart Card service is running and no other process holds the PIV applet
  exclusively.
