// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Topology benchmarks.

// Benchmark code uses expect for setup/readability in non-production paths.
#![allow(clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use opensovd_core::{Component, DataProvider, EntityCollection, Topology};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder};

const COMPONENT_COUNT: usize = 10_000;
const LOOKUP_COMPONENT_ID: &str = "c-5000";

fn make_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn build_topology(rt: &tokio::runtime::Runtime) -> Topology {
    let entities = EntityCollection {
        components: (0..COMPONENT_COUNT)
            .map(|index| Component::new(format!("c-{index}"), format!("Component {index}")))
            .collect(),
        ..EntityCollection::default()
    };

    let topology = Topology::new();
    rt.block_on(async {
        let mut t = topology.write().await;
        for c in entities.components {
            t.add_component(c);
        }
    });
    topology
}

fn build_topology_with_provider(rt: &tokio::runtime::Runtime) -> Topology {
    let provider = DataProviderBuilder::new()
        .read_data(
            "voltage",
            "Battery Voltage",
            &DataCategory::CurrentData,
            Constant::new(12.6).expect("constant"),
        )
        .build()
        .expect("build provider");

    let component = Component::new("ecu", "ECU").with_data_provider(provider);

    let topology = Topology::new();
    rt.block_on(async {
        topology.write().await.add_component(component);
    });
    topology
}

fn build_provider() -> Box<dyn DataProvider> {
    Box::new(
        DataProviderBuilder::new()
            .read_data(
                "voltage",
                "Battery Voltage",
                &DataCategory::CurrentData,
                Constant::new(12.6).expect("constant"),
            )
            .build()
            .expect("build provider"),
    )
}

fn bench_get_component(c: &mut Criterion) {
    let rt = make_runtime();
    let topology = build_topology(&rt);

    c.bench_function("topology/get_component", |b| {
        b.to_async(&rt).iter(|| async {
            let state = topology.read().await;
            std::hint::black_box(
                state
                    .get_component(LOOKUP_COMPONENT_ID)
                    .expect("benchmark target component missing"),
            );
        });
    });
}

fn bench_provider_read(c: &mut Criterion) {
    let rt = make_runtime();
    let topology = build_topology_with_provider(&rt);

    c.bench_function("topology/provider_read", |b| {
        b.to_async(&rt).iter(|| async {
            let topo = topology.read().await;
            let entity = topo.get_component("ecu").expect("ecu");
            let provider = entity.data_provider().expect("provider");
            std::hint::black_box(provider.read("voltage", false).await.expect("read"));
        });
    });
}

fn bench_data_read(c: &mut Criterion) {
    let provider = build_provider();
    let rt = make_runtime();

    c.bench_function("topology/data_read", |b| {
        b.to_async(&rt).iter(|| async {
            std::hint::black_box(provider.read("voltage", false).await.expect("read"));
        });
    });
}

criterion_group!(
    topology_benches,
    bench_get_component,
    bench_provider_read,
    bench_data_read
);
criterion_main!(topology_benches);
