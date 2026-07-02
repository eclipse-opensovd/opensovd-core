// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_core::Stream;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use opensovd_core::{
    Component, DiscoveryError, DiscoveryProvider, DiscoveryStream, EntityCollection, EntityRef,
};
use tokio::sync::mpsc;

type DiscoveryResult<T> = std::result::Result<T, DiscoveryError>;

// mDNS service type used by SOVD gateways and private-side SOVD services.
pub const SERVICE_TYPE: &str = "_sovd._tcp.local.";

pub const TXT_IDENTIFICATION: &str = "identification";
pub const TXT_ACCESS_URL: &str = "accessurl";

// Errors returned by MdnsWrapper operations.
#[derive(Debug, thiserror::Error)]
pub enum MdnsError {
    #[error("mdns-sd daemon error: {0}")]
    Daemon(#[from] mdns_sd::Error),
    #[error("invalid service info: {0}")]
    ServiceInfo(String),
}

/*
    Thin wrapper around the mdns-sd ServiceDaemon.
    Create one per process and wrap in Arc to share between gateway advertisement
    (MdnsWrapper::register) and private-side discovery (MdnsDiscoveryProvider).
*/
pub struct MdnsWrapper {
    daemon: ServiceDaemon,
    local_fullnames: Mutex<HashSet<String>>,
}

impl MdnsWrapper {
    /// Creates a new mDNS wrapper backed by a fresh `mdns-sd` daemon.
    ///
    /// # Errors
    ///
    /// Returns [`MdnsError::Daemon`] if the underlying mDNS daemon cannot be started.
    pub fn new() -> Result<Self, MdnsError> {
        let daemon = ServiceDaemon::new()?;
        Ok(Self {
            daemon,
            local_fullnames: Mutex::new(HashSet::new()),
        })
    }

    /// Advertises this process as a SOVD mDNS service.
    ///
    /// The gateway uses this to publish its infotainment-facing access URL.
    ///
    /// # Errors
    ///
    /// Returns [`MdnsError::ServiceInfo`] if the service metadata is invalid, or
    /// [`MdnsError::Daemon`] if the daemon rejects the registration.
    pub fn register(
        &self,
        instance_name: &str,
        identification: &str,
        access_url: &str,
        host_ip: IpAddr,
        port: u16,
    ) -> Result<(), MdnsError> {
        let host_name = format!("{instance_name}.local.");
        let txt = [
            (TXT_IDENTIFICATION, identification),
            (TXT_ACCESS_URL, access_url),
        ];
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &host_name,
            host_ip,
            port,
            &txt[..],
        )
        .map_err(|e| MdnsError::ServiceInfo(e.to_string()))?;
        let fullname = service_info.get_fullname().to_string();
        self.daemon.register(service_info)?;
        match self.local_fullnames.lock() {
            Ok(mut local) => {
                local.insert(fullname);
            }
            Err(poisoned) => {
                poisoned.into_inner().insert(fullname);
            }
        }
        tracing::info!(
            target: "mdns",
            service = %instance_name,
            %identification,
            %access_url,
            ip = %host_ip,
            %port,
            "Registered SOVD service on mDNS"
        );
        Ok(())
    }

    fn is_local_fullname(&self, fullname: &str) -> bool {
        match self.local_fullnames.lock() {
            Ok(local) => local.contains(fullname),
            Err(poisoned) => poisoned.into_inner().contains(fullname),
        }
    }

    /// Starts browsing for `_sovd._tcp.local.` services visible to this process.
    ///
    /// # Errors
    ///
    /// Returns [`MdnsError::Daemon`] if the browse operation cannot be started.
    pub fn browse(&self) -> Result<mdns_sd::Receiver<ServiceEvent>, MdnsError> {
        let receiver = self.daemon.browse(SERVICE_TYPE)?;
        Ok(receiver)
    }

    /// Shuts down the mDNS daemon and unregisters all services.
    ///
    /// # Errors
    ///
    /// Returns [`MdnsError::Daemon`] if the daemon fails to shut down cleanly.
    pub fn shutdown(&self) -> Result<(), MdnsError> {
        self.daemon.shutdown()?;
        Ok(())
    }
}

// Implements DiscoveryProvider for private-side mDNS service awareness.
pub struct MdnsDiscoveryProvider {
    wrapper: Arc<MdnsWrapper>,
}

impl MdnsDiscoveryProvider {
    /// Creates a discovery provider that owns a new [`MdnsWrapper`].
    ///
    /// # Errors
    ///
    /// Returns [`MdnsError::Daemon`] if the underlying mDNS daemon cannot be started.
    pub fn new() -> Result<Self, MdnsError> {
        Ok(Self {
            wrapper: Arc::new(MdnsWrapper::new()?),
        })
    }

    /// Creates a discovery provider sharing an existing MdnsWrapper.
    #[must_use]
    pub fn from_wrapper(wrapper: Arc<MdnsWrapper>) -> Self {
        Self { wrapper }
    }
}

#[async_trait]
impl DiscoveryProvider for MdnsDiscoveryProvider {
    async fn discover(&self) -> DiscoveryResult<DiscoveryStream> {
        let receiver = self
            .wrapper
            .browse()
            .map_err(|e| DiscoveryError::Transport(e.to_string()))?;

        let (tx, rx) = mpsc::channel::<DiscoveryResult<(Vec<EntityRef>, EntityCollection)>>(32);

        let wrapper = Arc::clone(&self.wrapper);
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = receiver.recv() {
                let Some(diff) = convert_event(|name| wrapper.is_local_fullname(name), event)
                else {
                    continue;
                };
                if tx.blocking_send(Ok(diff)).is_err() {
                    break;
                }
            }
        });

        Ok(Box::pin(MpscStream(rx)))
    }
}

