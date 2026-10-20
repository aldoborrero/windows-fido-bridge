{ flake, pkgs, ... }:
pkgs.callPackage ./package.nix {
  craneLib = flake.lib.craneLib pkgs;
  windowsCrossArgs = flake.lib.windowsCrossArgs pkgs;
}
