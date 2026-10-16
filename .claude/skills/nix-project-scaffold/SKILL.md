---
name: nix-project-scaffold
description: "House-style scaffold for new Nix-flake projects (Go, Python, or other languages) — numtide/blueprint with prefix=\"nix\", direnv .envrc, numtide devshell with in-repo toolchain caches, treefmt formatter, two-file package (default.nix shim + nixpkgs-style package.nix with src=flake), justfile facade, and GitHub Actions CI pinned to flake.lock's nixpkgs. Apply when creating a new project or repository that should use Nix; when adding Nix/flake tooling to an existing Go or Python repo; when asked for a devshell + formatter + CI setup 'like my other repos'; when wiring justfile targets to nix commands; when setting up direnv for a flake. Do NOT use for repos that already follow a different flake convention (flake-parts, devenv, hand-rolled outputs), for NixOS system configuration repos, or for nixpkgs contributions."
user-invocable: true
license: MIT
compatibility: Designed for Claude Code, Codex or similar harness. Assumes nix with flakes enabled and git.
metadata:
  author: aldoborrero
  version: "1.0.0"
  openclaw:
    emoji: "🏗"
    homepage: https://github.com/aldoborrero/cc-skills-nix
    requires:
      bins:
        - nix
        - git
    install: []
allowed-tools: Read Edit Write Glob Grep Bash(nix:*) Bash(git:*) Bash(just:*) Agent
paths:
  - "**/flake.nix"
  - "**/justfile"
  - "**/.envrc"
---

# New Nix project, house style

Reference implementations: `aldoborrero/cfleet` (Go) and `aldoborrero/reolink-exporter` (Python). Their tooling files are intentionally identical except for the language-specific blocks — keep new projects identical too, don't improvise. All file contents in `references/templates.md`; CI and justfile in `references/ci-and-justfile.md`.

→ Directory semantics and failure modes of blueprint: see the `nix-blueprint` skill. → formatter.nix details: see the `nix-treefmt-formatter` skill.

## Layout

```
flake.nix            # 4 fixed inputs, 5-line outputs (below)
flake.lock
.envrc               # use flake + source_env_if_exists .envrc.local
.gitignore           # result*, .direnv/, .envrc.local, language caches
justfile             # facade: every dev command, delegating to nix/uv/go
nix/
  devshell.nix       # numtide devshell: just + formatter + toolchain
  formatter.nix      # treefmt wrapper + hermetic format-check
  packages/<name>/
    default.nix      # { flake, pkgs, ... }: pkgs.callPackage ./package.nix { src = flake; }
    package.nix      # nixpkgs-style derivation, upstreamable
src/ | cmd/ + internal/   # the actual code, language-idiomatic
```

## flake.nix — always these four inputs

```nix
{
  description = "<one line, reused as the package meta.description>";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixos-unstable";

    blueprint.url = "github:numtide/blueprint";
    blueprint.inputs.nixpkgs.follows = "nixpkgs";

    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";

    devshell.url = "github:numtide/devshell";
    devshell.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs: inputs.blueprint {
    inherit inputs;
    prefix = "nix";
  };
}
```

Every non-nixpkgs input gets `inputs.nixpkgs.follows = "nixpkgs"` — one nixpkgs in the closure. New outputs are never added here; they are files under `nix/` (blueprint discovers them).

## Order of operations (a scaffold that skips these fails silently)

1. `git init` first — flake evaluation in a git repo only sees **tracked** files.
2. Write all files, then `git add -A` **before** any nix command.
3. `nix flake lock`, then `nix flake show` to confirm every expected output exists.
4. `nix fmt` and re-add — the formatter check will fail CI on an unformatted tree.
5. `nix develop` (or `direnv allow`) to smoke-test the shell; `nix build .#<name>` for the package.

## House rules (the why behind the templates)

- **Toolchain caches live in the repo**, not `$HOME`: `GOPATH=$PRJ_ROOT/.go`, `UV_PROJECT_ENVIRONMENT=$PRJ_ROOT/.venv` — set as devshell `env` entries, matching `.gitignore`.
- **`NIX_PATH = "nixpkgs=${toString pkgs.path}"`** in the devshell, so `nix-shell -p` and legacy tools resolve the same nixpkgs the flake pins.
- **Devshell always contains `just` and `perSystem.self.formatter`**; language toolchain and domain tools each get a comment stating *why* they're there.
- **Two-file package**: `default.nix` bridges blueprint's scope; `package.nix` never mentions `flake`/`perSystem` and would drop into nixpkgs unchanged. `meta` is complete: description, homepage, license, `sourceProvenance`, maintainers, `mainProgram`, `platforms`.
- **Tests run inside the build** (`nativeCheckInputs` + pytest hook, or `buildGoModule`'s default `go test`), so `nix flake check` is the single CI gate — no separate test job.
- **justfile is a thin facade**, one recipe per dev action, delegating to `nix`/`uv`/`go`; `default:` lists recipes. Long nix commands get `--log-format bar-with-logs` locally and `--print-build-logs` in CI.
- **Dual dependency resolution (Python)**: uv drives the local venv, the Nix build resolves the same deps from nixpkgs. Both must stay compatible — document the currently-carried nixpkgs version of critical deps in CLAUDE.md/AGENTS.md.
- **CI installs Nix with `cachix/install-nix-action` pinned by commit SHA** and derives `nix_path` from `flake.lock` (`jq -r '.nodes.nixpkgs.locked.rev'`) — CI uses exactly the pinned nixpkgs, no channels.
- **Repo docs**: `AGENTS.md` with `CLAUDE.md` as a symlink to it (layout table, commands table, invariants in prose).
