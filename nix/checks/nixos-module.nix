# Evaluates the NixOS module and checks what it wires up, without building a system.
{
  flake,
  inputs,
  pkgs,
  system,
  ...
}:
let
  nixos = inputs.nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      flake.nixosModules.windows-fido-bridge
      {
        programs.windows-fido-bridge.enable = true;
        programs.ssh.startAgent = true;
        boot.isContainer = true;
        system.stateVersion = "26.05";
      }
    ];
  };
  cfg = nixos.config;
  package = flake.packages.${system}.windows-fido-bridge;
in
assert cfg.environment.variables.SSH_SK_PROVIDER == "${package}/lib/libwindowsfidobridge.so";
# hasInfix builds a regex, which may not carry store-path context.
assert pkgs.lib.hasInfix (builtins.unsafeDiscardStringContext "-P ${package}/lib/*") (
  builtins.unsafeDiscardStringContext cfg.systemd.user.services.ssh-agent.serviceConfig.ExecStart
);
pkgs.runCommand "nixos-module-check" { } "touch $out"
