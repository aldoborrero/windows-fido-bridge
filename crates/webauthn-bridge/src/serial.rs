//! YubiKey serial numbers over CCID for `--list-serials` (an extension beyond upstream, used by
//! gfh). APDUs and response formats follow yubikit (yubikey-manager): `management.py`
//! `read_device_info` and `yubiotp.py` `get_serial`. Platform-independent so it is unit-tested on
//! Linux; `pcsc.rs` carries the APDUs on Windows.
#![deny(unsafe_code)]
// Outside Windows only the tests call into this module.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

/// SELECT the Management application (AID `A0 00 00 05 27 47 11 17`).
pub const SELECT_MANAGEMENT: &[u8] = &[
    0x00, 0xA4, 0x04, 0x00, 0x08, 0xA0, 0x00, 0x00, 0x05, 0x27, 0x47, 0x11, 0x17,
];
/// Management READ CONFIG page 0 (`INS_READ_CONFIG` = 0x1D); YubiKey ≥ 4.1.
pub const READ_CONFIG: &[u8] = &[0x00, 0x1D, 0x00, 0x00, 0x00];
/// SELECT the OTP application (AID `A0 00 00 05 27 20 01`).
pub const SELECT_OTP: &[u8] = &[
    0x00, 0xA4, 0x04, 0x00, 0x07, 0xA0, 0x00, 0x00, 0x05, 0x27, 0x20, 0x01,
];
/// OTP `INS_CONFIG` with slot `DEVICE_SERIAL` (0x10): 4-byte big-endian serial (YK4 and older).
pub const OTP_GET_SERIAL: &[u8] = &[0x00, 0x01, 0x10, 0x00, 0x00];
/// GET RESPONSE after `61xx` (yubikit `INS_SEND_REMAINING`).
const GET_RESPONSE: &[u8] = &[0x00, 0xC0, 0x00, 0x00, 0x00];

/// DeviceInfo TLV tag holding the serial.
const TAG_SERIAL: u8 = 0x02;

/// PC/SC reader names of YubiKeys look like `Yubico YubiKey OTP+FIDO+CCID 0`.
pub fn is_yubikey_reader(name: &str) -> bool {
    name.to_ascii_lowercase().contains("yubikey")
}

/// Splits a `REG_MULTI_SZ`-style list (`a\0b\0\0`) as returned by `SCardListReadersW`.
pub fn split_multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Reads the serial through `transmit` (one APDU in, raw response with SW out): Management
/// READ CONFIG first, then the OTP application. `None` when the key does not expose one.
pub fn read_serial(
    mut transmit: impl FnMut(&[u8]) -> io::Result<Vec<u8>>,
) -> io::Result<Option<u32>> {
    let mut command = |apdu: &[u8]| -> io::Result<Option<Vec<u8>>> {
        let mut data = Vec::new();
        let mut response = transmit(apdu)?;
        loop {
            let Some((chunk, &sw)) = response.split_last_chunk::<2>() else {
                return Ok(None);
            };
            data.extend_from_slice(chunk);
            match sw {
                [0x90, 0x00] => return Ok(Some(data)),
                [0x61, _] => response = transmit(GET_RESPONSE)?,
                _ => return Ok(None),
            }
        }
    };
    if command(SELECT_MANAGEMENT)?.is_some()
        && let Some(serial) = command(READ_CONFIG)?.and_then(|d| device_info_serial(&d))
    {
        return Ok(Some(serial));
    }
    if command(SELECT_OTP)?.is_none() {
        return Ok(None);
    }
    Ok(command(OTP_GET_SERIAL)?.and_then(|d| nonzero(&d)))
}

/// DeviceInfo page: a length byte, then TLVs with one-byte tags (yubikit `DeviceInfo.parse`).
fn device_info_serial(data: &[u8]) -> Option<u32> {
    let (&len, mut tlvs) = data.split_first()?;
    if usize::from(len) != tlvs.len() {
        return None;
    }
    while let [tag, len, rest @ ..] = tlvs {
        // BER long-form lengths (0x81 nn) never occur in a page that fits in 255 bytes.
        let (value, next) = rest.split_at_checked(usize::from(*len))?;
        if *tag == TAG_SERIAL {
            return nonzero(value);
        }
        tlvs = next;
    }
    None
}

/// Big-endian integer of up to 4 bytes; 0 means "no serial" in yubikit.
fn nonzero(value: &[u8]) -> Option<u32> {
    if value.is_empty() || value.len() > 4 {
        return None;
    }
    let serial = value.iter().fold(0u32, |n, &b| n << 8 | u32::from(b));
    (serial != 0).then_some(serial)
}

