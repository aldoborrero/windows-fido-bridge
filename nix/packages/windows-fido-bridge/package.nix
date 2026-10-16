{
  lib,
  src,
  craneLib,
  windowsTargetEnv,
  runCommand,
}:
let
  common = {
    src = craneLib.cleanCargoSource src;
    strictDeps = true;
  };

  # libwindowsfidobridge.so: the middleware OpenSSH loads via SSH_SK_PROVIDER.
  middleware = craneLib.buildPackage (
    common
    // {
      pname = "windows-fido-bridge-middleware";
      cargoExtraArgs = "--locked --package sk-middleware";
    }
  );

  # windowsfidobridge.exe: talks to webauthn.dll; launched through WSL interop.
  bridge = craneLib.buildPackage (
    common
    // windowsTargetEnv
    // {
      pname = "windows-fido-bridge-exe";
      cargoExtraArgs = "--locked --package webauthn-bridge";
      CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
      doCheck = false; # Windows binaries cannot run in the Linux build sandbox
    }
  );
in
# The middleware looks for the executable next to itself, so both land in lib/.
runCommand "windows-fido-bridge-${middleware.version}"
  {
    passthru = { inherit middleware bridge; };

    meta = {
      description = "OpenSSH security key middleware that signs with FIDO keys through Windows WebAuthn from WSL";
      homepage = "https://github.com/aldoborrero/windows-fido-bridge";
      license = lib.licenses.asl20;
      sourceProvenance = with lib.sourceTypes; [ fromSource ];
      maintainers = with lib.maintainers; [ aldoborrero ];
      platforms = [ "x86_64-linux" ];
    };
  }
  ''
    install -Dm755 ${middleware}/lib/libwindowsfidobridge.so $out/lib/libwindowsfidobridge.so
    install -Dm755 ${bridge}/bin/windowsfidobridge.exe $out/lib/windowsfidobridge.exe
  ''
