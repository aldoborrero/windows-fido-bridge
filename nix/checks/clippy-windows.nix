{ flake, pkgs, ... }:
# Lints the Windows-only modules (webauthn.rs, window.rs), which the Linux clippy check skips.
(flake.lib.craneLib pkgs).cargoClippy (
  flake.lib.windowsArgs pkgs
  // {
    cargoArtifacts = flake.lib.windowsArtifacts pkgs;
    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  }
)
