//! Thin WinSCard (PC/SC) adapter for `--list-serials`. PC/SC needs no administrator rights,
//! unlike raw FIDO HID access on Windows, and shows no UI.

use std::{io, ptr};

use wfb_protocol::{debug, warn};
use windows_sys::Win32::Foundation::{
    SCARD_E_NO_READERS_AVAILABLE, SCARD_E_NO_SERVICE, SCARD_E_SERVICE_STOPPED,
};
use windows_sys::Win32::Security::Credentials::{
    SCARD_IO_REQUEST, SCARD_LEAVE_CARD, SCARD_PROTOCOL_T0, SCARD_PROTOCOL_T1, SCARD_SCOPE_USER,
    SCARD_SHARE_SHARED, SCardConnectW, SCardDisconnect, SCardEstablishContext, SCardListReadersW,
    SCardReleaseContext, SCardTransmit,
};

use crate::serial;

/// Serials of the YubiKeys attached to Windows, deduplicated, in reader order. No smart card
/// service or no readers is an empty list; a key that cannot be read is skipped with a warning.
pub fn yubikey_serials() -> io::Result<Vec<u32>> {
    let Some(context) = Context::establish()? else {
        return Ok(Vec::new());
    };
    let mut serials = Vec::new();
    for reader in context.readers()? {
        if !serial::is_yubikey_reader(&reader) {
            continue;
        }
        match Card::connect(&context, &reader)
            .and_then(|card| serial::read_serial(|a| card.transmit(a)))
        {
            Ok(Some(s)) => serials.push(s),
            Ok(None) => debug!("{reader}: no serial exposed"),
            Err(e) => warn!("{reader}: {e}"),
        }
    }
    serial::dedup(&mut serials);
    Ok(serials)
}

/// `true` for the codes that mean "no smart card stack or readers right now" rather than failure.
fn absent(code: i32) -> bool {
    [
        SCARD_E_NO_SERVICE,
        SCARD_E_SERVICE_STOPPED,
        SCARD_E_NO_READERS_AVAILABLE,
    ]
    .contains(&code)
}

fn check(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}

struct Context(usize);

impl Context {
    fn establish() -> io::Result<Option<Self>> {
        let mut handle = 0;
        // SAFETY: reserved pointers must be NULL; `handle` is a valid out-parameter.
        let code = unsafe {
            SCardEstablishContext(SCARD_SCOPE_USER, ptr::null(), ptr::null(), &mut handle)
        };
        if absent(code) {
            return Ok(None);
        }
        check(code)?;
        Ok(Some(Self(handle)))
    }

    fn readers(&self) -> io::Result<Vec<String>> {
        let mut len = 0;
        // SAFETY: a NULL buffer asks for the required length in `len`.
        let code = unsafe { SCardListReadersW(self.0, ptr::null(), ptr::null_mut(), &mut len) };
        if absent(code) {
            return Ok(Vec::new());
        }
        check(code)?;
        let mut buf = vec![0u16; len as usize];
        // SAFETY: `buf` holds `len` u16s, the size SCardListReadersW asked for.
        let code = unsafe { SCardListReadersW(self.0, ptr::null(), buf.as_mut_ptr(), &mut len) };
        if absent(code) {
            return Ok(Vec::new());
        }
        check(code)?;
        Ok(serial::split_multi_sz(&buf[..len as usize]))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from SCardEstablishContext and is released once.
        unsafe { SCardReleaseContext(self.0) };
    }
}

struct Card {
    handle: usize,
    protocol: u32,
}

impl Card {
    fn connect(context: &Context, reader: &str) -> io::Result<Self> {
        let name: Vec<u16> = reader.encode_utf16().chain([0]).collect();
        let (mut handle, mut protocol) = (0, 0);
        // SAFETY: `name` is NUL-terminated and outlives the call; both out-parameters are valid.
        check(unsafe {
            SCardConnectW(
                context.0,
                name.as_ptr(),
                SCARD_SHARE_SHARED,
                SCARD_PROTOCOL_T0 | SCARD_PROTOCOL_T1,
                &mut handle,
                &mut protocol,
            )
        })?;
        Ok(Self { handle, protocol })
    }

    fn transmit(&self, apdu: &[u8]) -> io::Result<Vec<u8>> {
        let pci = SCARD_IO_REQUEST {
            dwProtocol: self.protocol,
            cbPciLength: size_of::<SCARD_IO_REQUEST>() as u32,
        };
        let mut buf = [0u8; 258]; // short APDU response: 256 data bytes + SW1 SW2
        let mut len = buf.len() as u32;
        // SAFETY: `apdu` and `buf` are valid for the lengths passed; `len` is in/out; a NULL
        // receive PCI is allowed.
        check(unsafe {
            SCardTransmit(
                self.handle,
                &pci,
                apdu.as_ptr(),
                apdu.len() as u32,
                ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut len,
            )
        })?;
        Ok(buf[..len as usize].to_vec())
    }
}

impl Drop for Card {
    fn drop(&mut self) {
        // SAFETY: `self.handle` came from SCardConnectW and is disconnected once.
        unsafe { SCardDisconnect(self.handle, SCARD_LEAVE_CARD) };
    }
}
