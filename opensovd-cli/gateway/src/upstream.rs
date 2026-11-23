// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Configure the gateway's upstreams and the routes each receives.

use opensovd_extra::{ForwardRoute, Upstream, UpstreamProxy};

use crate::cli::{Endpoint, UpstreamConfig, UpstreamUrl};

/// The upstreams of `config`. All serve their entities and take part in vehicle
/// locks. Only the first takes what has a single answer. `/authorize` always
/// stays with the gateway.
///
/// # Errors
///
/// Returns an error if a URL has no host.
pub fn configured(config: &UpstreamConfig) -> anyhow::Result<Vec<Upstream>> {
    config
        .upstreams
        .iter()
        .enumerate()
        .map(|(index, upstream)| {
            let UpstreamUrl { endpoint } = upstream;
            let routes = routes(index == 0);
            tracing::debug!(target: "forwarding", ?routes, "{endpoint}");
            let proxy = match endpoint {
                Endpoint::Tcp(url) => UpstreamProxy::new(url, config.connect_timeout)?,
                #[cfg(unix)]
                Endpoint::UnixSocket { socket, base } => {
                    UpstreamProxy::unix_socket(socket, base, config.connect_timeout)
                }
            };
            Ok(Upstream::new(proxy.into_service())
                .routes(routes)
                .discovery_timeout(config.discovery_timeout))
        })
        .collect()
}

/// The routes of an upstream: the `first` takes what has a single answer.
fn routes(first: bool) -> Vec<ForwardRoute> {
    let mut routes = vec![ForwardRoute::Entities, ForwardRoute::Locks];
    if first {
        routes.extend([
            ForwardRoute::Functions,
            ForwardRoute::Collections,
            ForwardRoute::CustomCollections,
        ]);
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_upstreams_take_locks() {
        assert!(routes(true).contains(&ForwardRoute::Locks));
        assert_eq!(routes(false), [ForwardRoute::Entities, ForwardRoute::Locks]);
    }

    #[test]
    fn authorize_stays_with_the_gateway() {
        assert!(!routes(true).contains(&ForwardRoute::Authorize));
        assert!(!routes(false).contains(&ForwardRoute::Authorize));
    }

    #[test]
    fn only_the_first_upstream_takes_routes_with_a_single_answer() {
        assert_eq!(
            routes(true),
            [
                ForwardRoute::Entities,
                ForwardRoute::Locks,
                ForwardRoute::Functions,
                ForwardRoute::Collections,
                ForwardRoute::CustomCollections,
            ]
        );
    }
}
