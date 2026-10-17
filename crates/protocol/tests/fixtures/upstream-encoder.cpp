// Encodes replies exactly as upstream win32_middleware_exe/src/main.cpp builds them.
#include <windows_fido_bridge/cbor.hpp>
#include <cstdio>
#include <span>
#include <vector>
using namespace wfb;
static void hex(const byte_vector& v) {
    for (size_t i = 0; i < v.size(); i++) printf("%02x%s", v[i], (i % 16 == 15) ? "\n" : " ");
    printf("\n");
}
int main() {
    std::vector<uint8_t> cert{0xce, 0xcf}, authdata{}, key_handle(3, 0x09), public_key(2, 0x22), sig{0x55};
    int return_code = 0;
    auto response_message = cbor_map({{"return_code", return_code}});
    response_message["response_parameters"] = cbor_map({
        {"public_key", std::span<const uint8_t>(public_key.data(), public_key.size())},
        {"key_handle", std::span<const uint8_t>(key_handle.data(), key_handle.size())},
        {"signature", std::span<const uint8_t>(sig.data(), sig.size())},
        {"attestation_cert", std::span<const uint8_t>(cert.data(), cert.size())},
        {"authdata", std::span<const uint8_t>(authdata.data(), authdata.size())},
    });
    hex(dump_cbor(response_message));
    printf("%s\n", response_message.dump_debug().c_str());
    auto err = cbor_map({{"return_code", -2}});
    hex(dump_cbor(err));
}
