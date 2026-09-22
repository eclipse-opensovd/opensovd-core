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
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      git-hooks,
      ...
    }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      rustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;

      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      rustToolchainFor = system: (pkgsFor system).rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      hooksFor =
        system:
        git-hooks.lib.${system}.run {
          src = ./.;
          imports = [ ./nix/git-hooks.nix ];
          opensovd.rustToolchain = rustToolchainFor system;
        };
    in
    {
      # `nix fmt`
      formatter = forAllSystems (system: (pkgsFor system).nixfmt);

      # The hook set, for other flakes to import:
      #   run { src = ./.; imports = [ opensovd-core.gitHooksModules.default ]; }
      gitHooksModules.default = ./nix/git-hooks.nix;

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          hooks = hooksFor system;

          rustToolchain = rustToolchainFor system;
        in
        {
          default = pkgs.mkShell {
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
                python313
                uv
                prek # the hook runner itself

                # General tools
                git
                curl
                jq
                gh # GitHub CLI
                bruno-cli # Bruno API testing CLI (bundles its own Node.js)
              ]
              ++ hooks.enabledPackages;

            RUST_BACKTRACE = "1";
            UV_PYTHON = "${pkgs.python313}/bin/python3";
            UV_PYTHON_DOWNLOADS = "never";

            # Interactive `nix develop` only. The banner would otherwise land on the
            # stdout of `nix develop --command`, corrupting anything parsed from it.
            shellHook = ''
              ${hooks.shellHook}
              case "$-" in
                *i*)
                  echo "OpenSOVD Core Development Environment"
                  echo "  Rust:   ${rustChannel}"
                  echo "  Python: ${pkgs.python313.version}"
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
        }
      );
    };
}
