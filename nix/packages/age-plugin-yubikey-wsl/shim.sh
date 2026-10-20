#!@shell@
# shellcheck shell=bash
# age-plugin-yubikey for WSL: runs the Windows build of the plugin through WSL interop, so it
# reaches YubiKeys attached to Windows over WinSCard. age finds this file on PATH and speaks the
# plugin protocol over stdin/stdout; exec hands argv, stdin, stdout, stderr and the exit status
# to the Windows process unchanged.

# The plugin's log filter (env_logger) crosses to Windows only when listed in WSLENV.
export WSLENV="${WSLENV:+$WSLENV:}RUST_LOG"
exec "@exe@" "$@"
