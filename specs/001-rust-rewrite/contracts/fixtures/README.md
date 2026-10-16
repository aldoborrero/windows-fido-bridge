# Golden fixtures

Captured from the upstream C++ build (`WINDOWS_FIDO_BRIDGE_DEBUG=1 just try-sign`-equivalent run
on 2026-10-16) signing with the ed25519-sk key `ssh:5c-github`. The message was `hola\n`
namespace `git`; the signature verified with `ssh-keygen -Y check-novalidate`.

- `sign-request.hex` — CBOR payload Linux → Windows (256 bytes, no frame header).
- `sign-response.hex` — CBOR payload Windows → Linux (125 bytes, no frame header).

Whitespace-separated hex; decoders should ignore whitespace. Copy them into
`crates/protocol/tests/fixtures/` (or `include_str!` them) for encode/decode tests.
