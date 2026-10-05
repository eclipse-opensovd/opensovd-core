#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Render a Markdown coverage report from a cargo-llvm-cov JSON report.
#
# Usage (run from anywhere):
#   bash scripts/coverage-report.sh [--detail | --badge] [--name NAME] [JSON] [WORKSPACE_ROOT]
#
#   --detail   also append a full per-file table sorted by path
#   --badge    print a shields.io endpoint badge instead
#   --name     name the report, e.g. after the binary it covers

set -euo pipefail

mode=summary
name=
while [ $# -gt 0 ]; do
  case "$1" in
    --detail) mode=detail; shift ;;
    --badge) mode=badge; shift ;;
    --name) name=$2; shift 2 ;;
    --*)
      echo "unknown option: $1" >&2
      exit 2
      ;;
    *) break ;;
  esac
done

json="${1:-coverage.json}"
root="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
root="${root%/}/"

if [ "$mode" = badge ]; then
  jq '(.data[0].totals.lines.percent | floor) as $p
    | {schemaVersion: 1, label: "coverage", message: "\($p)%",
       color: (if $p >= 80 then "28A745" elif $p >= 60 then "B08800" else "CB2431" end)}' "$json"
  exit 0
fi

total=$(jq -r '.data[0].totals.lines.percent' "$json")
covered=$(jq -r '.data[0].totals.lines.covered' "$json")
count=$(jq -r '.data[0].totals.lines.count' "$json")

printf '<!-- coverage-report%s -->\n' "${name:+:$name}"
printf '## Coverage%s: %.2f%%\n\n' "${name:+ of $name}" "$total"
printf '%s of %s lines covered (Rust unit tests + e2e).\n\n' "$covered" "$count"
printf '| Crate | Lines | Coverage |\n'
printf '| --- | --- | --- |\n'
jq -r --arg root "$root" '
  .data[0].files
  | map(select(.filename | startswith($root)))
  | map(.filename |= ltrimstr($root))
  | group_by(.filename | split("/")[0])
  | map({
      crate: (.[0].filename | split("/")[0]),
      covered: (map(.summary.lines.covered) | add),
      count: (map(.summary.lines.count) | add)
    })
  | sort_by(.crate)
  | .[]
  | "| `\(.crate)` | \(.covered)/\(.count) | \(if .count > 0 then (.covered * 10000 / .count | floor) / 100 else 0 end)% |"
' "$json"

if [ "$mode" = detail ]; then
  printf '\n'
  printf '| File | Lines | Coverage |\n'
  printf '| --- | --- | --- |\n'
  jq -r --arg root "$root" '
    def pct(c; n): if n > 0 then (c * 10000 / n | floor) / 100 else 0 end;
    .data[0].files
    | map(select(.filename | startswith($root)))
    | map(.filename |= ltrimstr($root))
    | sort_by(.filename)
    | .[]
    | "| `\(.filename)` | \(.summary.lines.covered)/\(.summary.lines.count) | \(pct(.summary.lines.covered; .summary.lines.count))% |"
  ' "$json"
fi
