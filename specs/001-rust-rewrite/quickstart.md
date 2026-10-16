# Quickstart: validate the Rust rewrite

## Prerequisites

- WSL 2 (x86_64) with this repo and direnv/Nix; OpenSSH ≥ 9.1 in WSL.
- A YubiKey with an existing `ed25519-sk` credential, e.g. `~/.ssh/id_ed25519_sk_5c-github`.
- `usbipd-win` on Windows (only to make sure the key is **not** attached to WSL).

## 1. Automated checks (no hardware)

```bash
just check      # nix flake check: crane build, cargo test --workspace, clippy -D warnings, treefmt
just test       # fast loop: cargo test --workspace
```

Expected: all green. The protocol tests decode/encode the golden fixtures in
[contracts/fixtures/](contracts/fixtures/); the conversion tests cover authenticator data, COSE
keys and DER signatures (see [data-model.md](data-model.md)).

## 2. Build the deliverable

```bash
just build
ls result/lib   # libwindowsfidobridge.so  windowsfidobridge.exe
```

## 3. Hand the key to Windows

```bash
"/mnt/c/Program Files/usbipd-win/usbipd.exe" list            # find the YubiKey BUSID
"/mnt/c/Program Files/usbipd-win/usbipd.exe" detach --busid <BUSID>
```

## 4. Sign with an existing credential (User Story 1)

```bash
just try-sign ~/.ssh/id_ed25519_sk_5c-github
```

Touch the key when the Windows security dialog appears. Expected last line:

```
Good "git" signature with ED25519-SK key SHA256:2/nG8D8+uIQ3+yErEqAPfi/5R07GC6lFSdhp91zUCnY
```

With diagnostics (User Story 3):

```bash
WINDOWS_FIDO_BRIDGE_DEBUG=1 just try-sign
```

Expected: debug lines from `linux-middleware` and `win32-bridge` on stderr, including the sign
request/response and parsed authenticator data; the PIN is shown only as present/not present.

Cancel test: run `just try-sign` and press Cancel in the Windows dialog → `ssh-keygen` reports a
signing failure and returns promptly.

## 5. Enroll a new credential (User Story 2)

```bash
ssh-keygen -t ed25519-sk -w "$PWD/result/lib/libwindowsfidobridge.so" -f /tmp/wfb-test -N ''
just try-sign /tmp/wfb-test
ssh-keygen -t ecdsa-sk   -w "$PWD/result/lib/libwindowsfidobridge.so" -f /tmp/wfb-test-ecdsa -N ''
just try-sign /tmp/wfb-test-ecdsa
```

Expected: both keys are created after a touch and both signatures verify.

## 6. Interoperability with upstream (optional, parity)

If you have an upstream C++ build (sources via `just upstream`; built with CMake + mingw), put its `windowsfidobridge.exe`
next to the Rust `.so` (and vice versa) and repeat step 4. Expected: identical result.

## 7. Give the key back to WSL (optional)

```bash
"/mnt/c/Program Files/usbipd-win/usbipd.exe" attach --wsl --busid <BUSID>
```
