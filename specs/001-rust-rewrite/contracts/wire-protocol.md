# Contract: Linux ↔ Windows wire protocol

Must stay byte-compatible with upstream (`common/src/communication.cpp`,
`linux_middleware/src/dll_entry_points.cpp`, `win32_middleware_exe/src/main.cpp`) so the Rust
library works with the C++ `.exe` and vice versa.

## Transport

- Linux library spawns `<dir of libwindowsfidobridge.so>/windowsfidobridge.exe` (no arguments)
  through WSL interop.
- Environment: `WSLENV` = previous value (if any) + `:`-joined
  `WINDOWS_FIDO_BRIDGE_DEBUG:WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION`.
- Exactly one request frame on the child's stdin, one response frame on its stdout. stderr is
  passed through (logs). The child MUST exit 0 after writing the response; non-zero exit →
  general error on the Linux side.

## Framing

```
frame   = length "|" payload
length  = 1*DIGIT            ; decimal byte count of payload, value ≤ 32768
payload = <CBOR map>
```

Reader: accumulate digits until `|`; any other byte → error; running value > 32768 or more than
5 digits → error;
then read exactly `length` bytes; EOF before that → error. Writer: decimal length without
padding, `|`, payload.

## Messages

CBOR major types: text = 3, bytes = 2, uint = 0, negative int = 1, array = 4, map = 5. No
booleans, floats or tags. Field names and types per [data-model.md](../data-model.md).

### Example: sign request (from a real run, 2026-10-16, key `ssh:5c-github`)

Diagnostic notation:

```
{"request_parameters": {"alg": 1, "application": "ssh:5c-github",
  "data": h'53534853494700000003676974...', "flags": 33,
  "key_handle": h'5737309566ac6bec...', "sk_options": []},
 "request_type": "sk_sign"}
```

First bytes: `a2 72 "request_parameters" a6 63 "alg" 01 6b "application" 6d "ssh:5c-github" …`.

### Example: sign response

```
{"response_parameters": {"counter": 689, "flags": 1, "sig_r": h'1a7b2111…4509'},
 "return_code": 0}
```

Bytes: `a2 73 "response_parameters" a3 67 "counter" 19 02 b1 65 "flags" 01 65 "sig_r" 58 40 …
6b "return_code" 00`.

These two messages are the golden fixtures `sign-request.hex` and `sign-response.hex` in
[`crates/protocol/tests/fixtures/`](../../../crates/protocol/tests/fixtures/) (with an upstream-encoded
enroll response and the authenticator data of this signature).

### Failure response

`{"return_code": -1}` or `{"return_code": -2}` (no `response_parameters`).
