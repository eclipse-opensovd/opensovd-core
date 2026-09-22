# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{
  config,
  lib,
  pkgs,
  ...
}:

let
  rustToolchain = config.opensovd.rustToolchain;
  withRust = rustToolchain != null;
in
{
  options.opensovd.rustToolchain = lib.mkOption {
    type = lib.types.nullOr lib.types.package;
    default = null;
    description = ''
      Toolchain backing the rustfmt and clippy hooks. Leave null to take cargo,
      rustfmt and clippy from nixpkgs instead of a pinned toolchain.
    '';
  };

  config = {
    # This overrides the flake-parts module's `mkDefault pkgs.pre-commit` but
    # still loses to a plain definition in the importing flake.
    package = lib.mkOverride 900 pkgs.prek;

    hooks = {
      check-yaml.enable = true;
      check-toml.enable = true;
      check-json.enable = true;
      check-merge-conflicts.enable = true;
      end-of-file-fixer.enable = true;
      trim-trailing-whitespace.enable = true;
      mixed-line-endings.enable = true;
      yamlfmt.enable = true;
      markdownlint.enable = true;
      shellcheck.enable = true;
      ruff.enable = true;
      ruff-format.enable = true;
      convco.enable = true;

      rustfmt = {
        enable = true;
        settings.check = true;
      }
      // lib.optionalAttrs withRust {
        packageOverrides = {
          cargo = rustToolchain;
          rustfmt = rustToolchain;
        };
      };

      clippy = {
        enable = true;
        settings = {
          offline = false;
          allFeatures = true;
          denyWarnings = true;
          allowedLints = [ "clippy::doc_markdown" ];
          extraArgs = "--locked --workspace --all-targets --no-deps";
        };
      }
      // lib.optionalAttrs withRust {
        packageOverrides = {
          cargo = rustToolchain;
          clippy = rustToolchain;
        };
      };

      cargo-machete = {
        enable = true;
        name = "cargo machete";
        entry = "${pkgs.cargo-machete}/bin/cargo-machete";
        extraPackages = [ pkgs.cargo-machete ];
        types_or = [
          "cargo"
          "cargo-lock"
        ];
        pass_filenames = false;
      };

      gitleaks = {
        enable = true;
        name = "gitleaks";
        entry = "${pkgs.gitleaks}/bin/gitleaks git --pre-commit --redact --staged --verbose";
        extraPackages = [ pkgs.gitleaks ];
        pass_filenames = false;
      };

      ty = {
        enable = true;
        name = "ty check";
        entry = "${pkgs.uv}/bin/uv run --locked -- ty check";
        extraPackages = [
          pkgs.uv
          pkgs.ty
        ];
        types = [ "python" ];
        pass_filenames = false;
      };
    };
  };
}
