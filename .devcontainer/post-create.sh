#!/usr/bin/env bash
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
set -euo pipefail

# The nix feature installs into the default profile, not ~/.nix-profile.
nix_direnv=/nix/var/nix/profiles/default/share/nix-direnv/direnvrc
test -f "$nix_direnv"

mkdir -p "$HOME/.config/direnv"
echo "source $nix_direnv" > "$HOME/.config/direnv/direnvrc"

cat >> "$HOME/.bashrc" <<'EOF'
eval "$(direnv hook bash)"
EOF

direnv allow

# Exercises the direnv path just written, warms the store and creates the venv.
direnv exec . uv sync --locked
