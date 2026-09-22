# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

{ inputs, ... }:
{
  imports = [ inputs.flake-parts.flakeModules.partitions ];

  partitionedAttrs = {
    devShells = "dev";
    formatter = "dev";
    checks = "dev";
  };

  partitions.dev = {
    extraInputsFlake = ../dev;
    module =
      { inputs, ... }:
      {
        imports = [
          inputs.git-hooks.flakeModule
          ./devshell.nix
        ];
      };
  };
}
