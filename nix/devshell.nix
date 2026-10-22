{
  flake,
  pkgs,
  perSystem,
}:
perSystem.devshell.mkShell {
  packages = with pkgs; [
    just
    perSystem.self.formatter

    # age client for `just try-age`.
    age

    # Rust toolchain from rust-toolchain.toml, with the x86_64-pc-windows-gnu target.
    (flake.lib.rustToolchain pkgs)
    # Linker and C runtime for the Windows bridge executable.
    pkgsCross.mingwW64.stdenv.cc
  ];

  env = [
    {
      name = "NIX_PATH";
      value = "nixpkgs=${toString pkgs.path}";
    }
    {
      name = "NIX_DIR";
      eval = "$PRJ_ROOT/nix";
    }
    {
      # Keep the toolchain cache in the project rather than $HOME.
      name = "CARGO_HOME";
      eval = "$PRJ_ROOT/.cargo";
    }
  ]
  ++ pkgs.lib.mapAttrsToList (name: value: { inherit name value; }) (flake.lib.windowsTargetEnv pkgs);
}