fn convert_event(
    is_local: impl Fn(&str) -> bool,
    event: ServiceEvent,
) -> Option<(Vec<EntityRef>, EntityCollection)> {
    match event {
        ServiceEvent::ServiceResolved(info) => {
            let id = info.get_fullname().to_string();
            if is_local(&id) {
                return None;
            }
            let name = info.get_hostname().trim_end_matches('.').to_string();
            let port = info.get_port();

            let access_url = info.get_properties().get(TXT_ACCESS_URL).map_or_else(
                || {
                    info.get_addresses().iter().next().map_or_else(
                        || format!("http://{name}:{port}/sovd"),
                        |ip| format!("http://{ip}:{port}/sovd"),
                    )
                },
                |p| p.val_str().to_string(),
            );

            // Keep private access URLs inside the gateway. Public SOVD responses should not
            // expose addresses that infotainment cannot route to directly.
            let mut metadata = HashMap::new();

            if let Some(ident) = info
                .get_properties()
                .get(TXT_IDENTIFICATION)
                .map(mdns_sd::TxtProperty::val_str)
            {
                metadata.insert(TXT_IDENTIFICATION.to_string(), ident.to_string());
            }

            let component = Component::new(id, name).with_metadata(metadata);
            tracing::info!(target: "mdns", %access_url, "Discovered SOVD server");

            let mut collection = EntityCollection::default();
            collection.add_component(component);
            Some((vec![], collection))
        }

        ServiceEvent::ServiceRemoved(_service_type, fullname) => {
            if is_local(&fullname) {
                return None;
            }
            tracing::info!(target: "mdns", service = %fullname, "SOVD server left network");
            Some((
                vec![EntityRef::component(fullname)],
                EntityCollection::default(),
            ))
        }

        _ => None,
    }
}

struct MpscStream<T>(mpsc::Receiver<T>);

impl<T> Stream for MpscStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.0.poll_recv(cx)
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use mdns_sd::ServiceInfo;

    use super::*;

    // Builds a ServiceResolved event from a directly-constructed ServiceInfo.
    // This needs no running daemon, so the conversion logic can be tested offline.
    fn resolved_event(instance: &str, txt: &[(&str, &str)]) -> ServiceEvent {
        let host_name = format!("{instance}.local.");
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            instance,
            &host_name,
            "192.168.1.20",
            7690,
            txt,
        )
        .unwrap();
        ServiceEvent::ServiceResolved(info)
    }

    #[test]
    fn resolved_service_becomes_component_with_identification() {
        let event = resolved_event(
            "peer-ecu",
            &[
                (TXT_IDENTIFICATION, "VIN999"),
                (TXT_ACCESS_URL, "http://192.168.1.20:7690/sovd"),
            ],
        );

        let (removed, added) = convert_event(|_| false, event).unwrap();

        assert!(removed.is_empty());
        assert_eq!(added.components.len(), 1);
        let component = &added.components[0];
        assert_eq!(component.id(), "peer-ecu._sovd._tcp.local.");
        assert_eq!(component.name(), "peer-ecu.local");
        assert_eq!(
            component
                .metadata()
                .get(TXT_IDENTIFICATION)
                .map(String::as_str),
            Some("VIN999")
        );
    }

    #[test]
    fn resolved_service_without_txt_has_no_metadata() {
        let event = resolved_event("peer-ecu", &[]);

        let (_removed, added) = convert_event(|_| false, event).unwrap();

        assert_eq!(added.components.len(), 1);
        assert!(added.components[0].metadata().is_empty());
    }

    #[test]
    fn private_access_url_is_not_exposed_in_metadata() {
        let event = resolved_event("peer-ecu", &[(TXT_ACCESS_URL, "http://10.0.0.5:7690/sovd")]);

        let (_removed, added) = convert_event(|_| false, event).unwrap();

        let metadata = added.components[0].metadata();
        assert!(!metadata.contains_key(TXT_ACCESS_URL));
        assert!(metadata.values().all(|v| !v.contains("10.0.0.5")));
    }

    #[test]
    fn removed_service_yields_component_removal() {
        let event = ServiceEvent::ServiceRemoved(
            SERVICE_TYPE.to_string(),
            "peer-ecu._sovd._tcp.local.".to_string(),
        );

        let (removed, added) = convert_event(|_| false, event).unwrap();

        assert_eq!(
            removed,
            vec![EntityRef::component("peer-ecu._sovd._tcp.local.")]
        );
        assert!(added.components.is_empty());
    }

    #[test]
    fn self_registered_service_is_filtered_on_resolve() {
        let event = resolved_event("self-ecu", &[(TXT_IDENTIFICATION, "self")]);

        assert!(convert_event(|name| name == "self-ecu._sovd._tcp.local.", event).is_none());
    }

    #[test]
    fn self_registered_service_is_filtered_on_removal() {
        let event = ServiceEvent::ServiceRemoved(
            SERVICE_TYPE.to_string(),
            "self-ecu._sovd._tcp.local.".to_string(),
        );

        assert!(convert_event(|name| name == "self-ecu._sovd._tcp.local.", event).is_none());
    }
}
