//! Windows side of the bridge: reads a request from the middleware on stdin,
//! performs it through `webauthn.dll` and writes the reply to stdout. `--list-serials` instead
//! prints the serials of attached YubiKeys, one per line, via PC/SC (used by gfh).
//!
//! A console-subsystem executable, like upstream's (its `WinMain` is only the mingw entry point;
//! CMake builds it without `WIN32`), so stderr diagnostics reach the WSL terminal.

mod convert;
#[cfg(windows)]
mod pcsc;
mod serial;
#[cfg(windows)]
mod webauthn;
#[cfg(windows)]
mod window;

use std::process::ExitCode;

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("windowsfidobridge: this executable only runs on Windows");
    ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> ExitCode {
    wfb_protocol::log::init("win32-bridge");
    let result = if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--list-serials")
    {
        list_serials()
    } else {
        bridge::serve()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            wfb_protocol::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Writes one decimal serial per line to stdout; does not touch stdin.
#[cfg(windows)]
fn list_serials() -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    for s in pcsc::yubikey_serials()? {
        writeln!(out, "{s}")?;
    }
    out.flush()
}

#[cfg(windows)]
mod bridge {
    use std::io;

    use wfb_protocol::message::{
        self, EnrollParams, EnrollResult, Request, Response, SignParams, SignResult,
    };
    use wfb_protocol::sk::{
        SSH_SK_ECDSA, SSH_SK_ED25519, SSH_SK_ERR_GENERAL, SSH_SK_ERR_UNSUPPORTED,
    };
    use wfb_protocol::{debug, error, frame};

    use crate::{convert, webauthn, window};

    /// Serves one request; an unreadable or unknown request is an error (non-zero exit).
    pub fn serve() -> io::Result<()> {
        let raw = frame::read(&mut io::stdin().lock())?;
        debug!("Received CBOR from caller: {}", message::diagnostic(&raw));
        let reply = match message::decode::<Request>(&raw)? {
            Request::Enroll(p) => message::encode(&respond(enroll(&p)))?,
            Request::Sign(p) => message::encode(&sign(&p))?,
        };
        debug!("Sending CBOR to caller: {}", message::diagnostic(&reply));
        frame::write(&mut io::stdout().lock(), &reply)
    }

    fn respond<T>(result: io::Result<T>) -> Response<T> {
        result.map_or_else(
            |e| {
                error!("{e}");
                Response::error(SSH_SK_ERR_GENERAL)
            },
            Response::ok,
        )
    }

    fn uv_required(application: &str, flags: u8) -> bool {
        let force = std::env::var_os("WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION").is_some();
        let required = convert::user_verification_required(application, flags, force);
        debug!("User verification required: {required}");
        required
    }

    fn sign(p: &SignParams) -> Response<SignResult> {
        debug!(
            "Sign request: algorithm {}, application \"{}\", flags 0b{:08b}, key handle {} bytes, data {} bytes",
            p.alg,
            p.application,
            p.flags,
            p.key_handle.len(),
            p.data.len()
        );
        if p.alg != SSH_SK_ECDSA && p.alg != SSH_SK_ED25519 {
            error!(
                "Algorithm {:#04x} is not supported by Microsoft's WebAuthn API",
                p.alg
            );
            return Response::error(SSH_SK_ERR_UNSUPPORTED);
        }
        respond(get_signature(p))
    }

    fn get_signature(p: &SignParams) -> io::Result<SignResult> {
        let uv = uv_required(&p.application, p.flags);
        let (application, data, key_handle) =
            (p.application.clone(), p.data.clone(), p.key_handle.clone());
        let a = window::run(move |hwnd| {
            webauthn::get_assertion(hwnd, &application, &data, &key_handle, uv)
        })??;
        convert::sign_result(p.alg, &a.authenticator_data, &a.signature)
    }

    fn enroll(p: &EnrollParams) -> io::Result<EnrollResult> {
        let user = convert::enroll_user_name(&p.sk_options).to_owned();
        debug!(
            "Enroll request: algorithm {}, application \"{}\", flags 0b{:08b}, user \"{user}\"",
            p.alg, p.application, p.flags
        );
        let uv = uv_required(&p.application, p.flags);
        let (alg, application, challenge) = (p.alg, p.application.clone(), p.challenge.clone());
        let object = window::run(move |hwnd| {
            webauthn::make_credential(hwnd, alg, &application, &user, &challenge, uv)
        })??;
        convert::parse_attestation(&object)
    }
}
