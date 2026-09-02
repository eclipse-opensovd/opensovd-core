// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Build-time version stamping shared by the OpenSOVD binaries.

// Emitting directives on stdout is how a build script talks to cargo.
#![allow(clippy::print_stdout)]

use std::{path::Path, process::Command};

use time::OffsetDateTime;

/// Emits `VERSION`, `RELEASE_CHANNEL`, `COMMIT_SHA` and `BUILD_DATE` for the
/// calling crate.
///
/// The revision and date are read from the environment first so builds without
/// a git checkout (source tarballs, vendored trees) can still be stamped:
/// `OPENSOVD_COMMIT_SHA` for the revision, `SOURCE_DATE_EPOCH` for the date.
/// The release channel comes from `OPENSOVD_CHANNEL`, see [`channel_suffix`].
pub fn emit() {
    emit_version();
    emit_commit_sha();
    emit_build_date();
}

/// Maps a release channel onto its version suffix.
///
/// Only `stable` ships unsuffixed; every other channel is a semver pre-release,
/// which orders `0.1.1-dev` before `0.1.1-nightly` before `0.1.1`.
///
/// # Panics
///
/// Panics on an unrecognised channel, failing the build rather than silently
/// mislabelling a binary.
fn channel_suffix(channel: &str) -> &'static str {
    match channel {
        "stable" => "",
        "nightly" => "-nightly",
        "dev" => "-dev",
        other => panic!("unknown OPENSOVD_CHANNEL `{other}`, expected stable, nightly or dev"),
    }
}

fn emit_version() {
    println!("cargo::rerun-if-env-changed=OPENSOVD_CHANNEL");

    // Anything not built by release infrastructure is a dev build.
    let channel = std::env::var("OPENSOVD_CHANNEL")
        .ok()
        .filter(|channel| !channel.is_empty())
        .unwrap_or_else(|| "dev".to_string());
    let suffix = channel_suffix(&channel);

    // Set by cargo for the crate being built, not for this one.
    let Ok(version) = std::env::var("CARGO_PKG_VERSION") else {
        panic!("CARGO_PKG_VERSION is unset; emit() must be called from a build script")
    };

    println!("cargo::rustc-env=RELEASE_CHANNEL={channel}");
    println!("cargo::rustc-env=VERSION={version}{suffix}");
}

fn emit_commit_sha() {
    println!("cargo::rerun-if-env-changed=OPENSOVD_COMMIT_SHA");

    // Resolve the reflog via git: .git is a file, not a directory, in worktrees
    // and submodules, so a hardcoded ../../.git/logs/HEAD does not exist there.
    // A missing rerun-if-changed path makes cargo rebuild on every invocation.
    if let Some(reflog) = git(["rev-parse", "--git-path", "logs/HEAD"])
        && Path::new(&reflog).exists()
    {
        println!("cargo::rerun-if-changed={reflog}");
    }

    let sha = std::env::var("OPENSOVD_COMMIT_SHA")
        .ok()
        .filter(|sha| !sha.is_empty())
        .or_else(|| git(["describe", "--dirty", "--always"]))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo::rustc-env=COMMIT_SHA={sha}");
}

fn emit_build_date() {
    println!("cargo::rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|ts| ts.parse::<i64>().ok())
        .or_else(|| git(["log", "-1", "--pretty=%ct"]).and_then(|ts| ts.parse::<i64>().ok()));

    let build_date = if let Some(epoch) = epoch
        && let Ok(dt) = OffsetDateTime::from_unix_timestamp(epoch)
    {
        let format =
            time::macros::format_description!("[year]-[month padding:zero]-[day padding:zero]");
        dt.format(&format).unwrap_or_else(|_| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    println!("cargo::rustc-env=BUILD_DATE={build_date}");
}

/// Runs git in the crate being built, returning trimmed stdout on success.
fn git<const N: usize>(args: [&str; N]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .filter(|out| !out.is_empty())
}

#[cfg(test)]
mod tests {
    use super::channel_suffix;

    #[test]
    fn stable_is_the_only_unsuffixed_channel() {
        assert_eq!(channel_suffix("stable"), "");
        assert_eq!(channel_suffix("nightly"), "-nightly");
        assert_eq!(channel_suffix("dev"), "-dev");
    }

    #[test]
    #[should_panic(expected = "unknown OPENSOVD_CHANNEL")]
    fn unknown_channel_fails_the_build() {
        let _ = channel_suffix("beta");
    }
}
