#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

BASE_URL="${BULKDATA_BASE_URL:-http://127.0.0.1:7690/sovd/v1/apps/bulkdata-example/bulk-data}"

usage() {
	cat <<'EOF'
Usage:
  bulkdata.sh categories
  bulkdata.sh files <category>
  bulkdata.sh upload <category> <local-file> [remote-name]
  bulkdata.sh download <category> <file-id> [output-path]
  bulkdata.sh delete-file <category> <file-id>
  bulkdata.sh delete-category <category>

Environment:
  BULKDATA_BASE_URL  Override the default bulkdata endpoint.

Examples:
  bulkdata.sh categories
  bulkdata.sh files logs
  bulkdata.sh upload logs ./demo.bin
  bulkdata.sh download logs demo.bin ./downloaded-demo.bin
  bulkdata.sh delete-file logs demo.bin
  bulkdata.sh delete-category logs
EOF
}

die() {
	printf 'error: %s\n' "$1" >&2
	exit 1
}

need_args() {
	local expected="$1"
	local actual="$2"
	if [[ "$actual" -lt "$expected" ]]; then
		usage
		exit 1
	fi
}

curl_json() {
	curl --fail --silent --show-error "$@"
	printf '\n'
}

command="${1:-}"

if [[ -z "$command" ]]; then
	usage
	exit 1
fi

case "$command" in
	categories|list-categories)
		curl_json "$BASE_URL"
		;;

	files|list-files)
		need_args 2 "$#"
		category="$2"
		curl_json "$BASE_URL/$category"
		;;

	upload)
		need_args 3 "$#"
		category="$2"
		local_file="$3"
		remote_name="${4:-$(basename "$local_file")}"

		[[ -f "$local_file" ]] || die "file not found: $local_file"

		curl_json \
			-X POST \
			-H "Content-Disposition: attachment; filename=\"$remote_name\"" \
			-F "file=@${local_file};type=application/octet-stream" \
			"$BASE_URL/$category"
		;;

	download)
		need_args 3 "$#"
		category="$2"
		file_id="$3"
		output_path="${4:-$file_id}"

		curl --fail --silent --show-error \
			"$BASE_URL/$category/$file_id" \
			-o "$output_path"
		printf 'saved %s\n' "$output_path"
		;;

	delete-file)
		need_args 3 "$#"
		category="$2"
		file_id="$3"
		curl --fail --silent --show-error \
			-X DELETE \
			"$BASE_URL/$category/$file_id"
		printf 'deleted %s/%s\n' "$category" "$file_id"
		;;

	delete-category)
		need_args 2 "$#"
		category="$2"
		curl --fail --silent --show-error \
			-X DELETE \
			"$BASE_URL/$category"
		printf 'deleted category %s\n' "$category"
		;;

	-h|--help|help)
		usage
		;;

	*)
		die "unknown command: $command"
		;;
esac
