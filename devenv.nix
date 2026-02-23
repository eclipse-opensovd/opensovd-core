# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{ pkgs, lib, inputs, ... }:

let
  rustToolchain = inputs.fenix.packages.${pkgs.system}.fromToolchainFile {
    file = ./rust-toolchain.toml;
    sha256 = "sha256-yV19PoLACCNwhlCWATLcINR1/usvRJW+1be7P1yqwNw=";
  };
  unstable = inputs.nixpkgs-unstable.legacyPackages.${pkgs.system};
in
{
  # Environment name
  name = "opensovd-core";

  # Use Cachix binary caches for faster builds
  cachix.enable = true;
  cachix.pull = [ "nix-community" ];

  # Python for integration tests (requires 3.14+)
  languages.python = {
    enable = true;
    version = "3.14";
    uv = {
      enable = true;
      sync.enable = true;
    };
  };

  # Packages
  packages = with pkgs; [
    # Rust toolchain
    rustToolchain

    # Rust tools
    cargo-deny      # License and security auditing
    cargo-llvm-cov  # Code coverage
    git-cliff       # Changelog generation

    # General tools
    git
    docker
    shellcheck
    markdownlint-cli
    yamlfmt
    gitleaks
    unstable.fish
    unstable.prek   # Pre-commit alternative (Rust)
    curl
    jq
    gh              # GitHub CLI
    nodejs          # Node.js runtime (Bruno CLI)
    bruno-cli       # Bruno API testing CLI
  ];

   # Environment variables
  env = {
    RUST_BACKTRACE = "1";
  };

  # Shell hook - runs when entering the environment
  enterShell = ''
    echo "OpenSOVD Core Development Environment"
    echo "  Rust:   $(rustc --version)"
    echo "  Python: $(python --version)"
    echo "  uv:     $(uv --version)"
    echo ""
    echo "Common commands:"
    echo "  cargo build          - Build the project"
    echo "  cargo test           - Run Rust tests"
    echo "  uv run pytest        - Run Python integration tests"
    echo "  devenv test          - Run pre-commit hooks"
  '';
}
