// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Configure the gateway's upstreams and the routes each receives.

use opensovd_extra::{ForwardRoute, Upstream, UpstreamProxy};

use crate::cli::{UpstreamConfig, UpstreamUrl};

/// The upstreams of `config`. All serve their entities and take part in vehicle
/// locks. Only the first takes what has a single answer, but for authorize,
/// which stays with the gateway unless `config` names its upstream.
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
            let UpstreamUrl { url, authorize } = upstream;
            let routes = routes(index == 0, *authorize);
            tracing::debug!(target: "forwarding", ?routes, "{url}");
            let proxy = UpstreamProxy::new(url, config.connect_timeout)?;
            Ok(Upstream::new(proxy.into_service())
                .routes(routes)
                .discovery_timeout(config.discovery_timeout))
        })
        .collect()
}

/// The routes of an upstream: the `first` takes what has a single answer, and
/// `authorize` says whether it takes `/authorize`.
fn routes(first: bool, authorize: bool) -> Vec<ForwardRoute> {
    let mut routes = vec![ForwardRoute::Entities, ForwardRoute::Locks];
    if first {
        routes.extend([
            ForwardRoute::Functions,
            ForwardRoute::Collections,
            ForwardRoute::CustomCollections,
        ]);
    }
    if authorize {
        routes.push(ForwardRoute::Authorize);
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_upstreams_take_locks() {
        assert!(routes(true, false).contains(&ForwardRoute::Locks));
        assert_eq!(
            routes(false, false),
            [ForwardRoute::Entities, ForwardRoute::Locks]
        );
    }

    #[test]
    fn authorize_stays_with_the_gateway_unless_an_upstream_is_named() {
        assert!(!routes(true, false).contains(&ForwardRoute::Authorize));
        assert!(routes(false, true).contains(&ForwardRoute::Authorize));
    }

    #[test]
    fn only_the_first_upstream_takes_routes_with_a_single_answer() {
        assert_eq!(
            routes(true, false),
            [
                ForwardRoute::Entities,
                ForwardRoute::Locks,
                ForwardRoute::Functions,
                ForwardRoute::Collections,
                ForwardRoute::CustomCollections,
            ]
        );
        assert_eq!(
            routes(false, true),
            [
                ForwardRoute::Entities,
                ForwardRoute::Locks,
                ForwardRoute::Authorize
            ]
        );
    }
}
