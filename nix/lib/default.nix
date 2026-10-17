{ inputs, ... }:
rec {
  # The toolchain pinned by rust-toolchain.toml, including the Windows target
  # the bridge executable is cross-compiled for.
  rustToolchain =
    pkgs:
    (inputs.rust-overlay.lib.mkRustBin { } pkgs).fromRustupToolchainFile ../../rust-toolchain.toml;

  craneLib = pkgs: (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  # cleanCargoSource plus the golden CBOR fixtures (crates/**/*.hex) the tests read.
  cargoSource =
    pkgs: src:
    pkgs.lib.cleanSourceWith {
      inherit src;
      name = "source";
      filter =
        path: type:
        (pkgs.lib.hasPrefix "${toString src}/crates/" path && pkgs.lib.hasSuffix ".hex" path)
        || (craneLib pkgs).filterCargoSources path type;
    };

  # Arguments shared by every crane derivation, so they agree on one dependency build.
  commonArgs = pkgs: {
    src = cargoSource pkgs inputs.self;
    strictDeps = true;
  };

  # Workspace dependencies for Linux, compiled once and reused by the middleware build, the
  # tests and clippy.
  cargoArtifacts = pkgs: (craneLib pkgs).buildDepsOnly (commonArgs pkgs);

  # The Windows executable, cross-compiled with nixpkgs' mingw-w64.
  windowsArgs =
    pkgs:
    commonArgs pkgs
    // windowsTargetEnv pkgs
    // {
      pname = "windows-fido-bridge-exe";
      CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
      cargoExtraArgs = "--locked --package webauthn-bridge";
      # dlltool for windows-sys raw-dylib imports (webauthn.dll, user32.dll).
      nativeBuildInputs = [ pkgs.pkgsCross.mingwW64.stdenv.cc ];
      doCheck = false; # Windows binaries cannot run in the Linux build sandbox
    };

  # Dependencies of the Windows executable, shared by its build and its clippy check.
  windowsArtifacts = pkgs: (craneLib pkgs).buildDepsOnly (windowsArgs pkgs);

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
