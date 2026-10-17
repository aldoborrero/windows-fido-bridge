{ flake, pkgs, ... }:
pkgs.callPackage ./package.nix {
  craneLib = flake.lib.craneLib pkgs;
  commonArgs = flake.lib.commonArgs pkgs;
  cargoArtifacts = flake.lib.cargoArtifacts pkgs;
  windowsArgs = flake.lib.windowsArgs pkgs;
  windowsArtifacts = flake.lib.windowsArtifacts pkgs;
}
