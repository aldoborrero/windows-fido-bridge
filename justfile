# windows-fido-bridge

# Upstream C++ implementation the rewrite follows (socram8888 fork, SK API v10).
upstream_url := "https://github.com/socram8888/windows-fido-bridge"
upstream_rev := "34dd39a6af4b90479fd8c2841aa0209cc3e32787"

default:
    @just --list

# Run the test suite
test *args:
    cargo test --workspace {{ args }}

# Lint
lint:
    cargo clippy --workspace --all-targets -- --deny warnings

# Format everything (nix, rust, yaml, toml, markdown, shell)
fmt:
    nix fmt

# Every check CI runs
check:
    nix flake check --log-format bar-with-logs

# Build the middleware library and the Windows executable into ./result/lib
build:
    nix build .#windows-fido-bridge --log-format bar-with-logs

# Build the Windows executable with cargo (devshell provides the mingw linker)
build-exe:
    cargo build --package webauthn-bridge --target x86_64-pc-windows-gnu

# Sign a test message through the bridge; the key must be attached to Windows, not WSL
try-sign key="~/.ssh/id_ed25519_sk_5c-github": build
    mkdir -p target && echo hola > target/msg && rm -f target/msg.sig
    SSH_AUTH_SOCK= ssh-keygen -w "$PWD/result/lib/libwindowsfidobridge.so" -Y sign -n git -f {{ key }} target/msg
    ssh-keygen -Y check-novalidate -n git -f {{ key }}.pub -s target/msg.sig < target/msg

# Clone the upstream C++ implementation into .scratch/ for reference
upstream:
    test -d .scratch/windows-fido-bridge-cpp || git clone --quiet {{ upstream_url }} .scratch/windows-fido-bridge-cpp
    git -C .scratch/windows-fido-bridge-cpp checkout --quiet {{ upstream_rev }}

# Create a worktree for a feature branch under .worktrees/
worktree branch:
    git worktree add .worktrees/{{ branch }} -b {{ branch }}
