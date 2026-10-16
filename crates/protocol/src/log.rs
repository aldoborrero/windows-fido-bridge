//! Minimal stderr logger: debug when `WINDOWS_FIDO_BRIDGE_DEBUG` is set (any value), otherwise
//! warnings and errors only (upstream `common/src/util.cpp: set_up_logger`).

use std::fmt::{self, Write as _};
use std::io::Write as _;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static SIDE: OnceLock<(&'static str, bool)> = OnceLock::new();

/// Severity of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Written only when `WINDOWS_FIDO_BRIDGE_DEBUG` is set.
    Debug,
    /// Always written.
    Warn,
    /// Always written.
    Error,
}

/// Sets the side name shown in every line (`linux-middleware`, `win32-bridge`); first call wins.
pub fn init(side: &'static str) {
    let debug = std::env::var_os("WINDOWS_FIDO_BRIDGE_DEBUG").is_some();
    _ = SIDE.set((side, debug));
}

/// Whether debug lines are written (`WINDOWS_FIDO_BRIDGE_DEBUG` was set at [`init`]).
pub fn debug_enabled() -> bool {
    SIDE.get().is_some_and(|&(_, debug)| debug)
}

/// Writes `[<unix time>] [<side>] [<level>] <line>` for every line of the message.
pub fn write(level: Level, args: fmt::Arguments) {
    if level == Level::Debug && !debug_enabled() {
        return;
    }
    let side = SIDE.get().map_or("windows-fido-bridge", |&(side, _)| side);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let level = match level {
        Level::Debug => "debug",
        Level::Warn => "warning",
        Level::Error => "error",
    };
    let message = args.to_string();
    let mut out = String::new();
    for line in message.lines() {
        _ = writeln!(
            out,
            "[{}.{:03}] [{side}] [{level}] {line}",
            now.as_secs(),
            now.subsec_millis()
        );
    }
    _ = std::io::stderr().lock().write_all(out.as_bytes());
}

/// Logs a debug line; the arguments are not evaluated when debug output is off.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        // Checked first so disabled debug lines do not evaluate their arguments.
        if $crate::log::debug_enabled() {
            $crate::log::write($crate::log::Level::Debug, format_args!($($arg)*))
        }
    };
}

/// Logs a warning.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Warn, format_args!($($arg)*)) };
}

/// Logs an error.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Error, format_args!($($arg)*)) };
}

/// Hex dump in upstream's layout (`common/src/util.cpp: dump_binary`), every line prefixed with
/// `indent` (upstream logs it with `"  | "`).
pub fn hex_dump(bytes: &[u8], indent: &str) -> String {
    const WIDTH: usize = 16;
    let mut out = format!("{indent}      ");
    (0..WIDTH).for_each(|i| _ = write!(out, " {i:x} "));
    for (row, chunk) in bytes.chunks(WIDTH).enumerate() {
        _ = write!(out, "\n{indent}{:04x}: ", row * WIDTH);
        chunk.iter().for_each(|b| _ = write!(out, "{b:02x} "));
        out.push_str(&" ".repeat((WIDTH - chunk.len()) * 3 + 1));
        out.extend(chunk.iter().map(|&b| match b {
            0x20..=0x7e => char::from(b),
            _ => '.',
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "  |        0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f ";

    #[test]
    fn hex_dump_empty() {
        assert_eq!(hex_dump(&[], "  | "), HEADER);
    }

    #[test]
    fn hex_dump_partial_line() {
        let bytes: Vec<u8> = (b'A'..b'A' + 15).collect();
        let expected = format!(
            "{HEADER}\n  | 0000: 41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f     ABCDEFGHIJKLMNO"
        );
        assert_eq!(hex_dump(&bytes, "  | "), expected);
    }

    #[test]
    fn hex_dump_full_line() {
        let mut bytes: Vec<u8> = (b'A'..b'A' + 15).collect();
        bytes.push(0x00);
        let expected = format!(
            "{HEADER}\n  | 0000: 41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f 00  ABCDEFGHIJKLMNO."
        );
        assert_eq!(hex_dump(&bytes, "  | "), expected);
    }

    #[test]
    fn hex_dump_two_lines() {
        let bytes = [0x7fu8; 17];
        let expected = format!(
            "{HEADER}\n  | 0000: {}  {}\n  | 0010: 7f {}.",
            "7f ".repeat(16).trim_end(),
            ".".repeat(16),
            " ".repeat(46)
        );
        assert_eq!(hex_dump(&bytes, "  | "), expected);
    }
}
