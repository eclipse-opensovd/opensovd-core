// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

use time::OffsetDateTime;

fn main() {
    // Set COMMIT_SHA from git describe
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    let sha = Command::new("git")
        .args(["describe", "--dirty", "--always"])
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=COMMIT_SHA={sha}");

    // Set BUILD_DATE from SOURCE_DATE_EPOCH or git commit timestamp
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let timestamp = std::env::var("SOURCE_DATE_EPOCH").ok().or_else(|| {
        Command::new("git")
            .args(["log", "-1", "--pretty=%ct"])
            .output()
            .ok()
            .and_then(|output| {
                output
                    .status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
            })
    });

    let build_date = if let Some(ts) = timestamp
        && let Ok(epoch) = ts.parse::<i64>()
        && let Ok(dt) = OffsetDateTime::from_unix_timestamp(epoch)
    {
        let format =
            time::macros::format_description!("[year]-[month padding:zero]-[day padding:zero]");
        dt.format(&format).unwrap_or_else(|_| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    println!("cargo:rustc-env=BUILD_DATE={build_date}");
}
