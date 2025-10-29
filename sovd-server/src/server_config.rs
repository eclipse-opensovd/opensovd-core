/*
* Copyright (c) 2025 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
*
* See the NOTICE file(s) distributed with this work for additional
* information regarding copyright ownership.
*
* This program and the accompanying materials are made available under the
* terms of the Apache License Version 2.0 which is available at
* https://www.apache.org/licenses/LICENSE-2.0
*
* SPDX-License-Identifier: Apache-2.0
*/

use log::{error, info};
use mdns_sd::{Error, Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::time::Duration;

use std::sync::{Arc, Mutex};
pub struct ServiceDaemonWrapper {
    mdns: Arc<Mutex<ServiceDaemon>>,
}

#[allow(dead_code)]
impl ServiceDaemonWrapper {
    pub fn new(mdns: ServiceDaemon) -> Self {
        Self {
            mdns: Arc::new(Mutex::new(mdns)),
        }
    }

    pub fn browse(&self, service_type: &str) -> Result<Receiver<ServiceEvent>, Error> {
        let mdns = self.mdns.lock().expect("Failed to lock mdns");
        mdns.browse(service_type)
    }

    pub fn shutdown(&self) {
        let mdns = self.mdns.lock().expect("Failed to lock mdns");

        if let Err(e) = mdns.shutdown() {
            error!("mDNS shutdown failed: {}", e);
        }
    }

    pub fn register(&self, service_info: ServiceInfo) -> Result<(), Box<dyn std::error::Error>> {
        // Entpacken des Arc und Sperren des Mutex
        let mdns = self.mdns.lock().expect("Failed to lock ServiceDaemon");

        // Aufruf der register-Methode auf der ServiceDaemon-Instanz
        mdns.register(service_info)?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigEntry {
    pub component_id: String,
    pub apps: Vec<String>,
    pub instance_name: String,
    pub mode: String,
}

#[allow(dead_code)]
impl ConfigEntry {
    pub fn new(
        component_id: String,
        apps: Vec<String>,
        instance_name: String,
        mode: String,
    ) -> Self {
        ConfigEntry {
            component_id,
            apps,
            instance_name,
            mode,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub protocol: String,
    pub ip_address: String,
    pub port: String,
    pub base_uri: String,
    pub sovd_mode: String,
    pub host_name: String,
    pub sovd_server_list: Vec<String>,
    pub config_entries: Vec<ConfigEntry>,
}

#[allow(dead_code)]
impl ServerConfig {
    pub fn new(
        protocol: String,
        ip_address: String,
        port: String,
        sovd_mode: String,
        host_name: String,
        config_entries: Vec<ConfigEntry>,
    ) -> Self {
        let base_uri = format!("{}://{}:{}/v1", &protocol, &ip_address, &port);
        ServerConfig {
            protocol,
            ip_address,
            port,
            base_uri,
            sovd_mode,
            host_name,
            sovd_server_list: Vec::new(),
            config_entries,
        }
    }

    // Function to access the protocol
    pub fn get_protocol(&self) -> &str {
        &self.protocol
    }

    // Function to access the IP address
    pub fn get_ip_address(&self) -> &str {
        &self.ip_address
    }

    // Function to access the port
    pub fn get_port(&self) -> &str {
        &self.port
    }

    // Function to access the base URI
    pub fn get_base_uri(&self) -> &str {
        &self.base_uri
    }

    // Function to access the SOVD mode
    pub fn get_sovd_mode(&self) -> &str {
        &self.sovd_mode
    }

    // Function to access the host name
    pub fn get_hostname(&self) -> &str {
        &self.host_name
    }

    // Function to access the configuration entries
    pub fn get_config_entries(&self) -> &Vec<ConfigEntry> {
        &self.config_entries
    }

    pub fn get_instance_name_for_standalone(&self) -> Option<String> {
        for entry in &self.config_entries {
            if entry.mode == "standalone" {
                return Some(entry.instance_name.clone());
            }
        }
        None
    }

    pub fn get_component_by_app(&self, app_id: &str) -> Option<&str> {
        // Remove the part after the last hyphen
        let app_id_base = match app_id.rsplit_once('-') {
            Some((base, _)) => base,
            None => app_id,
        };

        info!("Original app_id: {}", app_id);
        info!("Cleaned app_id: {}", app_id_base);

        for entry in &self.config_entries {
            info!("Checking component: {}", entry.component_id);
            if entry.apps.contains(&app_id_base.to_string()) {
                info!("Found in component: {}", entry.component_id);
                return Some(&entry.component_id);
            }
        }

        info!("No component found for app_id: {}", app_id_base);
        None
    }

    // get componant name
    pub fn get_component_name(&self, component_id: &str) -> Option<&str> {
        for entry in &self.config_entries {
            if entry.component_id == component_id {
                return Some(&entry.instance_name);
            }
        }
        None
    }

    // Get apps
    pub fn get_apps_by_component_id(&self, component: &str) -> Option<&Vec<String>> {
        for entry in &self.config_entries {
            if entry.component_id == component {
                return Some(&entry.apps);
            }
        }
        None
    }

    // Function to read settings from file
    fn read_settings(file_path: &str) -> io::Result<Vec<ConfigEntry>> {
        let file = File::open(file_path)?;
        let reader = io::BufReader::new(file);
        let json_data: serde_json::Value = serde_json::from_reader(reader)?;

        if let Some(entries) = json_data.get("config_entries") {
            info!("Found config_entries: {:?}", entries);
            let config_entries: Vec<ConfigEntry> = serde_json::from_value(entries.clone())?;
            info!("Parsed config_entries: {:?}", config_entries);
            return Ok(config_entries);
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Missing config_entries key",
        ))
    }

    pub fn get_ip_and_port(
        &self,
        mdns: &ServiceDaemonWrapper,
        instance_name: &String,
    ) -> Option<(String, u16)> {
        // Check for mDNS messages
        let service_type = "_sovd_server._udp.local.";
        let receiver = mdns.browse(service_type).expect("Failed to browse mDNS");

        let timeout = Duration::from_secs(2); // Timeout 2 seconds

        // Infinite loop with timeout
        let start_time = std::time::Instant::now();
        while start_time.elapsed() < timeout {
            match receiver.recv_timeout(timeout) {
                Ok(event) => {
                    if let ServiceEvent::ServiceResolved(info) = event {
                        // Check if it is chassis-hpc
                        if info.get_fullname().starts_with(instance_name) {
                            // Extract IP-Address and Port
                            let ip_address = info
                                .get_addresses_v4()
                                .iter()
                                .next()
                                .map(|ip| ip.to_string())
                                .unwrap_or_else(|| "127.0.0.1".to_string());
                            let port = info.get_port();
                            return Some((ip_address, port));
                        }
                    }
                }

                Err(err) => {
                    error!("Error receiving mDNS event: {:?}", err);
                    break; // Break the loop on error or timeout
                }
            }
        }

        error!("mDNS browse timed out or no matching service found.");
        None // No device found within timeout
    }

    pub fn create_server_settings(
        file_path: &str,
        protocol: String,
        ip_address: String,
        port: String,
        sovd_mode: String,
        host_name: String,
    ) -> io::Result<Self> {
        let config_entries = Self::read_settings(file_path)?;
        Ok(ServerConfig::new(
            protocol,
            ip_address,
            port,
            sovd_mode,
            host_name,
            config_entries,
        ))
    }
}
