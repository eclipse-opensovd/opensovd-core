# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{
  description = "OpenSOVD Core development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, fenix, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
      ];
      rustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
    in
    {
      # `nix fmt`
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};

          # sha256 covers the resolved rust-toolchain.toml manifest. Regenerate after
          # toolchain or fenix changes: set it to pkgs.lib.fakeHash, run `nix develop`,
          # copy the "got:" hash.
          rustToolchain = fenix.packages.${system}.fromToolchainFile {
            file = ./rust-toolchain.toml;
            sha256 = "sha256-yrnQpNRN/qYf9KiMrN5g01WhjUTobuAcrMupwvYnGos=";
          };
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
              prek # pre-commit alternative
              curl
              jq
              gh # GitHub CLI
              bruno-cli # Bruno API testing CLI (bundles its own Node.js)
            ];

            RUST_BACKTRACE = "1";

            shellHook = ''
              echo "OpenSOVD Core Development Environment"
              echo "  Rust:   ${rustChannel}"
              echo "  Python: ${pkgs.python313.version}"
              echo "  uv:     ${pkgs.uv.version}"
              echo ""
              echo "Common commands:"
              echo "  uv sync              - Sync Python integration-test dependencies"
              echo "  cargo build          - Build the project"
              echo "  cargo test           - Run Rust tests"
              echo "  uv run pytest        - Run Python integration tests"
              echo "  prek run -a          - Run pre-commit hooks"

              # Interactive `nix develop` only. direnv and `nix develop -c` skip this.
              case "$-" in
                *i*) command -v fish >/dev/null && exec fish ;;
              esac
            '';
          };
        }
      );
    };
}
