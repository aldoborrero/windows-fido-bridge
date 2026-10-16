{
  lib,
  craneLib,
  commonArgs,
  cargoArtifacts,
  windowsArgs,
  windowsArtifacts,
  runCommand,
}:
let
  # libwindowsfidobridge.so: the middleware OpenSSH loads via SSH_SK_PROVIDER.
  middleware = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      pname = "windows-fido-bridge-middleware";
      cargoExtraArgs = "--locked --package sk-middleware";
      doCheck = false; # checks.tests runs the whole workspace's tests
    }
  );

  # windowsfidobridge.exe: talks to webauthn.dll; launched through WSL interop.
  bridge = craneLib.buildPackage (windowsArgs // { cargoArtifacts = windowsArtifacts; });
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
