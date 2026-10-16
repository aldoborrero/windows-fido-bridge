# justfile and CI

## justfile — the facade

Every dev action is a recipe; contributors and agents never need to remember raw commands. From the Python reference repo (adapt recipes to the language):

```just
# <name>

registry := env_var_or_default("REGISTRY", "registry.example.com")   # set your registry here
image := "<name>"

default:
    @just --list

# Run the test suite
test *args:
    uv run pytest {{ args }}          # Go: go test ./... {{ args }}

# Lint and type-check
lint:
    uv run ruff check src tests
    uv run mypy                       # Go: golangci-lint run

# Format everything (nix, <lang>, yaml, toml, markdown, shell)
fmt:
    nix fmt

# Every check CI runs
check:
    nix flake check --log-format bar-with-logs

# Build the package
build:
    nix build .#<name> --log-format bar-with-logs

# Build the OCI image into ./result       (only for container projects)
image:
    nix build .#docker --log-format bar-with-logs

# Push the built image. Zot rejects `docker push`, it needs OCI via skopeo.
push tag: image
    skopeo copy \
      docker-archive:./result \
      docker://{{ registry }}/{{ image }}:{{ tag }} \
      --format oci
```

Conventions: `default` lists recipes; fast native-toolchain loops (`test`, `lint`, `run`) call the language tool directly, everything hermetic goes through nix; `--log-format bar-with-logs` for local readability; `skopeo` must be in the devshell if `push` exists.

## .github/workflows/ci.yml

Two jobs sharing three identical setup steps. Key moves: actions pinned by **commit SHA with the version as a comment**, and `nix_path` derived from `flake.lock` so CI and local builds use the exact same nixpkgs:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  check:
    name: Flake Check
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0

      - name: Resolve pinned nixpkgs
        id: nixpkgs
        run: echo "url=https://github.com/NixOS/nixpkgs/archive/$(jq -r '.nodes.nixpkgs.locked.rev' flake.lock).tar.gz" >> "$GITHUB_OUTPUT"

      - name: Install Nix
        uses: cachix/install-nix-action@8aa03977d8d733052d78f4e008a241fd1dbf36b3 # v31.10.6
        with:
          nix_path: nixpkgs=${{ steps.nixpkgs.outputs.url }}
          extra_nix_config: |
            experimental-features = nix-command flakes

      # Builds the package (which runs the test suite) and checks formatting.
      - name: Run flake check
        run: nix flake check --print-build-logs

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      # same three setup steps
      - name: Build the package
        run: nix build .#<name> --print-build-logs
```

Why one `nix flake check` suffices: blueprint puts the package (tests included via `nativeCheckInputs` / `go test`), every devshell, and the formatter's `passthru.tests` check into `checks.<sys>.*` — the single invocation covers build + tests + formatting. No separate lint/test jobs. The Build job therefore re-builds what the check job already built (the reference repo keeps it for per-artifact logs); without a binary cache that doubles CI compute — on cost-sensitive repos drop it, or add the cache first.

No binary cache is configured in the reference setup; if builds get heavy, add one (cachix or `nix-community/cache-nix-action` keyed on `hashFiles('**/flake.lock')` — the latter is used in the NixOS-config repo's CI together with `nix flake check --keep-going`).

## Scaffold checklist

```bash
git init
# write: flake.nix .envrc .gitignore justfile nix/devshell.nix nix/formatter.nix nix/packages/<name>/{default,package}.nix
git add -A                      # BEFORE any nix command — untracked files are invisible
nix flake lock
nix flake show                  # expect: devShells.default, formatter, packages.<name>, checks.pkgs-*
nix fmt && git add -A           # formatter check must pass on the first CI run
nix build .#<name> -L
nix flake check -L
direnv allow                    # if direnv is in use
```
