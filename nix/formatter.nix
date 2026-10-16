{
  flake,
  inputs,
  pkgs,
  ...
}:
let
  mod = inputs.treefmt-nix.lib.evalModule pkgs {
    projectRootFile = "flake.nix";
    programs = {
      # nix — deadnix strips dead code, then nixfmt formats
      deadnix.enable = true;
      deadnix.no-lambda-pattern-names = true;
      deadnix.priority = 1;
      nixfmt.enable = true;
      nixfmt.priority = 2;

      # shell
      shellcheck.enable = true;
      shellcheck.includes = [
        "*.sh"
        "*.bash"
        "*.envrc"
        "*.envrc.*"
      ];
      shellcheck.priority = 1;
      shfmt.enable = true;
      shfmt.includes = [
        "*.sh"
        "*.bash"
        "*.envrc"
        "*.envrc.*"
      ];
      shfmt.priority = 2;

      rustfmt.enable = true;
      rustfmt.package = flake.lib.rustToolchain pkgs;

      yamlfmt.enable = true;
      yamlfmt.settings.formatter = {
        type = "basic";
        indent = 2;
        retain_line_breaks = true;
      };
      jsonfmt.enable = true;
      taplo.enable = true;
      mdformat.enable = true;
      just.enable = true;
    };
    settings.excludes = [
      ".specify/**"
      ".agents/**"
      ".claude/**"
      "specs/**"
    ];
  };
  wrapper = mod.config.build.wrapper;
in
wrapper
// {
  passthru = wrapper.passthru // {
    tests.check = mod.config.build.check flake;
  };
}
