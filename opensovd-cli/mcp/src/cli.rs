// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line interface definitions.

use std::path::PathBuf;

use clap::Parser;

pub const ABOUT: &str = env!("CARGO_PKG_DESCRIPTION");

const VERSION_STRING: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("COMMIT_SHA"),
    " ",
    env!("BUILD_DATE"),
    ")"
);

#[derive(Parser)]
#[command(name = "opensovd-mcp")]
#[command(version = VERSION_STRING)]
#[command(about = ABOUT)]
pub struct Cli {
    /// Path to log file.
    #[arg(long, default_value = "mcp.log")]
    pub log: PathBuf,

    /// SOVD server URL (including version prefix, e.g. `http://localhost:7690/sovd/v1`).
    #[arg(long, default_value = "http://localhost:7690/sovd/v1")]
    pub url: String,
}
