# SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
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
