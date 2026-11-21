// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Log providers backed by system logging services.

mod journalctl;

pub use journalctl::JournalctlLogProvider;
