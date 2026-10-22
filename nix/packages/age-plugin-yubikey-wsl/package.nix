{
  lib,
  craneLib,
  windowsCrossArgs,
  fetchFromGitHub,
  runCommand,
  runtimeShell,
}:
let
  version = "0.5.1";

  # age-plugin-yubikey.exe: the upstream plugin, built from source for Windows, where it reaches
  # the YubiKey's PIV applet through WinSCard (the Smart Card service).
  exe = craneLib.buildPackage (
    windowsCrossArgs
    // {
      pname = "age-plugin-yubikey-exe";
      inherit version;
      src = fetchFromGitHub {
        owner = "str4d";
        repo = "age-plugin-yubikey";
        tag = "v${version}";
        hash = "sha256-RmRc71BhDv1zAyuQ85sK/EXH+Wz6dAA5LqLtgmVWQ4c=";
      };
      strictDeps = true;
      cargoExtraArgs = "--locked";
    }
  );
in
# The shim runs the executable from the same output, like the middleware and its bridge.
runCommand "age-plugin-yubikey-wsl-${version}"
  {
    passthru = { inherit exe; };

    meta = {
      description = "age-plugin-yubikey for WSL, running the Windows build against YubiKeys attached to Windows";
      homepage = "https://github.com/aldoborrero/windows-fido-bridge";
      license = with lib.licenses; [
        asl20
        mit
      ];
      sourceProvenance = with lib.sourceTypes; [ fromSource ];
      maintainers = with lib.maintainers; [ aldoborrero ];
      mainProgram = "age-plugin-yubikey";
      platforms = [ "x86_64-linux" ];
    };
  }
  ''
    install -Dm755 ${exe}/bin/age-plugin-yubikey.exe $out/lib/age-plugin-yubikey.exe
    mkdir -p $out/bin
    substitute ${./shim.sh} $out/bin/age-plugin-yubikey \
      --subst-var-by shell ${runtimeShell} \
      --subst-var-by exe $out/lib/age-plugin-yubikey.exe
    chmod 755 $out/bin/age-plugin-yubikey
  ''
