# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Rust target wrappers for the OpenSOVD workspace crates."""

load("@crate_index//:defs.bzl", "aliases", _crate_deps = "crate_deps")
load("@rules_rust//cargo:defs.bzl", "cargo_build_script")
load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")

def workspace_rust_library(
        crate_deps = [],
        proc_macro_crate_deps = [],
        deps = [],
        **kwargs):
    """Defines a Rust 2024 library with explicitly selected Cargo dependencies."""
    rust_library(
        aliases = aliases(),
        edition = "2024",
        deps = deps + _crate_deps(crate_deps),
        proc_macro_deps = _crate_deps(proc_macro_crate_deps),
        **kwargs
    )

def workspace_rust_binary(
        crate_deps = [],
        proc_macro_crate_deps = [],
        deps = [],
        **kwargs):
    """Defines a Rust 2024 binary with explicitly selected Cargo dependencies."""
    rust_binary(
        aliases = aliases(),
        edition = "2024",
        deps = deps + _crate_deps(crate_deps),
        proc_macro_deps = _crate_deps(proc_macro_crate_deps),
        **kwargs
    )

def workspace_rust_test(
        crate_deps = [],
        proc_macro_crate_deps = [],
        deps = [],
        **kwargs):
    """Defines a Rust 2024 test with explicitly selected Cargo dependencies."""
    rust_test(
        aliases = aliases(),
        edition = "2024",
        deps = deps + _crate_deps(crate_deps),
        proc_macro_deps = _crate_deps(proc_macro_crate_deps),
        **kwargs
    )

def workspace_cargo_build_script(
        crate_deps = [],
        proc_macro_crate_deps = [],
        deps = [],
        **kwargs):
    """Defines a Cargo build script with explicitly selected Cargo dependencies."""
    cargo_build_script(
        aliases = aliases(),
        edition = "2024",
        deps = deps + _crate_deps(crate_deps),
        proc_macro_deps = _crate_deps(proc_macro_crate_deps),
        **kwargs
    )
