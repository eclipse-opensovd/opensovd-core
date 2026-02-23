// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use tower_http::services::ServeDir;

pub fn create_serve_dir(directory: impl AsRef<Path>) -> ServeDir {
    ServeDir::new(directory)
}