/// Drops repeated serials, keeping the first occurrence (reader order).
pub fn dedup(serials: &mut Vec<u32>) {
    let mut seen = Vec::new();
    serials.retain(|s| {
        let new = !seen.contains(s);
        seen.push(*s);
        new
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: [u8; 2] = [0x90, 0x00];

    /// Answers each expected APDU in order with the given response; panics on anything else.
    fn card(script: &[(&[u8], &[u8])]) -> impl FnMut(&[u8]) -> io::Result<Vec<u8>> + use<> {
        let mut script = script
            .iter()
            .map(|(a, r)| (a.to_vec(), r.to_vec()))
            .collect::<Vec<_>>()
            .into_iter();
        move |apdu| {
            let (expected, response) = script.next().expect("unexpected extra APDU");
            assert_eq!(apdu, expected);
            Ok(response)
        }
    }

    fn with_sw(data: &[u8], sw: [u8; 2]) -> Vec<u8> {
        [data, &sw].concat()
    }

    // YubiKey 5 DeviceInfo page 0 shaped as in yubikit: usb supported (0x01), serial (0x02)
    // 12345678 = 0x00BC614E, form factor (0x04), firmware (0x05) 5.4.3.
    const DEVICE_INFO: &[u8] = &[
        0x12, 0x01, 0x02, 0x02, 0x3B, 0x02, 0x04, 0x00, 0xBC, 0x61, 0x4E, 0x04, 0x01, 0x01, 0x05,
        0x03, 0x05, 0x04, 0x03,
    ];

    #[test]
    fn read_serial_uses_management_read_config() {
        let version = with_sw(b"5.4.3", OK);
        let info = with_sw(DEVICE_INFO, OK);
        let tx = card(&[(SELECT_MANAGEMENT, &version), (READ_CONFIG, &info)]);
        assert_eq!(read_serial(tx).unwrap(), Some(12_345_678));
    }

    #[test]
    fn read_serial_falls_back_to_otp_when_read_config_unsupported() {
        let version = with_sw(b"4.3.7", OK);
        let otp_status = with_sw(&[4, 3, 7, 1, 6, 0], OK);
        let serial = with_sw(&[0x00, 0x5B, 0x8D, 0x80], OK); // 6000000
        let tx = card(&[
            (SELECT_MANAGEMENT, &version),
            (READ_CONFIG, &[0x6D, 0x00]),
            (SELECT_OTP, &otp_status),
            (OTP_GET_SERIAL, &serial),
        ]);
        assert_eq!(read_serial(tx).unwrap(), Some(6_000_000));
    }

    #[test]
    fn read_serial_falls_back_to_otp_without_management() {
        let serial = with_sw(&[0x00, 0x00, 0x30, 0x39], OK);
        let tx = card(&[
            (SELECT_MANAGEMENT, &[0x6A, 0x82]),
            (SELECT_OTP, &OK),
            (OTP_GET_SERIAL, &serial),
        ]);
        assert_eq!(read_serial(tx).unwrap(), Some(12345));
    }

    #[test]
    fn read_serial_follows_61xx_response_chaining() {
        let (head, tail) = DEVICE_INFO.split_at(7);
        let first = with_sw(head, [0x61, tail.len() as u8]);
        let rest = with_sw(tail, OK);
        let tx = card(&[
            (SELECT_MANAGEMENT, &OK),
            (READ_CONFIG, &first),
            (GET_RESPONSE, &rest),
        ]);
        assert_eq!(read_serial(tx).unwrap(), Some(12_345_678));
    }

    #[test]
    fn read_serial_is_none_when_no_application_answers() {
        let tx = card(&[
            (SELECT_MANAGEMENT, &[0x6A, 0x82]),
            (SELECT_OTP, &[0x6A, 0x82]),
        ]);
        assert_eq!(read_serial(tx).unwrap(), None);
    }

    #[test]
    fn read_serial_is_none_for_hidden_serial() {
        // Serial-visibility off: OTP answers 9000 with no data.
        let tx = card(&[
            (SELECT_MANAGEMENT, &[0x6A, 0x82]),
            (SELECT_OTP, &OK),
            (OTP_GET_SERIAL, &OK),
        ]);
        assert_eq!(read_serial(tx).unwrap(), None);
    }

    #[test]
    fn device_info_without_serial_tag_is_none() {
        assert_eq!(device_info_serial(&[0x03, 0x01, 0x01, 0x3B]), None);
    }

    #[test]
    fn device_info_with_bad_length_byte_is_none() {
        assert_eq!(
            device_info_serial(&DEVICE_INFO[..DEVICE_INFO.len() - 1]),
            None
        );
    }

    #[test]
    fn device_info_with_truncated_tlv_is_none() {
        assert_eq!(device_info_serial(&[0x03, 0x02, 0x04, 0x00]), None);
    }

    #[test]
    fn zero_serial_is_none() {
        assert_eq!(nonzero(&[0, 0, 0, 0]), None);
    }

    #[test]
    fn yubikey_readers_match_case_insensitively() {
        assert!(is_yubikey_reader("Yubico YubiKey OTP+FIDO+CCID 0"));
        assert!(is_yubikey_reader("yubico yubikey ccid"));
        assert!(!is_yubikey_reader("Microsoft Virtual Smart Card 0"));
    }

    #[test]
    fn split_multi_sz_returns_each_reader() {
        let buf: Vec<u16> = "Yubico YubiKey CCID 0\0Other Reader 1\0\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            split_multi_sz(&buf),
            ["Yubico YubiKey CCID 0", "Other Reader 1"]
        );
    }

    #[test]
    fn dedup_keeps_first_occurrence_in_order() {
        let mut serials = vec![3, 1, 3, 2, 1];
        dedup(&mut serials);
        assert_eq!(serials, [3, 1, 2]);
    }
}
