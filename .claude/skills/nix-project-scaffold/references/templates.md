# File templates

Copied from the reference repos (cfleet = Go, reolink-exporter = Python); language-specific parts are marked. Everything else should be reproduced verbatim.

## .envrc (identical in all repos)

```bash
#!/usr/bin/env bash

# Load the Nix flake
use flake

# Load local environment variables if .envrc.local exists
source_env_if_exists .envrc.local
```

`.envrc.local` is the per-machine escape hatch (credentials, overrides) and is gitignored.

## .gitignore

```gitignore
# nix
result
result-*

# direnv
.direnv/
.envrc.local
```

plus the language block — Go: `.go/` and the built binary name; Python: `.venv/`, `__pycache__/`, `*.py[cod]`, `.mypy_cache/`, `.pytest_cache/`, `.ruff_cache/`.

## nix/devshell.nix (numtide devshell; `devshell` input required)

```nix
{ pkgs, perSystem }:
perSystem.devshell.mkShell {
  packages = (
    with pkgs;
    [
      just
      perSystem.self.formatter

      # <language toolchain — comment WHY each tool is present>
      go gopls golangci-lint gotools          # Go variant
      # python313 uv ruff mypy                # Python variant
    ]
  );

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
      name = "GOPATH";                        # Go variant
      eval = "$PRJ_ROOT/.go";
      # name = "UV_PROJECT_ENVIRONMENT";      # Python variant
      # eval = "$PRJ_ROOT/.venv";
    }
  ];
}
```

devshell's `env` is a **list** of `{ name, value|eval }` attrsets (`eval` for values referencing `$PRJ_ROOT`); nixpkgs' `mkShell` uses a plain attrset instead.

## nix/formatter.nix

Use the template from the `nix-treefmt-formatter` skill: `treefmt-nix.lib.evalModule` with deadnix→nixfmt (priorities, `no-lambda-pattern-names`), shellcheck→shfmt (includes covering `*.envrc*`), yamlfmt/jsonfmt/taplo/mdformat/just, plus the language block (gofmt | ruff-check→ruff-format), exposing `build.wrapper` with `build.check flake` attached via **`passthru.tests`**.

## nix/packages/<name>/default.nix (the shim)

```nix
{ flake, pkgs, ... }:
pkgs.callPackage ./package.nix { src = flake; }
```

Python variant: `pkgs.python313Packages.callPackage ./package.nix { src = flake; }` (gives `package.nix` access to python-scoped deps by name). Depending on a sibling package: add `other = perSystem.self.other;` to the override set.

## nix/packages/<name>/package.nix — Go

```nix
{
  lib,
  src,
  buildGoModule,
}:
buildGoModule {
  pname = "<name>";
  version = "0.1.0";

  inherit src;

  vendorHash = null; # no external modules yet; set to the real hash once deps land

  # go test ./... runs by default in checkPhase

  meta = {
    description = "<same one-liner as the flake description>";
    homepage = "https://github.com/aldoborrero/<name>";
    license = lib.licenses.mit;
    sourceProvenance = with lib.sourceTypes; [ fromSource ];
    maintainers = with lib.maintainers; [ aldoborrero ];
    mainProgram = "<name>";
    platforms = lib.platforms.unix;
  };
}
```

If the binary shells out to external tools, wrap it (add `makeWrapper` + the tool to the argument set):

```nix
nativeBuildInputs = [ makeWrapper ];
postInstall = ''
  wrapProgram $out/bin/<name> --prefix PATH : ${lib.makeBinPath [ openssh ]}
'';
```

`vendorHash` bookkeeping: with any external dependency, `vendorHash = null` stops working; set it to the value from the mismatch error of a `lib.fakeHash` build.

## nix/packages/<name>/package.nix — Python

```nix
{
  lib,
  src,
  buildPythonApplication,
  uv-build,
  # runtime deps, by nixpkgs python-package name:
  prometheus-client,
  # test deps:
  pytestCheckHook,
  pytest-asyncio,
}:
buildPythonApplication {
  pname = "<name>";
  version = "0.1.0";
  pyproject = true;

  inherit src;

  build-system = [ uv-build ];

  dependencies = [ prometheus-client ];

  nativeCheckInputs = [ pytestCheckHook pytest-asyncio ];

  pythonImportsCheck = [ "<import_name>" ];

  meta = {
    description = "<one-liner>";
    homepage = "https://github.com/aldoborrero/<name>";
    license = lib.licenses.mit;
    sourceProvenance = with lib.sourceTypes; [ fromSource ];
    maintainers = with lib.maintainers; [ aldoborrero ];
    mainProgram = "<name>";
    platforms = lib.platforms.all;
  };
}
```

Deps come from **nixpkgs**, not `uv.lock` — uv owns the local venv, nixpkgs owns the Nix build; keeping them compatible is a documented project invariant, not something the build enforces.

## OCI image package (only when the project ships a container)

`nix/packages/docker/default.nix`:

```nix
{ pkgs, perSystem }:
pkgs.callPackage ./package.nix { <name> = perSystem.self.<name>; }
```

`package.nix`: `dockerTools.buildLayeredImage` over a `buildEnv` of the app + `cacert` (`pathsToLink = [ "/bin" "/etc" "/lib" "/share" ]`), `tag = <name>.version`, numeric `User = "10001:10001"` (satisfies Kubernetes `runAsNonRoot` without /etc/passwd), `Env` with `PATH=/bin` and `SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt` (+ language flags like `PYTHONUNBUFFERED=1`), `ExposedPorts`. Publishing is a justfile recipe, not Nix (see ci-and-justfile.md) — via skopeo, because OCI registries like Zot reject `docker push`.
