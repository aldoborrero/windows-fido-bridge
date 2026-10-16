{ flake, pkgs, ... }:
pkgs.callPackage ./package.nix {
  craneLib = flake.lib.craneLib pkgs;
  windowsTargetEnv = flake.lib.windowsTargetEnv pkgs;
  src = flake;
}
