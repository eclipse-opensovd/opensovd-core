# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{
  description = "Development-only inputs. Used by the dev partition of the top level flake, so they stay out of consumers' lock files.";

  inputs = {
    git-hooks.url = "github:cachix/git-hooks.nix";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  # This flake exists only for its inputs.
  outputs = { ... }: { };
}
