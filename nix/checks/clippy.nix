{ flake, pkgs, ... }:
let
  craneLib = flake.lib.craneLib pkgs;
  src = craneLib.cleanCargoSource flake;
in
craneLib.cargoClippy {
  inherit src;
  strictDeps = true;
  cargoArtifacts = craneLib.buildDepsOnly { inherit src; };
  cargoClippyExtraArgs = "--all-targets -- --deny warnings";
}
