{ flake, ... }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.windows-fido-bridge;
in
{
  options.programs.windows-fido-bridge = {
    enable = lib.mkEnableOption "signing with FIDO keys attached to Windows, through WebAuthn, from OpenSSH in WSL";

    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.stdenv.hostPlatform.system}.windows-fido-bridge;
      defaultText = lib.literalExpression "windows-fido-bridge.packages.\${system}.windows-fido-bridge";
      description = "Package providing `lib/libwindowsfidobridge.so` and `lib/windowsfidobridge.exe`.";
    };
  };

  config = lib.mkIf cfg.enable {
    # ssh, ssh-keygen and ssh-add use this middleware for every `-sk` key.
    environment.variables.SSH_SK_PROVIDER = "${cfg.package}/lib/libwindowsfidobridge.so";

    # ssh-agent only loads middleware from /usr/lib* unless told otherwise (-P).
    programs.ssh.agentPKCS11Whitelist = lib.mkDefault "${cfg.package}/lib/*";
  };
}
