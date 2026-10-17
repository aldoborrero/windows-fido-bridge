//! Message framing: `<decimal length>|<payload>` (upstream `common/src/communication.cpp`).

use std::io::{self, Read, Write};

/// Largest payload a reader accepts.
pub const MAX_MESSAGE_SIZE: usize = 32768;

/// Digits in `MAX_MESSAGE_SIZE`; longer headers (leading zeros) are rejected so an endless
/// `000…` stream cannot keep the reader busy (upstream accepts any number of leading zeros).
const MAX_HEADER_DIGITS: usize = 5;

/// Writes one frame: the payload length in decimal, `|`, then the payload.
pub fn write(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    write!(w, "{}|", payload.len())?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads one frame and returns its payload.
pub fn read(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut size = 0usize;
    let mut digits = 0;
    loop {
        let mut byte = [0u8];
        r.read_exact(&mut byte)?;
        match byte[0] {
            b'|' => break,
            digit @ b'0'..=b'9' => {
                digits += 1;
                size = size * 10 + usize::from(digit - b'0');
                if size > MAX_MESSAGE_SIZE || digits > MAX_HEADER_DIGITS {
                    return Err(invalid("message too large"));
                }
            }
            _ => return Err(invalid("malformed message header")),
        }
    }
    let mut payload = vec![0; size];
    r.read_exact(&mut payload)?;
    Ok(payload)
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn read_err(bytes: &[u8]) -> io::Error {
        read(&mut &bytes[..]).unwrap_err()
    }

    #[test]
    fn round_trips() {
        let mut buf = Vec::new();
        write(&mut buf, b"\xa1\x01\x02").unwrap();
        assert_eq!(buf, b"3|\xa1\x01\x02");
        assert_eq!(read(&mut buf.as_slice()).unwrap(), b"\xa1\x01\x02");
    }

    #[test]
    fn empty_payload() {
        assert_eq!(read(&mut &b"0|"[..]).unwrap(), b"");
    }

    #[test]
    fn rejects_non_digit_header() {
        let err = read_err(b"1a|x");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "malformed message header");
    }

    #[test]
    fn rejects_oversized_message() {
        let err = read_err(b"32769|");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "message too large");
    }

    #[test]
    fn accepts_max_size() {
        let mut buf = b"32768|".to_vec();
        buf.resize(buf.len() + MAX_MESSAGE_SIZE, 7);
        assert_eq!(read(&mut buf.as_slice()).unwrap().len(), MAX_MESSAGE_SIZE);
    }

    #[test]
    fn header_digits_are_capped() {
        assert_eq!(read(&mut &b"00005|hello"[..]).unwrap(), b"hello");
        assert_eq!(read_err(b"000005|hello").to_string(), "message too large");
        // Without the cap this never returns.
        let err = read(&mut io::repeat(b'0')).unwrap_err();
        assert_eq!(err.to_string(), "message too large");
    }

    #[test]
    fn eof_before_separator() {
        assert_eq!(read_err(b"12").kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn short_payload() {
        assert_eq!(read_err(b"5|abc").kind(), io::ErrorKind::UnexpectedEof);
    }

    /// Tiny deterministic xorshift64 generator for the no-panic loops below.
    pub(crate) fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn random_input_never_panics() {
        let mut state = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..10_000 {
            let len = (xorshift(&mut state) % 16) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|_| match xorshift(&mut state) % 4 {
                    0 => b'|',
                    1 | 2 => b'0' + (xorshift(&mut state) % 10) as u8,
                    _ => xorshift(&mut state) as u8,
                })
                .collect();
            if let Ok(payload) = read(&mut bytes.as_slice()) {
                assert!(payload.len() <= MAX_MESSAGE_SIZE);
            }
        }
    }
}
