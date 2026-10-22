# Runs the age-plugin-yubikey shim against a stand-in for the Windows executable and checks that
# argv, stdin, stdout, stderr and the exit status pass through unchanged, and that RUST_LOG is
# added to WSLENV. The real executable cannot run in the build sandbox.
{ pkgs, ... }:
let
  fakeExe = pkgs.writeShellScript "age-plugin-yubikey.exe" ''
    printf 'arg:%s\n' "$@"
    printf 'WSLENV=%s\n' "$WSLENV"
    cat
    echo to-stderr >&2
    exit 7
  '';
in
pkgs.runCommand "age-shim-check" { } ''
  substitute ${../packages/age-plugin-yubikey-wsl/shim.sh} shim \
    --subst-var-by shell ${pkgs.runtimeShell} \
    --subst-var-by exe ${fakeExe}
  chmod +x shim

  status=0
  printf -- '-> add-identity x\nbody\n' |
    WSLENV=FOO/u ./shim --age-plugin=identity-v1 'two words' >stdout 2>stderr || status=$?

  test "$status" = 7
  printf -- '%s\n' 'arg:--age-plugin=identity-v1' 'arg:two words' 'WSLENV=FOO/u:RUST_LOG' \
    '-> add-identity x' body >expected
  diff -u expected stdout
  test "$(cat stderr)" = to-stderr

  # Without a WSLENV of its own, the shim adds no leading colon.
  env -u WSLENV ./shim </dev/null >bare 2>/dev/null || true
  grep -qx 'WSLENV=RUST_LOG' bare
  touch $out
''
