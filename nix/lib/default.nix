{ inputs, ... }:
rec {
  # The toolchain pinned by rust-toolchain.toml, including the Windows target
  # the bridge executable is cross-compiled for.
  rustToolchain =
    pkgs:
    (inputs.rust-overlay.lib.mkRustBin { } pkgs).fromRustupToolchainFile ../../rust-toolchain.toml;

  craneLib = pkgs: (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  # Environment that makes cargo link x86_64-pc-windows-gnu with nixpkgs' mingw-w64.
  windowsTargetEnv =
    pkgs:
    let
      mingw = pkgs.pkgsCross.mingwW64;
      cc = "${mingw.stdenv.cc}/bin/${mingw.stdenv.cc.targetPrefix}cc";
    in
    {
      CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = cc;
      CC_x86_64_pc_windows_gnu = cc;
      CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${mingw.windows.pthreads}/lib";
    };
}
