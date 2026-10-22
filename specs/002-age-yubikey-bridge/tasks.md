---

description: "Task list for age-plugin-yubikey through Windows from WSL"
---

# Tasks: age-plugin-yubikey through Windows from WSL

**Input**: [spec.md](spec.md), [plan.md](plan.md)

**Tests**: `checks.age-shim` (shim passthrough) and `checks.nixos-module` (option); hardware
validation with `just try-age`.

## Phase 1: Build

- [x] T001 Factor the Windows cross-compilation crane arguments into `windowsCrossArgs` in
  `nix/lib/default.nix`, reused by `windowsArgs`
- [x] T002 Read the plugin source at `v0.5.1` and confirm the decrypt path needs no console
  (plan R2)
- [x] T003 Build `age-plugin-yubikey.exe` from source at `v0.5.1` in
  `nix/packages/age-plugin-yubikey-wsl/package.nix`

## Phase 2: User Story 1 - Decrypt through Windows (P1)

- [x] T004 [US1] Shim `nix/packages/age-plugin-yubikey-wsl/shim.sh`, installed as
  `bin/age-plugin-yubikey` with the `.exe` in `lib/` of the same output
- [x] T005 [US1] Sandbox test `nix/checks/age-shim.nix`: argv, stdin, stdout, stderr, exit
  status, `WSLENV`
- [x] T006 [US1] `just try-age` recipe: encrypt to the attached key's recipient and decrypt it
- [ ] T007 [US1] Hardware: run `just try-age` with the key attached to Windows (needs the user's
  PIN/touch); run it twice to observe the PIN cache (plan R5)

## Phase 3: User Story 2 - Modules (P2)

- [x] T008 [US2] `programs.windows-fido-bridge.age.{enable,package}` in
  `nix/modules/nixos/windows-fido-bridge.nix` and `nix/modules/home/windows-fido-bridge.nix`
- [x] T009 [US2] Extend `nix/checks/nixos-module.nix`

## Phase 4: Polish

- [x] T010 README section on age/sops; AGENTS.md layout and invariants
- [x] T011 `just check`, `just lint`, `just fmt`, `just test`
