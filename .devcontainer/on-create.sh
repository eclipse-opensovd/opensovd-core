#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

sudo chown vscode:vscode /mnt/mise-data
mise trust
mise install
eval "$(mise activate bash --shims)"
uv sync --locked
if git rev-parse --git-dir >/dev/null 2>&1; then
  prek install
fi

{
  printf 'OpenSOVD Core dev container\n\nmise run <task>:\n'
  mise tasks ls | sed 's/^/  /'
} | sudo tee /usr/local/etc/vscode-dev-containers/first-run-notice.txt >/dev/null
