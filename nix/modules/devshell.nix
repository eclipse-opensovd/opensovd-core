# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{ inputs, ... }:
{
  perSystem =
    { config, pkgs, ... }:
    let
      rustBin = inputs.rust-overlay.lib.mkRustBin { } pkgs;
      rustToolchain = rustBin.fromRustupToolchainFile ../../rust-toolchain.toml;
      rustChannel = (builtins.fromTOML (builtins.readFile ../../rust-toolchain.toml)).toolchain.channel;

      python = pkgs.python313;
    in
    {
      formatter = pkgs.nixfmt;

      # The cargo and ty hooks need the crates.io registry and a synced
      # virtualenv, neither of which exists in the flake-check sandbox.
      pre-commit.check.enable = false;

      pre-commit.settings = {
        imports = [ ../git-hooks.nix ];
        opensovd.rustToolchain = rustToolchain;
      };

      devShells.default = pkgs.mkShell {
        name = "opensovd-core";

        packages =
          with pkgs;
          [
            # Rust toolchain (from rust-toolchain.toml)
            rustToolchain
            cargo-deny # license and security auditing
            cargo-llvm-cov # code coverage
            git-cliff # changelog generation

            # Python integration tests
            python
            uv
            prek # the hook runner itself

            # General tools
            git
            curl
            jq
            gh # GitHub CLI
            bruno-cli # Bruno API testing CLI (bundles its own Node.js)
          ]
          ++ config.pre-commit.settings.enabledPackages;

        RUST_BACKTRACE = "1";
        UV_PYTHON = "${python}/bin/python3";
        UV_PYTHON_DOWNLOADS = "never";

        # Interactive `nix develop` only. The banner would otherwise land on the
        # stdout of `nix develop --command`, corrupting anything parsed from it.
        shellHook = ''
          ${config.pre-commit.shellHook}
          case "$-" in
            *i*)
              echo "OpenSOVD Core Development Environment"
              echo "  Rust:   ${rustChannel}"
              echo "  Python: ${python.version}"
              echo "  uv:     ${pkgs.uv.version}"
              echo ""
              echo "Common commands:"
              echo "  uv sync       - Sync Python integration-test dependencies"
              echo "  cargo build   - Build the project"
              echo "  cargo test    - Run Rust tests"
              echo "  uv run pytest - Run Python integration tests"
              echo "  prek run -a   - Run pre-commit hooks"

              command -v fish >/dev/null && exec fish
              ;;
          esac
        '';
      };
    };
}
