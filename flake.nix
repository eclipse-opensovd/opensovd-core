# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{
  description = "OpenSOVD Core development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      rustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
    in
    {
      # `nix fmt`
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            name = "opensovd-core";

            packages = with pkgs; [
              # Rust toolchain (from rust-toolchain.toml)
              rustToolchain
              cargo-deny # license and security auditing
              cargo-llvm-cov # code coverage
              cargo-machete # unused dependency detection
              git-cliff # changelog generation

              # Python integration tests
              python313
              uv

              # General tools
              git
              shellcheck
              markdownlint-cli
              yamlfmt
              gitleaks
              go # prek builds the gitleaks hook; without it prek fetches its own
              curl
              jq
              gh # GitHub CLI
              bruno-cli # Bruno API testing CLI (bundles its own Node.js)
            ];

            RUST_BACKTRACE = "1";
            UV_PYTHON = "${pkgs.python313}/bin/python3";
            UV_PYTHON_DOWNLOADS = "never";

            # Interactive `nix develop` only. The banner would otherwise land on the
            # stdout of `nix develop --command`, corrupting anything parsed from it.
            shellHook = ''
              case "$-" in
                *i*)
                  echo "OpenSOVD Core Development Environment"
                  echo "  Rust:   ${rustChannel}"
                  echo "  Python: ${pkgs.python313.version}"
                  echo "  uv:     ${pkgs.uv.version}"
                  echo ""
                  echo "Common commands:"
                  echo "  uv sync                          - Sync Python integration-test dependencies"
                  echo "  cargo build                      - Build the project"
                  echo "  cargo test                       - Run Rust tests"
                  echo "  uv run pytest                    - Run Python integration tests"
                  echo "  uv run --group tools prek run -a - Run pre-commit hooks"

                  command -v fish >/dev/null && exec fish
                  ;;
              esac
            '';
          };
        }
      );
    };
}
