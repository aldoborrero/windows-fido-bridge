//! Invisible always-on-top owner window for the WebAuthn dialogs (upstream
//! `win32_middleware_common/src/window.cpp`). `GetConsoleWindow` does not work as the parent in
//! Windows Terminal or VS Code, so the dialogs attach to this window instead.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr;

use wfb_protocol::{debug, warn};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetWindowThreadProcessId, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    SW_SHOWNORMAL, SetForegroundWindow, SetParent, ShowWindow, TranslateMessage, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUPWINDOW,
};
use windows_sys::core::w;

/// Runs `f` on a worker thread with the window handle while pumping this thread's messages,
/// then destroys the window.
pub fn run<T: Send + 'static>(f: impl FnOnce(HWND) -> T + Send + 'static) -> io::Result<T> {
    // SAFETY: plain Win32 call; a NULL module name means the current executable. "STATIC" is a
    // predefined class, so no window class needs registering. All pointers are static strings.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("windows-fido-bridge"),
            WS_POPUPWINDOW,
            960,
            540,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `hwnd` is a window this thread just created.
    unsafe { ShowWindow(hwnd, SW_SHOWNORMAL) };
    // SAFETY: as above.
    if unsafe { SetForegroundWindow(hwnd) } == 0 && !reparent_to_vs_code(hwnd) {
        warn!(
            "Please ensure your foreground window is set to the Windows security key prompt \
             before entering your security key PIN."
        );
    }

    // HWND is a raw pointer and not Send; the worker only passes it back to Win32.
    let hwnd_addr = hwnd as usize;
    debug!("Spawning background thread");
    let worker = std::thread::spawn(move || f(hwnd_addr as HWND));
    let thread: HANDLE = worker.as_raw_handle();
    loop {
        // SAFETY: `thread` stays valid until `worker` is joined below.
        let woke = unsafe { MsgWaitForMultipleObjects(1, &thread, 0, INFINITE, QS_ALLINPUT) };
        if woke != WAIT_OBJECT_0 + 1 {
            break; // the worker finished (or waiting failed and join() blocks instead)
        }
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid out-parameter; NULL hwnd reads all of this thread's messages.
        while unsafe { PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            // SAFETY: `msg` was filled in by PeekMessageW.
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    let result = worker.join();
    // SAFETY: `hwnd` was created on this thread and is destroyed once.
    unsafe { DestroyWindow(hwnd) };
    result.map_err(|_| io::Error::other("WebAuthn worker thread panicked"))
}

/// Upstream workaround: in a VS Code Remote-WSL terminal `SetForegroundWindow` is denied, but
/// making VS Code's window our parent brings the WebAuthn dialog to the front.
fn reparent_to_vs_code(hwnd: HWND) -> bool {
    debug!("Setting the foreground window was denied; checking for VS Code.");
    // SAFETY: no preconditions; NULL is handled below.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut pid = 0;
    // SAFETY: `foreground` is a window handle and `pid` a valid out-parameter.
    if unsafe { GetWindowThreadProcessId(foreground, &mut pid) } == 0 {
        return false;
    }
    // SAFETY: no pointer arguments; the handle is checked and closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    // SAFETY: `path` holds `len` u16s and `len` is a valid in/out parameter.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len)
    };
    // SAFETY: `process` was opened above and is closed once.
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return false;
    }
    let path = String::from_utf16_lossy(&path[..len as usize]);
    debug!("Foreground window process image: \"{path}\"");
    if path.rsplit('\\').next() != Some("Code.exe") {
        return false;
    }
    debug!("Reparenting the windows-fido-bridge host window to VS Code.");
    // SAFETY: both are valid window handles; `hwnd` belongs to this thread.
    unsafe { !SetParent(hwnd, foreground).is_null() }
}
