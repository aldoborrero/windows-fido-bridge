//! Runs `windowsfidobridge.exe` (next to this library) and exchanges one framed message with it
//! (upstream `linux_middleware/src/bridge.cpp`).

use std::ffi::{CStr, OsStr, OsString, c_void};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use wfb_protocol::{debug, frame, log};

const EXE_NAME: &str = "windowsfidobridge.exe";

/// Variables forwarded from WSL to the Windows process.
const FORWARDED_VARS: [&str; 2] = [
    "WINDOWS_FIDO_BRIDGE_DEBUG",
    "WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION",
];

/// Sends `payload` to the Windows bridge and returns its reply.
pub fn invoke(payload: &[u8]) -> io::Result<Vec<u8>> {
    invoke_exe(&library_dir()?.join(EXE_NAME), payload)
}

fn invoke_exe(exe: &Path, payload: &[u8]) -> io::Result<Vec<u8>> {
    let wslenv = wslenv_value(std::env::var_os("WSLENV"));
    debug!("Using Windows bridge at \"{}\".", exe.display());
    debug!(
        "Setting WSLENV environment variable to \"{}\".",
        wslenv.display()
    );
    let mut cmd = Command::new(exe);
    cmd.env("WSLENV", wslenv);
    exchange(cmd, payload).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", exe.display())))
}

fn exchange(mut cmd: Command, payload: &[u8]) -> io::Result<Vec<u8>> {
    // The bridge rejects larger messages and exits; writing into its closed stdin would then
    // raise SIGPIPE and kill the OpenSSH process that loaded us.
    if payload.len() > frame::MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "request of {} bytes exceeds the {} byte message limit",
                payload.len(),
                frame::MAX_MESSAGE_SIZE
            ),
        ));
    }
    debug!(
        "Invoking Windows bridge with the following parameters:\n{}",
        log::hex_dump(payload, "  | ")
    );
    // SAFETY: the closure runs in the forked child before exec and only calls close_range(2),
    // which is async-signal-safe and touches no memory.
    unsafe {
        cmd.pre_exec(|| {
            // The host's descriptors (e.g. ssh-sk-helper's socket) must not leak into the bridge.
            // Mark them close-on-exec rather than closing them, so std's own CLOEXEC pipe that
            // reports exec failures keeps working. Kernels without close_range (< 5.11, never
            // WSL 2's) keep the old behaviour of leaking them.
            libc::close_range(3, u32::MAX, libc::CLOSE_RANGE_CLOEXEC as libc::c_int);
            Ok(())
        })
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    debug!("Child process PID = {}", child.id());
    let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
    // Dropping the pipes before waiting lets the child see EOF / EPIPE if we bail out early.
    let reply = (|| {
        let (mut stdin, mut stdout) = stdin.zip(stdout).ok_or(io::ErrorKind::BrokenPipe)?;
        frame::write(&mut stdin, payload)?;
        drop(stdin);
        frame::read(&mut stdout)
    })();
    let status = child.wait()?;
    debug!("Child process exited with {status}.");
    let reply = reply?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "child process exited with {status}"
        )));
    }
    debug!(
        "Reply received from child process:\n{}",
        log::hex_dump(&reply, "  | ")
    );
    Ok(reply)
}

/// The existing `WSLENV` plus the forwarded variables, `:`-separated.
fn wslenv_value(existing: Option<OsString>) -> OsString {
    let mut value = existing.unwrap_or_default();
    for var in FORWARDED_VARS {
        if !value.is_empty() {
            value.push(":");
        }
        value.push(var);
    }
    value
}

