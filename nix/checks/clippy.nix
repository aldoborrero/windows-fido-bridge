{ flake, pkgs, ... }:
(flake.lib.craneLib pkgs).cargoClippy (
  flake.lib.commonArgs pkgs
  // {
    cargoArtifacts = flake.lib.cargoArtifacts pkgs;
    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  }
)
