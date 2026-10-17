{ flake, pkgs, ... }:
# Runs every unit test, including the bridge's conversion tests, on Linux.
(flake.lib.craneLib pkgs).cargoTest (
  flake.lib.commonArgs pkgs
  // {
    cargoArtifacts = flake.lib.cargoArtifacts pkgs;
    cargoTestExtraArgs = "--workspace";
  }
)