/// Directory holding this shared library, found with `dladdr` on one of its functions.
fn library_dir() -> io::Result<PathBuf> {
    let addr = library_dir as fn() -> io::Result<PathBuf> as *const c_void;
    // SAFETY: Dl_info is plain old data, so all-zero is a valid value.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    // SAFETY: `addr` is an address inside this object and `info` is a valid out-parameter.
    if unsafe { libc::dladdr(addr, &mut info) } == 0 || info.dli_fname.is_null() {
        return Err(io::Error::other(
            "dladdr could not locate the middleware library",
        ));
    }
    // SAFETY: on success dli_fname is a NUL-terminated path owned by the dynamic loader.
    let file = unsafe { CStr::from_ptr(info.dli_fname) };
    let file = std::path::absolute(Path::new(OsStr::from_bytes(file.to_bytes())))?;
    debug!("Detected own library file path is \"{}\".", file.display());
    file.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("library path has no parent directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORWARDED: &str = "WINDOWS_FIDO_BRIDGE_DEBUG:WINDOWS_FIDO_BRIDGE_FORCE_USER_VERIFICATION";

    #[test]
    fn wslenv_without_existing_value() {
        assert_eq!(wslenv_value(None), FORWARDED);
    }

    #[test]
    fn wslenv_appends_to_existing_value() {
        assert_eq!(
            wslenv_value(Some("FOO/u".into())),
            format!("FOO/u:{FORWARDED}").as_str()
        );
    }

    #[test]
    fn wslenv_empty_existing_value_has_no_leading_colon() {
        assert_eq!(wslenv_value(Some("".into())), wslenv_value(None));
    }

    #[test]
    fn wslenv_keeps_non_utf8_value() {
        let existing = OsStr::from_bytes(b"A\xff/u").to_owned();
        let value = wslenv_value(Some(existing));
        assert_eq!(
            value.as_bytes(),
            [&b"A\xff/u:"[..], FORWARDED.as_bytes()].concat()
        );
    }

    #[test]
    fn missing_executable_is_an_error() {
        let err = invoke_exe(Path::new("/nonexistent/windowsfidobridge.exe"), b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(
            err.to_string()
                .starts_with("/nonexistent/windowsfidobridge.exe: ")
        );
    }

    #[test]
    fn library_dir_resolves() {
        assert!(library_dir().unwrap().is_absolute());
    }

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[test]
    fn exchanges_one_frame() {
        // The child echoes the request frame, which is itself a valid reply frame.
        assert_eq!(exchange(sh("cat"), b"\xa0").unwrap(), b"\xa0");
    }

    #[test]
    fn non_zero_exit_is_an_error() {
        let err = exchange(sh("cat; exit 3"), b"\xa0").unwrap_err();
        assert_eq!(err.to_string(), "child process exited with exit status: 3");
    }

    #[test]
    fn early_close_is_an_error() {
        let err = exchange(sh("exit 0"), b"\xa0").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn oversized_request_is_rejected_before_spawning() {
        let dir = std::env::temp_dir().join(format!("wfb-oversized-{}", std::process::id()));
        let script = format!("touch {}; cat", dir.display());
        let err = exchange(sh(&script), &[0; frame::MAX_MESSAGE_SIZE + 1]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!dir.exists(), "the child must not have been spawned");
        // The limit itself is still allowed. `tail` buffers until EOF: a streaming `cat` deadlocks
        // once pipes shrink to one page (over `fs.pipe-user-pages-soft`), as exchange() writes
        // the whole request before reading.
        let max = vec![0xa0; frame::MAX_MESSAGE_SIZE];
        assert_eq!(exchange(sh("tail -c 65536"), &max).unwrap(), max);
    }

    #[test]
    fn host_descriptors_do_not_leak_into_the_child() {
        // A descriptor without close-on-exec, like ssh-sk-helper's dup'd socket.
        let null = std::fs::File::open("/dev/null").unwrap();
        // SAFETY: F_DUPFD returns a new descriptor (or -1) and has no memory effects.
        let fd = unsafe { libc::fcntl(std::os::fd::AsRawFd::as_raw_fd(&null), libc::F_DUPFD, 500) };
        assert!(fd >= 500);
        // The child lists its own descriptors and returns them as the reply frame.
        let script =
            "cat >/dev/null; fds=$(ls /proc/$$/fd | tr '\\n' ' '); printf '%s|%s' ${#fds} \"$fds\"";
        let reply = exchange(sh(script), b"\xa0");
        // SAFETY: `fd` was opened above and is closed once.
        unsafe { libc::close(fd) };
        let reply = String::from_utf8(reply.unwrap()).unwrap();
        let fds: Vec<&str> = reply.split_whitespace().collect();
        assert!(fds.contains(&"0") && fds.contains(&"1"), "{reply}");
        assert!(
            !fds.contains(&fd.to_string().as_str()),
            "fd {fd} leaked: {reply}"
        );
    }
}
