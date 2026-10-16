# Golden fixtures

Whitespace-separated hex; decoders ignore whitespace. Every file comes from upstream behaviour
(the C++ at the commit pinned in the `justfile`):

| File | Bytes | Origin |
| --- | --- | --- |
| `sign-request.hex` | 256 | CBOR payload Linux → Windows, captured from the upstream debug log (2026-10-16, `ed25519-sk` key `ssh:5c-github`, message `hola\n`, namespace `git`; the signature verified with `ssh-keygen -Y check-novalidate`) |
| `sign-response.hex` | 125 | CBOR payload Windows → Linux from the same run |
| `sign-authdata.hex` | 37 | WebAuthn authenticator data of that signature: `SHA-256("ssh:5c-github") ‖ flags 0x01 ‖ counter 689`. Upstream does not log it, so it was rebuilt from the captured values and checked: `openssl pkeyutl -verify -rawin` with the key's public key accepts `sig_r` over `authdata ‖ SHA-256(data)`, so these are exactly the bytes the key signed |
| `enroll-response.hex` | 103 | `{"response_parameters": {…EnrollResult…}, "return_code": 0}` produced by upstream's own CBOR encoder: `upstream-encoder.cpp` builds the map exactly like `win32_middleware_exe/src/main.cpp` |

No frame header (`<len>|`) in any file.

## Regenerating `enroll-response.hex`

With `nix shell nixpkgs#cmake nixpkgs#gcc nixpkgs#gnumake nixpkgs#git`:

```sh
cmake -S .scratch/windows-fido-bridge-cpp -B /tmp/wfb-cpp -DSK_API_VERSION=10 \
  -DBUILD_WINDOWS_MIDDLEWARE_EXE=OFF -DBUILD_TESTS=OFF -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build /tmp/wfb-cpp --target common
U=.scratch/windows-fido-bridge-cpp/src/common B=/tmp/wfb-cpp
g++ -std=c++20 -I$U/include -I$U/3p-include -I$B/_deps/fmt-src/include \
  -I$B/_deps/spdlog-src/include crates/protocol/tests/fixtures/upstream-encoder.cpp \
  $B/src/common/libcommon.a $B/_deps/fmt-build/libfmt.a $B/_deps/spdlog-build/libspdlog.a \
  -o /tmp/upstream-encoder
/tmp/upstream-encoder   # hex, upstream's dump_debug line, then {"return_code": -2} in hex
```
