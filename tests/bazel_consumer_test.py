# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Check that a downstream crate_index can coexist with OpenSOVD's."""

import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="opensovd-bazel-consumer-") as temp_dir:
        workspace = Path(temp_dir)
        (workspace / ".bazelversion").write_text((ROOT / ".bazelversion").read_text())
        (workspace / "BUILD.bazel").write_text("")
        (workspace / "MODULE.bazel").write_text(
            "\n".join(
                [
                    'module(name = "opensovd_consumer_test")',
                    'bazel_dep(name = "opensovd_core", version = "0.0.0")',
                    'bazel_dep(name = "rules_rust", version = "0.70.0")',
                    'local_path_override(module_name = "opensovd_core", '
                    f"path = {json.dumps(str(ROOT))})",
                    'crate = use_extension("@rules_rust//crate_universe:extensions.bzl", "crate")',
                    'crate.spec(package = "itoa", version = "=1.0.15")',
                    'crate.from_specs(name = "crate_index")',
                    'use_repo(crate, "crate_index")',
                    "",
                ]
            )
        )
        subprocess.run(
            [
                "bazel",
                "--batch",
                "--ignore_all_rc_files",
                "query",
                "--registry=https://raw.githubusercontent.com/eclipse-score/bazel_registry/main/",
                "--registry=https://bcr.bazel.build",
                "--lockfile_mode=off",
                "set(@crate_index//:itoa @opensovd_core//:tokio)",
            ],
            cwd=workspace,
            check=True,
        )


if __name__ == "__main__":
    main()
