# Contract: OpenSSH security key middleware C ABI (API v10)

Source of truth: OpenSSH `sk-api.h` (identical to upstream `common/3p-include/sk-api-v10.h`
modulo a `HAVE_STDINT_H` guard). Exported by `libwindowsfidobridge.so` with C linkage.

## Constants

| Name | Value |
| --- | --- |
| `SSH_SK_VERSION_MAJOR` | `0x000a0000` |
| `SSH_SK_USER_PRESENCE_REQD` | `0x01` |
| `SSH_SK_USER_VERIFICATION_REQD` | `0x04` |
| `SSH_SK_FORCE_OPERATION` | `0x10` |
| `SSH_SK_RESIDENT_KEY` | `0x20` |
| `SSH_SK_ECDSA` / `SSH_SK_ED25519` | `0x00` / `0x01` |
| `SSH_SK_ERR_GENERAL` … `CREDENTIAL_EXISTS` | `-1` … `-5` |

## Structs (`#[repr(C)]`, field order exactly as below)

```c
struct sk_enroll_response {
  uint8_t flags;
  uint8_t *public_key;       size_t public_key_len;
  uint8_t *key_handle;       size_t key_handle_len;
  uint8_t *signature;        size_t signature_len;
  uint8_t *attestation_cert; size_t attestation_cert_len;
  uint8_t *authdata;         size_t authdata_len;
};
struct sk_sign_response {
  uint8_t flags; uint32_t counter;
  uint8_t *sig_r; size_t sig_r_len;
  uint8_t *sig_s; size_t sig_s_len;
};
struct sk_option { char *name; char *value; uint8_t required; };
```

## Functions

| Function | Behaviour |
| --- | --- |
| `uint32_t sk_api_version(void)` | returns `0x000a0000` |
| `int sk_enroll(uint32_t alg, const uint8_t *challenge, size_t challenge_len, const char *application, uint8_t flags, const char *pin, struct sk_option **options, struct sk_enroll_response **enroll_response)` | forwards an `sk_enroll` request; on success writes a `calloc`ed response to `*enroll_response` and returns 0; otherwise returns -1 and leaves `*enroll_response` untouched |
| `int sk_sign(uint32_t alg, const uint8_t *data, size_t data_len, const char *application, const uint8_t *key_handle, size_t key_handle_len, uint8_t flags, const char *pin, struct sk_option **options, struct sk_sign_response **sign_response)` | forwards an `sk_sign` request; same success/failure contract |
| `int sk_load_resident_keys(const char *pin, struct sk_option **options, struct sk_resident_key ***rks, size_t *nrks)` | returns `SSH_SK_ERR_UNSUPPORTED` (-2) |

## Input handling

- `application` and `pin` may be NULL → treated as empty strings. `pin` is never forwarded or
  logged (only "present"/"not present").
- `options` may be NULL; otherwise a NULL-terminated array of `sk_option*`.
- `challenge`/`data`/`key_handle` with length 0 may have a NULL pointer.

## Guarantees

- No panic or unwind crosses the boundary; failures → -1.
- All memory reachable from a returned response is `calloc`/`malloc` memory, freeable by
  OpenSSH's `free()` field by field and then the struct.
- No memory is leaked on any failure path.
