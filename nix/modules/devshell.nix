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

      # These are what the static-check jobs need and the base of the full shell.
      lintPackages =
        with pkgs;
        [
          rustToolchain
          cargo-deny # license and security auditing
          python
          uv
          prek # the hook runner itself
          git
        ]
        ++ config.pre-commit.settings.enabledPackages;
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

      devShells = {
        default = pkgs.mkShell {
          name = "opensovd-core";

          packages =
            lintPackages
            ++ (with pkgs; [
              cargo-llvm-cov # code coverage
              git-cliff # changelog generation
              curl
              jq
              gh # GitHub CLI
              bruno-cli # Bruno API testing CLI (bundles its own Node.js)
            ]);

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

        # This is what the lint, licenses and advisories jobs enter. Nothing
        # interactive runs here, so it installs no hooks and prints no banner.
        lint = pkgs.mkShell {
          name = "opensovd-core-lint";

          packages = lintPackages;

          UV_PYTHON = "${python}/bin/python3";
          UV_PYTHON_DOWNLOADS = "never";

          shellHook = config.pre-commit.shellHook;
        };
      };
    };
}
