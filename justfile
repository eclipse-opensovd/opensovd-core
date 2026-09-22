# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

set positional-arguments
set default-list

set shell := ['bash', '-cu']
set script-interpreter := ['bash', '-euo', 'pipefail']

profile := "dev"
target := ""

# Empty means no flag, leaving cargo on the host and pytest on the dev profile.
_target := if target == "" { "" } else { "--target " + target }
_pyprofile := if profile == "dev" { "" } else { "--opensovd-profile=" + profile }
_pytarget := if target == "" { "" } else { "--opensovd-target=" + target }

# Build the workspace. Pass profile=release-small or target=<triple> to override.
build *args:
    cargo build --locked --profile {{ profile }} {{ _target }} "$@"

# Run tests: all (default), rust, e2e, harness, bruno
[script]
test suite='all' *args:
    shift || true
    py=(uv run --locked pytest {{ _pyprofile }} {{ _pytarget }})
    case '{{ suite }}' in
      rust)    cargo test --locked --all-features "$@" ;;
      e2e)     "${py[@]}" tests/ "$@" ;;
      harness) "${py[@]}" opensovd-e2e/tests "$@" ;;
      bruno)   "${py[@]}" tests/bruno "$@" ;;
      all)     cargo test --locked --all-features
               "${py[@]}" opensovd-e2e/tests
               "${py[@]}" tests/ ;;
      *) echo "unknown suite '{{ suite }}' (all|rust|e2e|harness|bruno)" >&2; exit 2 ;;
    esac

# Merged Rust + e2e coverage (coverage.json, HTML, Cobertura)
coverage *args:
    uv run --locked pytest tests/ --opensovd-coverage "$@"

# Run every git hook across the whole tree
lint *args:
    prek run --all-files "$@"

# Apply rustfmt. The hook only checks it.
fmt:
    cargo fmt --all

# Gateway with built-in mock entities on :7690
run *args:
    cargo run --locked --profile {{ profile }} -p opensovd-gateway -- --mock "$@"

# Sync the Python test virtualenv
sync:
    uv sync --locked

# These are CI gates and stay out of `just --list`.

[private]
deny:
    cargo deny check licenses sources
    cargo deny check advisories

[private]
flake-check:
    nix flake check --all-systems

[private]
certs dir='':
    bash scripts/mkcerts.sh {{ dir }}
