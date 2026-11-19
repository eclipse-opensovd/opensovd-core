// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Gateway server binary.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
#[expect(clippy::print_stderr)]
async fn main() -> ExitCode {
    let cli = opensovd_gateway::cli::parse();

    if let Err(e) = libcli::init_tracing("warn,gw=info,srv=info,mdns=info,tower_http=debug", None) {
        eprintln!("Failed to initialize tracing: {e}");
        return ExitCode::FAILURE;
    }

    if let Err(e) = opensovd_gateway::run_default(cli).await {
        eprintln!("Error: {e:?}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
