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

use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, to_value};

use hyper::client::HttpConnector;
use hyper::{Body, Client, Request, Response};
use serde_json::{Map, Value};

use log::{error, info};
use std::fs;
use std::str;
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, ProcessStatus, RefreshKind, System};

use std::net::TcpStream;

use std::time::Duration;

use hyper::{Method, header::HeaderMap};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;

use serde_json::json;

use sysinfo::Disks;

use openapi_client::EntityCollectionEntityIdDataDataIdGetResponse;
use openapi_client::EntityCollectionEntityIdDataGetResponse;
use openapi_client::EntityCollectionEntityIdDataGroupsGetResponse;
use openapi_client::EntityCollectionEntityIdGetResponse;
use openapi_client::models::*;

use lazy_static::lazy_static;
use tokio::time::timeout;
// Only for testing
lazy_static! {
    pub static ref IDENT_DATA_RESPONSE: Mutex<Vec<JsonValue>> = Mutex::new(vec![]);
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiskStats {
    reads_completed: u64,
    reads_merged: u64,
    sectors_read: u64,
    read_time: u64,
    writes_completed: u64,
    writes_merged: u64,
    sectors_written: u64,
    write_time: u64,
    io_in_progress: u64,
    io_time: u64,
    weighted_io_time: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    name: String,
    value: String,
    data_type: String,
    unit: String,
    factor: u32,
    divisor: u32,
    precision: u32,
    offset: u32,
    bit_length: u32,
    length: u32,
    raw_coding: String,
    min_value: u32,
    max_value: u32,
    package_byte_length: u32,
    sub_records: Option<Vec<Record>>,
}

// Structure for a single element in the JSON array
#[derive(Debug, Deserialize, Serialize)]
pub struct JsonElement {
    identifier: u64,
    name: String,
    is_writable: bool,
    records: Vec<Record>,
}
#[derive(Serialize, Deserialize, Debug)]
struct DiskSpaceInfo {
    name: String,
    pid: Option<String>,
    total_space_in_kilobyte: u64,
    available_space_in_kilobyte: u64,
    used_space: Option<u64>,
}

// Function that searches by identifier or name and creates a EntityCollectionEntityIdDataDataIdGet200Response object
pub fn find_and_create_read_value(
    response: &str,
    search_identifier: Option<u64>,
    search_name: Option<&str>,
) -> Option<EntityCollectionEntityIdDataDataIdGet200Response> {
    // Deserialization of JSON-Arrays
    let json_array: Vec<JsonElement> = serde_json::from_str(response).ok()?;

    // Search for identifier or name in the JSON array
    for element in json_array {
        if search_identifier.is_none_or(|id| element.identifier == id)
            && search_name.is_none_or(|name| element.name == name)
        {
            // Convert the found element into a EntityCollectionEntityIdDataDataIdGet200Response object
            let mut collect_data: Map<String, JsonValue> = Map::new();
            let any_value: Vec<_> = element
                .records
                .into_iter()
                .map(|record| collect_data.insert(record.name, JsonValue::String(record.value)))
                .collect();

            return Some(EntityCollectionEntityIdDataDataIdGet200Response {
                id: element.identifier.to_string(),
                data: to_value(any_value).expect("Failed to create read value"),
                errors: None,
                schema: None,
            });
        }
    }

    None
}

// Your existing function with Result as the return type
pub fn filter_by_writable(
    response: &str,
    writable: bool,
) -> Result<Vec<EntityCollectionEntityIdDataDataIdGet200Response>, Box<dyn Error>> {
    // Deserialization of JSON-Arrays
    let json_array: Vec<JsonElement> =
        serde_json::from_str(response).map_err(|e| Box::new(e) as Box<dyn Error>)?;

    // Vector to store filtered elements
    let mut matching_elements = Vec::new();

    // Filter by isWritable in the JSON array
    for element in json_array {
        if element.is_writable == writable {
            // Convert the found element into a EntityCollectionEntityIdDataDataIdGet200Response object
            let mut collect_data: Map<String, JsonValue> = Map::new();
            let any_value: Vec<_> = element
                .records
                .into_iter()
                .map(|record| collect_data.insert(record.name, JsonValue::String(record.value)))
                .collect();

            let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                id: element.identifier.to_string(),
                data: to_value(any_value).expect("Failed to filter writables"),
                errors: None,
                schema: None,
            };

            // Store the found element
            matching_elements.push(read_value);
        }
    }

    // Return the results as Result
    Ok(matching_elements)
}

// Function that returns the first found element with identical identifier
pub fn find_by_identifier(
    response: &str,
    search_identifier: u64,
) -> Option<EntityCollectionEntityIdDataDataIdGet200Response> {
    // Deserialization of the JSON array
    let json_array: Vec<JsonElement> = serde_json::from_str(response).ok()?;

    // Search for identifier in the JSON array
    if let Some(element) = json_array
        .into_iter()
        .find(|e| e.identifier == search_identifier)
    {
        // Convert the found element into a EntityCollectionEntityIdDataDataIdGet200Response object
        let mut collect_data: Map<String, JsonValue> = Map::new();
        let any_value: Vec<_> = element
            .records
            .into_iter()
            .map(|record| collect_data.insert(record.name, JsonValue::String(record.value)))
            .collect();
        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
            id: element.identifier.to_string(),
            data: to_value(any_value).expect("Failed to find by identifier"),
            errors: None,
            schema: None,
        };

        // Return the found element
        Some(read_value)
    } else {
        // If no element was found
        None
    }
}

// Function that returns the first found element with identical name
pub fn find_by_name(
    response: &str,
    search_name: &str,
) -> Option<EntityCollectionEntityIdDataDataIdGet200Response> {
    // Deserialization of the JSON array
    let json_array: Vec<JsonElement> = serde_json::from_str(response).ok()?;

    // Search for name in the JSON array
    if let Some(element) = json_array.into_iter().find(|e| e.name == search_name) {
        // Convert the found element into a EntityCollectionEntityIdDataDataIdGet200Response object
        let mut collect_data: Map<String, JsonValue> = Map::new();
        let any_value: Vec<_> = element
            .records
            .into_iter()
            .map(|record| collect_data.insert(record.name, JsonValue::String(record.value)))
            .collect();
        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
            id: element.identifier.to_string(),
            data: to_value(any_value).expect("Failed to find by name"),
            errors: None,
            schema: None,
        };

        // Return the found element
        Some(read_value)
    } else {
        // If no element was found
        None
    }
}

pub fn create_entity_collection_response(
    json_data: &[JsonValue],
) -> Result<
    EntityCollectionEntityIdDataGroupsGetResponse,
    Box<EntityCollectionEntityIdDataDataIdGet200ResponseErrorsInnerError>,
> {
    // Process the JSON elements
    let items: Vec<EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner> = json_data
        .iter()
        .filter_map(|json_value| serde_json::from_value(json_value.clone()).ok())
        .map(|json_element: JsonElement| {
            // Here you add the type
            // Create the ID by concatenating identifier and name
            let id = format!(
                "{}-{}",
                json_element.identifier,
                json_element.name.replace(' ', "-")
            );

            // Create a EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner object with the required fields
            EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner {
                id,
                category: "identData".to_string(),
                category_translation_id: None,
                group: None,
                group_translation_id: None,
            }
        })
        .collect();

    let inline_response = EntityCollectionEntityIdDataGroupsGet200Response::new(items);

    // Create and return the EntityCollectionEntityIdDataGroupsGetResponse
    Ok(EntityCollectionEntityIdDataGroupsGetResponse::TheRequestWasSuccessful(inline_response))
}

pub fn group_by_writability(
    json_data: &Vec<JsonValue>,
) -> Result<
    EntityCollectionEntityIdDataGroupsGetResponse,
    Box<EntityCollectionEntityIdDataDataIdGet200ResponseErrorsInnerError>,
> {
    // Group the JSON elements by isWritable
    let mut grouped_data: HashMap<
        String,
        EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner,
    > = HashMap::new();

    for json_value in json_data {
        if let Ok(json_element) = serde_json::from_value::<JsonElement>(json_value.clone()) {
            let is_writable = json_element.is_writable;
            let id = if is_writable {
                "writeable"
            } else {
                "nonWriteable"
            };

            // Add the ValueGroup only if the ID does not already exist
            grouped_data.entry(id.to_string()).or_insert_with(|| {
                EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner::new(
                    id.to_string(),
                    "identData".to_string(),
                )
            });
        }
    }

    let items: Vec<EntityCollectionEntityIdDataGroupsGet200ResponseItemsInner> =
        grouped_data.into_values().collect();

    // Create and return the EntityCollectionEntityIdDataGroupsGetResponse
    Ok(
        EntityCollectionEntityIdDataGroupsGetResponse::TheRequestWasSuccessful(
            EntityCollectionEntityIdDataGroupsGet200Response::new(items),
        ),
    )
}

pub fn prepare_data_response(
    json_data: &[JsonValue],
) -> Result<
    EntityCollectionEntityIdDataGetResponse,
    Box<EntityCollectionEntityIdDataDataIdGet200ResponseErrorsInnerError>,
> {
    // Filter the JSON elements based on isWritable
    let writable_elements: Vec<_> = json_data
        .iter()
        .filter_map(|json_value| serde_json::from_value(json_value.clone()).ok())
        .filter(|json_element: &JsonElement| json_element.is_writable)
        .collect();

    let non_writable_elements: Vec<_> = json_data
        .iter()
        .filter_map(|json_value| serde_json::from_value(json_value.clone()).ok())
        .filter(|json_element: &JsonElement| !json_element.is_writable)
        .collect();

    // Create the groups
    let writable_group = create_group(writable_elements, "writeable".to_string())?;
    let non_writable_group = create_group(non_writable_elements, "nonWriteable".to_string())?;

    // Concatenate the two groups
    let items: Vec<EntityCollectionEntityIdDataGet200ResponseItemsInner> = writable_group
        .into_iter()
        .chain(non_writable_group)
        .collect();

    let inline_response = EntityCollectionEntityIdDataGet200Response::new(items);

    Ok(EntityCollectionEntityIdDataGetResponse::TheRequestWasSuccessful(inline_response))
}

// Helper function to create a group based on isWritable
fn create_group(
    elements: Vec<JsonElement>,
    group_id: String,
) -> Result<
    Vec<EntityCollectionEntityIdDataGet200ResponseItemsInner>,
    Box<EntityCollectionEntityIdDataDataIdGet200ResponseErrorsInnerError,>,
> {
    Ok(elements
        .into_iter()
        .map(|json_element| {
            // Create the ID by concatenating identifier and name
            let id: String = json_element.identifier.to_string();

            // Create a ValueMetadata object with the required fields
            EntityCollectionEntityIdDataGet200ResponseItemsInner {
                id,
                name: json_element.name.clone(),
                translation_id: None, // If needed, you can add the translation here
                category: "identData".to_string(),
                groups: Some(vec![group_id.clone()]),
            }
        })
        .collect())
}

/// Returns a process entity element for the given process name
///
/// # Arguments
///
/// * `process_name` - A string slice that holds the name of the process
/// * `process_pid` - A string slice that holds the pid of the process
/// * `base_uri` - A string slice that holds the base uri
///
/// # Examples
///
pub fn find_single_process(
    process_name: &str,
    process_pid: &str,
    base_uri: &str,
) -> Option<EntityCollectionGet200ResponseItemsInner> {
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    system.refresh_processes();

    let pid = match process_pid.parse::<u32>() {
        Ok(pid) => pid,
        Err(_) => return None,
    };

    for process in system.processes().values() {
        let cmd_string = process.cmd().join(" ");
        if cmd_string.contains(process_name) && process.pid().as_u32() == pid {
            let name = process_name.replace(" ", "-");
            let pid_name = format!("{}-{}", name, pid);
            let href = format!("{}/apps/{}", base_uri, pid_name);
            return Some(EntityCollectionGet200ResponseItemsInner::new(
                pid_name, name, href,
            ));
        }
    }

    None
}

// Function to search and return processes
pub fn find_processes(
    search_terms: Vec<&str>,
    base_uri: &str,
) -> Vec<EntityCollectionGet200ResponseItemsInner> {
    use sysinfo::{ProcessRefreshKind, ProcessStatus, RefreshKind, System};

    let system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    let all_processes = system.processes();
    let mut response_body = Vec::new();

    for term in search_terms {
        if let Some((_, process)) = all_processes.iter().find(|(_, process)| {
            let cmd_string = process.cmd().join(" ");
            cmd_string.contains(term) && process.status() != ProcessStatus::Idle
        }) {
            let name = term.replace(" ", "-");
            let pid = process.pid();
            let pid_name = format!("{}-{}", name, pid);
            let href = format!("{}/apps/{}", base_uri, pid_name);
            let entity_ref = EntityCollectionGet200ResponseItemsInner::new(pid_name, name, href);
            response_body.push(entity_ref);
        }
    }

    response_body
}

// Function to search and return pid
pub fn get_process_pid(search_term: &str) -> Option<i32> {
    let system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    system.processes().iter().find_map(|(_, process)| {
        let cmd_string = process.cmd().join(" ");
        if cmd_string.contains(search_term) && process.status() != ProcessStatus::Idle {
            Some(process.pid().as_u32() as i32) // Convert Pid to i32
        } else {
            None
        }
    })
}

// Find an entity by name and return its reference
pub fn find_entity_by_name(
    search_term: &str,
    base_uri: &str,
) -> Option<EntityCollectionGet200ResponseItemsInner> {
    let system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    let all_processes = system.processes(); // Retrieve all running processes
    for process in all_processes {
        let cmd = process.1.cmd(); // Retrieve the command line of the process
        let name = if !cmd.is_empty() {
            let name = Path::new(&cmd[0])
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let name = if let Some(idx) = name.rfind('.') {
                name[..idx].to_string()
            } else {
                name
            };
            name.replace(" ", "-")
        } else {
            "".to_string()
        };

        let name_with_pid = format!("{}-{}", name, process.1.pid()); // Extend name with PID

        // Check if the process is not in an idle state, the name is not empty, and contains the search term
        if process.1.status() != ProcessStatus::Idle
            && !name.is_empty()
            && name_with_pid.contains(search_term)
        {
            let pid = process.1.pid();
            let href = format!("{}/apps/{}", base_uri, name_with_pid); // Construct the resource URI
            return Some(EntityCollectionGet200ResponseItemsInner::new(
                pid.to_string(),
                name.clone(),
                href,
            )); // Return entity reference
        }
    }
    None // Return None if no matching entity is found
}

// Extract name from string and replace dashes
pub fn extract_name_and_replace_dashes(input_string: &str, pid: &str) -> String {
    let name = input_string.trim_end_matches(pid); // Remove PID from input string
    let mut formatted_name = name.replace("-", " "); // Replace dashes with spaces, except the last one
    if let Some(last_dash_index) = formatted_name.rfind('-') {
        formatted_name.replace_range(last_dash_index..last_dash_index + 1, " ");
    }
    formatted_name // Return formatted name
}

// Get the last part of a string after the last dash
pub fn get_first_part_after_dash(entity_id: &str) -> String {
    if let Some(last_dash_index) = entity_id.rfind('-') {
        // Find the index of the last dash
        return entity_id[..(last_dash_index)].to_string(); // Return the substring before the last dash
    }
    entity_id.to_string() // Return the original string if no dash is found
}

// Get the last part of a string after the last dash
pub fn get_last_part_after_dash(entity_id: &str) -> String {
    if let Some(last_dash_index) = entity_id.rfind('-') {
        // Find the index of the last dash
        return entity_id[(last_dash_index + 1)..].to_string(); // Return the substring after the last dash
    }
    entity_id.to_string() // Return the original string if no dash is found
}

pub fn get_before_last_dash(entity_id: &str) -> String {
    if let Some(last_dash_index) = entity_id.rfind('-') {
        // Find the index of the last dash
        return entity_id[..last_dash_index].to_string(); // Return the substring before the last dash
    }
    entity_id.to_string() // Return the original string if no dash is found
}
use std::thread;
pub fn get_cpu_usage(pid_str: &str) -> Option<f32> {
    let mut system = System::new_all();

    // Versuchen, die PID in den Pid-Typ umzuwandeln
    let pid: Pid = match Pid::from_str(pid_str) {
        Ok(p) => p,
        Err(_) => {
            error!("Error: PID must be a valid number.");
            return None;
        }
    };

    // Initial update of the system information
    system.refresh_all();

    // Wait and refresh system information again
    thread::sleep(Duration::from_secs(1));
    system.refresh_all();

    // Get the process
    if let Some(process) = system.process(pid) {
        let cpu_usage = process.cpu_usage();
        info!("CPU usage of process {}: {}%", pid, cpu_usage);
        Some(cpu_usage)
    } else {
        info!("Process with PID {} not found", pid);
        None
    }
}

pub fn disk_total_space() {
    let disks = Disks::new_with_refreshed_list();
    for disk in disks.list() {
        info!("[{:?}] {}B", disk.name(), disk.total_space());
        info!("[{:?}] {}B", disk.name(), disk.available_space());
    }
}

pub fn get_process_filesize(pid: u32) -> Option<u64> {
    let system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    let process = system.process(Pid::from_u32(pid)).unwrap();
    let final_size = process.memory();

    info!("get_process_filesize {}", final_size);

    Some(final_size) // Convert the result of `rss_bytes` to `u64` and return it
}

pub fn get_executable_size(pid: u32) -> Option<u64> {
    // Construct the file path to the symbolic link to the process's executable
    let exe_link_path = format!("/proc/{}/exe", pid);

    // Try to read the symbolic link to obtain the file path of the executable
    match fs::read_link(&exe_link_path) {
        Ok(exe_path) => {
            // Try to read the metadata of the executable file to obtain its size
            match fs::metadata(exe_path) {
                Ok(metadata) => Some(metadata.len()),
                Err(err) => {
                    error!("Error reading metadata of the executable file: {}", err);
                    None
                }
            }
        }
        Err(err) => {
            error!(
                "Error reading symbolic link to the executable file: {}",
                err
            );
            None
        }
    }
}

pub fn get_disk_usage_for_pid(pid: i32) -> Option<u64> {
    let proc_fd_path = format!("/proc/{}/fd", pid);
    let mut disk_usage = 0;
    info!("get_disk_usage_for_pid with {}", pid);
    if let Ok(entries) = fs::read_dir(proc_fd_path) {
        for entry in entries {
            if let Ok(entry) = entry && let Ok(metadata) = entry.metadata() {
                    // Nur reguläre Dateien berücksichtigen
                    if metadata.is_file() {
                        disk_usage += metadata.len();
                    }
                }
            
        }
        info!("get_disk_usage_for_pid {}", disk_usage);
        Some(disk_usage)
    } else {
        info!("NONE {}", disk_usage);
        None
    }
}

pub fn get_disk_info() -> serde_json::Result<String> {
    // Create an empty vector structure to collect memory information
    let mut disk_infos = Vec::new();

    // Create a Disks object and retrieve an updated list of available hard drives
    let disks = Disks::new_with_refreshed_list();

    // Iterate over each hard drive and collect storage information
    for disk in disks.list() {
        let disk_info = DiskSpaceInfo {
            name: disk.name().to_str().unwrap().to_string(),
            pid: None,
            total_space_in_kilobyte: disk.total_space() / 1000,
            available_space_in_kilobyte: disk.available_space() / 1000,
            used_space: None,
        };

        // Add the storage information to the vector structure
        disk_infos.push(disk_info);
    }

    // Serialize the vector structure into JSON
    let json_output = serde_json::to_string(&disk_infos)?;

    // Return the JSON string
    Ok(json_output)
}

pub fn get_memory_usage(pid: &str) -> Option<u64> {
    let s = System::new_all();

    if let Ok(pid_int) = pid.parse::<u32>() {
        s.process(Pid::from_u32(pid_int)).map(|process| process.memory() / 1000)
    } else {
        None
    }
}

pub fn get_disk_io(pid: &str) -> Option<(u64, u64)> {
    let s = System::new_all();

    if let Ok(pid_int) = pid.parse::<u32>() {
        if let Some(process) = s.process(Pid::from_u32(pid_int)) {
            let disk_usage = process.disk_usage();
            let read_bytes = disk_usage.read_bytes;
            let read_bytes_total = disk_usage.total_read_bytes;
            let write_bytes = disk_usage.written_bytes;
            let write_byte_total = disk_usage.total_written_bytes;
            info!(
                "Read {}, Total read {}",
                read_bytes / 1000,
                read_bytes_total / 1000
            );
            info!(
                "Write {}, Total written {}",
                write_bytes / 1000,
                write_byte_total / 1000
            );
            Some((read_bytes_total / 1000, write_byte_total / 1000))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn handle_app_resource(
    resource: &str,
    pid_to_monitor: &str,
    entity: &str,
    id: &str,
) -> EntityCollectionEntityIdDataDataIdGetResponse {
    match resource {
        "cpu" => {
            if let Some(cpu_usage) = get_cpu_usage(pid_to_monitor) {
                let mut response_data = BTreeMap::new();
                response_data.insert("name".to_string(), JsonValue::String("CPU".to_string()));
                response_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("CPU usage for {}", entity)),
                );
                response_data.insert(
                    "cpu_usage".to_string(),
                    JsonValue::String(format!("{:.2}%", cpu_usage)),
                );

                let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                    id: id.to_string(),
                    data: to_value(response_data).expect("Failed to serialize CPU usage"),
                    errors: None,
                    schema: None,
                };

                EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
            } else {
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "UnknownResource".to_string(),
                    message: "Unknown resource.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
            }
        }
        "memory" => {
            if let Some(memory_usage) = get_memory_usage(pid_to_monitor) {
                let mut response_data = BTreeMap::new();
                response_data.insert("name".to_string(), JsonValue::String("Memory".to_string()));
                response_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("Memory usage for {}", entity)),
                );
                response_data.insert("memory_usage_kb".to_string(), memory_usage.into());

                if let Some((total_memory_mb, used_memory_mb)) = get_system_memory_usage() {
                    let total_memory_as_json_number = JsonValue::Number(total_memory_mb.into());
                    let used_memory_as_json_number = JsonValue::Number(used_memory_mb.into());

                    response_data
                        .insert("total_memory_mb".to_string(), total_memory_as_json_number);
                    response_data.insert("used_memory_mb".to_string(), used_memory_as_json_number);
                }

                let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                    id: id.to_string(),
                    data: to_value(response_data).expect("Failed to serialize memory usage"),
                    errors: None,
                    schema: None,
                };

                EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
            } else {
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "UnknownResource".to_string(),
                    message: "Unknown resource.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
            }
        }
        "disk" => {
            if let Some(disk_io) = get_executable_size(pid_to_monitor.parse::<u32>().unwrap()) {
                info!("disk {}", disk_io);
                let disks = Disks::new_with_refreshed_list();
                let disk_space_available = disks.first().unwrap().available_space();
                let total_disk_space = disks.first().unwrap().total_space();
                let mut response_data = BTreeMap::new();
                response_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("Disk usage for {}", entity)),
                );
                response_data.insert("application_size_byte".to_string(), disk_io.into());
                response_data.insert(
                    "disk_space_available_byte".to_string(),
                    disk_space_available.into(),
                );
                response_data.insert("total_disk_space_byte".to_string(), total_disk_space.into());

                let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                    id: id.to_string(),
                    data: to_value(response_data).expect("Failed to serialize disk usage"),
                    errors: None,
                    schema: None,
                };
                EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
            } else {
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "UnknownResource".to_string(),
                    message: "Unknown resource.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
            }
        }
        "all" => {
            // Behandlung für alle anderen Fälle (all)
            let mut response_data = BTreeMap::new();
            response_data.insert("name".to_string(), JsonValue::String(entity.to_string()));
            response_data.insert(
                "description".to_string(),
                JsonValue::String(
                    "App system resources monitoring for CPU-, Memory usage and Disk usage"
                        .to_string(),
                ),
            );

            let mut resources = Map::new();

            if let Some(disk_io) = get_executable_size(pid_to_monitor.parse::<u32>().unwrap()) {
                let mut disk_data = Map::new();
                let disks = Disks::new_with_refreshed_list();
                let disk_space_available = disks.first().unwrap().available_space();
                let total_disk_space = disks.first().unwrap().total_space();
                disk_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("Disk usage {}", entity)),
                );
                disk_data.insert("application_size_byte".to_string(), disk_io.into());
                disk_data.insert(
                    "disk_space_available_byte".to_string(),
                    disk_space_available.into(),
                );
                disk_data.insert("total_disk_space_byte".to_string(), total_disk_space.into());
                resources.insert("disk".to_string(), JsonValue::Object(disk_data));
            }

            if let Some(cpu_usage) = get_cpu_usage(pid_to_monitor) {
                let mut cpu_data = Map::new();
                cpu_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("CPU usage for {}", entity)),
                );
                cpu_data.insert(
                    "cpu_usage".to_string(),
                    JsonValue::String(format!("{:.2}%", cpu_usage)),
                );
                resources.insert("cpu".to_string(), JsonValue::Object(cpu_data));
            }

            if let Some(memory_usage) = get_memory_usage(pid_to_monitor) {
                let mut memory_data = Map::new();
                memory_data.insert(
                    "description".to_string(),
                    JsonValue::String(format!("Memory usage for {}", entity)),
                );
                memory_data.insert("memory_usage_kb".to_string(), memory_usage.into());

                if let Some((total_memory_mb, used_memory_mb)) = get_system_memory_usage() {
                    let total_memory_as_json_number = JsonValue::Number(total_memory_mb.into());
                    let used_memory_as_json_number = JsonValue::Number(used_memory_mb.into());

                    memory_data.insert("total_memory_mb".to_string(), total_memory_as_json_number);
                    memory_data.insert("used_memory_mb".to_string(), used_memory_as_json_number);
                }

                resources.insert("memory".to_string(), JsonValue::Object(memory_data));
            }

            response_data.insert("resources".to_string(), JsonValue::Object(resources));

            let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                id: id.to_string(),
                data: to_value(response_data).expect("Failed to serialize disk usage"),
                errors: None,
                schema: None,
            };
            EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
        }
        _ => {
            info!("Unknown resource type: {}", resource);
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "UnknownResource".to_string(),
                message: "Unknown resource.".to_string(),
                vendor_code: None,
                translation_id: None,
                parameters: None,
            };
            EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
        }
    }
}

pub fn get_system_cpu_usage() -> Option<f32> {
    let mut s =
        System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));

    // Wait a bit because CPU usage is based on diff.
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    // Refresh CPUs again.
    s.refresh_cpu();

    let mut cpu_usage_sum: f32 = 0.0;
    let num_cores = s.physical_core_count().unwrap_or_default() as f32;

    for cpu in s.cpus() {
        cpu_usage_sum += cpu.cpu_usage();
        info!(
            "cpu.cpu_usage(): {} num cores: {}",
            cpu.cpu_usage(),
            num_cores
        );
    }
    info!("cpu_usage_sum: {} num cores: {}", cpu_usage_sum, num_cores);
    Some(cpu_usage_sum / num_cores)
}

pub fn get_system_memory_usage() -> Option<(u64, u64)> {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let mut total_memory_kb = 0;
        let mut free_memory_kb = 0;

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                match parts[0] {
                    "MemTotal:" => {
                        total_memory_kb = parts[1].parse().unwrap_or(0);
                    }
                    "MemFree:" => {
                        free_memory_kb = parts[1].parse().unwrap_or(0);
                    }
                    _ => {}
                }
            }
        }

        if total_memory_kb > 0 {
            // Conversion from kilobytes to megabytes
            let total_memory_mb = total_memory_kb / 1024;
            let used_memory_mb = (total_memory_kb - free_memory_kb) / 1024;
            return Some((total_memory_mb, used_memory_mb));
        }
    }

    None
}

pub fn get_system_disk_io() -> Vec<(String, i32, u64, u64)> {
    let mut result = Vec::new();
    let s = System::new_all();

    for process in s.processes().values() {
        let disk_usage = process.disk_usage();
        let name = process.name().to_string();
        let pid_value = process.pid().to_string().parse::<i32>().unwrap();
        let read_bytes = disk_usage.read_bytes;
        let written_bytes = disk_usage.written_bytes;
        result.push((name, pid_value, read_bytes, written_bytes));
    }
    result
}
pub fn handle_system_resource(
    resource: &str,
    entity: &str,
    id: &str,
) -> EntityCollectionEntityIdDataDataIdGetResponse {
    match resource {
        "cpu" => handle_cpu_resource(id),
        "memory" => handle_memory_resource(id),
        "disk" => handle_disk_resource(id),
        "all" => handle_all_system_resources(id, entity),
        _ => {
            info!("Unknown resource type: {}", resource);
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "UnknownResource".to_string(),
                message: "Unknown resource.".to_string(),
                vendor_code: None,
                translation_id: None,
                parameters: None,
            };
            EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
        }
    }
}

pub fn handle_cpu_resource(id: &str) -> EntityCollectionEntityIdDataDataIdGetResponse {
    if let Some(cpu_usage) = get_system_cpu_usage() {
        let cpu_usage_formatted = format!("{:.2}%", cpu_usage); // Format CPU usage as a percentage
        let mut response_data = BTreeMap::new();
        response_data.insert(
            "cpu_usage".to_string(),
            JsonValue::String(cpu_usage_formatted),
        );
        response_data.insert(
            "description".to_string(),
            JsonValue::String(format!(
                "CPU usage for component {}",
                get_first_part_after_dash(id)
            )),
        );
        response_data.insert("name".to_string(), JsonValue::String("CPU".to_string()));

        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
            id: id.to_string(),
            data: to_value(response_data).expect("Failed to serialize handle cpu usage"),
            errors: None,
            schema: None,
        };

        EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
    } else {
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnknownResource".to_string(),
            message: "Unknown resource.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };
        EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
    }
}
fn handle_memory_resource(id: &str) -> EntityCollectionEntityIdDataDataIdGetResponse {
    if let Some((total_memory_mb, used_memory_mb)) = get_system_memory_usage() {
        let total_memory_as_json_number = JsonValue::Number(total_memory_mb.into());
        let used_memory_as_json_number = JsonValue::Number(used_memory_mb.into());

        let mut response_data = BTreeMap::new();
        response_data.insert("total_memory_mb".to_string(), total_memory_as_json_number);
        response_data.insert("used_memory_mb".to_string(), used_memory_as_json_number);

        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
            id: id.to_string(),
            data: to_value(response_data).expect("Failed to serialize handle memory usage"),
            errors: None,
            schema: None,
        };

        EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
    } else {
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnknownResource".to_string(),
            message: "Unknown resource.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };
        EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error)
    }
}

pub fn handle_disk_resource(id: &str) -> EntityCollectionEntityIdDataDataIdGetResponse {
    let mut resources = Map::new();
    let disks = Disks::new_with_refreshed_list();
    let disk_space_available = disks.first().unwrap().available_space();
    let total_disk_space = disks.first().unwrap().total_space();

    resources.insert(
        ("disk_space_available_bytes").into(),
        disk_space_available.into(),
    );
    resources.insert(("total_disk_space_bytes").into(), total_disk_space.into());

    let response_data = json!({
        "description": "System resources monitoring Disk space",
        "name": format!("Component {}", get_first_part_after_dash(id)), // Extract the first part before the first hyphen
        "resources": resources
    });

    let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
        id: id.to_string(),
        data: to_value(response_data).expect("Error"),
        errors: None,
        schema: None,
    };
    EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
}

fn handle_all_system_resources(
    id: &str,
    entity: &str,
) -> EntityCollectionEntityIdDataDataIdGetResponse {
    let mut response_data = Map::new();
    response_data.insert("name".to_string(), JsonValue::String(entity.to_string()));
    response_data.insert(
        "description".to_string(),
        JsonValue::String(
            "App system resources monitoring for CPU-, Memory usage and Disk I/O".to_string(),
        ),
    );

    let mut resources = Map::new();

    let mut disk_io_entries = Map::new();
    let disk_io_data = get_system_disk_io();
    for (name, pid, read_speed_b_s, write_speed_b_s) in disk_io_data {
        let description = format!("Disk I/O for {}", name);
        let mut disk_entry = Map::new();
        disk_entry.insert(
            "description".to_string(),
            JsonValue::String(description.clone()),
        );
        disk_entry.insert("name".to_string(), JsonValue::String(name.clone()));
        disk_entry.insert("pid".to_string(), JsonValue::Number(pid.into()));
        disk_entry.insert(
            "read_speed_b_s".to_string(),
            JsonValue::Number(read_speed_b_s.into()),
        );
        disk_entry.insert(
            "write_speed_b_s".to_string(),
            JsonValue::Number(write_speed_b_s.into()),
        );
        disk_io_entries.insert(name.clone(), JsonValue::Object(disk_entry));
    }
    resources.insert("disk".to_string(), JsonValue::Object(disk_io_entries));

    if let Some(cpu_usage) = get_system_cpu_usage() {
        let cpu_usage_as_u64 = cpu_usage as u64;
        let cpu_usage_as_json_number = JsonValue::Number(cpu_usage_as_u64.into());

        let mut cpu_usage_data = Map::new();
        let cpu_usage_formatted = format!("{:.2}%", cpu_usage_as_json_number); // Format CPU usage as a percentage

        cpu_usage_data.insert(
            "cpu_usage".to_string(),
            JsonValue::String(cpu_usage_formatted),
        );
        cpu_usage_data.insert(
            "description".to_string(),
            JsonValue::String(format!(
                "CPU usage for component {}",
                get_first_part_after_dash(id)
            )),
        );
        cpu_usage_data.insert("name".to_string(), JsonValue::String("CPU".to_string()));

        resources.insert("cpu".to_string(), JsonValue::Object(cpu_usage_data));
    }

    if let Some((total_memory_kb, used_memory_kb)) = get_system_memory_usage() {
        let total_memory_as_json_number = JsonValue::Number(total_memory_kb.into());
        let used_memory_as_json_number = JsonValue::Number(used_memory_kb.into());

        let mut memory_usage_data = Map::new();
        memory_usage_data.insert(
            "description".to_string(),
            JsonValue::String(format!(
                "CPU usage for component {}",
                get_first_part_after_dash(id)
            )),
        );
        memory_usage_data.insert("total_memory_mb".to_string(), total_memory_as_json_number);
        memory_usage_data.insert("used_memory_mb".to_string(), used_memory_as_json_number);

        resources.insert("memory".to_string(), JsonValue::Object(memory_usage_data));
    }

    response_data.insert("resources".to_string(), JsonValue::Object(resources));

    let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
        id: id.to_string(),
        data: to_value(response_data).expect("Failed to handle all system resource"),
        errors: None,
        schema: None,
    };

    EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value)
}

pub async fn gateway_request(
    uri: String,
    method: Method,
    headers: HeaderMap,
    body: Option<Body>,
) -> Result<Response<Body>, Box<dyn Error + Send + Sync>> {
    // Configure HTTP connector with timeout
    let mut http_connector = HttpConnector::new();
    http_connector.set_connect_timeout(Some(Duration::from_secs(30)));
    let client = Client::builder().build::<_, Body>(http_connector);

    // Calculate header size and add it to the request
    let mut request_builder = Request::builder();

    // Add header
    for (key, value) in headers.iter() {
        request_builder = request_builder.header(key, value);
    }

    // Create request
    let request = match method {
        Method::GET => {
            // No body is required for GET requests
            request_builder
                .method(method)
                .uri(&uri)
                .body(Body::empty())? // Empty body for GET requests
        }
        _ => {
            // Extract body content
            let request_body = body.unwrap_or_else(Body::empty);

            let body_bytes = hyper::body::to_bytes(request_body).await?;
            let size = body_bytes.len();

            info!("Genau Größe des request_body: {} Bytes", size);

            // Check if body is actually present
            if body_bytes.is_empty() {
                return Err("Body content is empty.".into());
            }

            // Create request with body
            request_builder
                .method(method)
                .uri(&uri)
                .header(hyper::header::CONTENT_LENGTH, body_bytes.len().to_string())
                .body(Body::from(body_bytes))?
        }
    };

    // Send request
    let response = timeout(Duration::from_secs(30), client.request(request)).await??;

    // Output response status and return the response
    info!("Received response status: {:?}", response.status());
    Ok(response)
}

use std::net::ToSocketAddrs;
pub async fn is_host_available(host: &str, port: u16) -> bool {
    if TcpStream::connect_timeout (
        &(host, port).to_socket_addrs().unwrap().next().unwrap(),
        Duration::from_secs(5),
    ).is_ok() {
        info!("Verbindung zu {}:{} erfolgreich.", host, port);
        true // Connection successful -> Port available
    } else {
        info!("Verbindung zu {}:{} fehlgeschlagen.", host, port);
        false // Connection failed -> Port not available
    }
}

pub fn update_href_with_base_uri(json_value: &mut Value, base_uri: &str) {
    // Check if it is an array
    if let Some(items_array) = json_value.get_mut("items").and_then(|v| v.as_array_mut()) {
        // Iterate over each element in the "items" array
        for item in items_array.iter_mut() {
            // Check if the element is an object and contains the "href" field
            if let Some(obj) = item.as_object_mut() &&
             let Some(href_value) = obj.get("href").and_then(|v| v.as_str()) {
                    // Check if the "href" matches the base URI
                    if !href_value.starts_with(base_uri) {
                        // Replace the non-matching part of the URI with the base URI
                        let updated_href = format!("{}{}", base_uri, href_value);
                        // Replace the value of the "href" field in the object
                        obj.insert("href".to_string(), Value::String(updated_href));
                    }
                }
            
        }
    }
}

pub fn extract_response_data(
    response: EntityCollectionEntityIdGet200Response,
) -> HashMap<String, Option<String>> {
    let mut data = HashMap::new();

    data.insert("id".to_string(), Some(response.id));
    data.insert("name".to_string(), Some(response.name));
    data.insert("translation_id".to_string(), response.translation_id);
    data.insert(
        "variant".to_string(),
        response.variant.map(|v| format!("{:?}", v)),
    );
    data.insert("configurations".to_string(), response.configurations);
    data.insert("bulk_data".to_string(), response.bulk_data);
    data.insert("data".to_string(), response.data);
    data.insert("data_lists".to_string(), response.data_lists);
    data.insert("faults".to_string(), response.faults);
    data.insert("operations".to_string(), response.operations);
    data.insert("updates".to_string(), response.updates);
    data.insert("modes".to_string(), response.modes);
    data.insert("relatedapps".to_string(), response.relatedapps);
    data.insert("relatedcomponents".to_string(), response.relatedcomponents);
    data.insert("subareas".to_string(), response.subareas);
    data.insert("subcomponents".to_string(), response.subcomponents);
    data.insert("locks".to_string(), response.locks);
    data.insert("logs".to_string(), response.logs);

    data
}

pub fn extract_response_data_from_json(
    json_value: &mut JsonValue,
) -> HashMap<String, Option<String>> {
    let mut data = HashMap::new();

    if let JsonValue::Object(obj) = json_value {
        // Extract each field from the JsonValue and add it to the HashMap
        for (key, value) in obj.iter_mut() {
            if let Some(string_value) = value.as_str() {
                data.insert(key.clone(), Some(string_value.to_string()));
            } else {
                data.insert(key.clone(), None);
            }
        }
    }

    data
}

pub fn extract_response_data_from_json_to_response(
    json_value: &mut Value,
    base_uri: &str,
) -> EntityCollectionEntityIdGetResponse {
    // Check if it is an object
    if let Some(object) = json_value.as_object() {
        // Extract the values from the JSON object
        let id = object.get("id").and_then(|id| id.as_str());
        let name = object.get("name").and_then(|name| name.as_str());

        // Check if the required fields are present
        if let (Some(id), Some(name)) = (id, name) {
            // Create an InlineResponse2002 instance with the available fields
            let mut response =
                EntityCollectionEntityIdGet200Response::new(id.to_string(), name.to_string());

            // Iterate over the fields in the JSON object and add them to the response if they contain values
            for (field_name, field_value) in object.iter() {
                if let Some(field_value_str) = field_value.as_str() {
                    // Format the value using the base URI and the corresponding path
                    let formatted_value = format!(
                            "{}/apps/{}/{}",
                            base_uri,
                            id,
                            match field_name.as_str() {
                                "data" => "data",
                                "configurations" => "configuration",
                                "bulk_data" => "bulk-data",
                                // add other fields here
                                _ => "",
                            }
                        );
                    

                    // Add the field to the response if the value is not empty
                    if !field_value_str.is_empty() {
                        match field_name.as_str() {
                            "data" => response.data = Some(formatted_value),
                            "configurations" => response.configurations = Some(formatted_value),
                            "bulk_data" => response.bulk_data = Some(formatted_value),
                            // Add more fields here
                            _ => {} // Ignore unknown fields
                        }
                    }
                }
            }

            // Check if at least one value is present in the response
            if response.data.is_some()
                || response.configurations.is_some()
                || response.bulk_data.is_some()
            // Add more fields here
            {
                // Return of the created EntityCollectionEntityIdGetResponse
                EntityCollectionEntityIdGetResponse::TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response)
            } else {
                // Return None if no values are present in the response
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "EntityNotFound".to_string(),
                    message: "Entity not found.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error)
            }
        } else {
            // Return None if the required fields are missing
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "EntityNotFound".to_string(),
                message: "Entity not found.".to_string(),
                vendor_code: None,
                translation_id: None,
                parameters: None,
            };
            EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error)
        }
    } else {
        // Return None if it is not an object
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "EntityNotFound".to_string(),
            message: "Entity not found.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };
        EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error)
    }
}

pub fn resolve_hostname(hostname: &str) -> Result<String, std::io::Error> {
    let addr = (hostname, 0).to_socket_addrs()?.next().ok_or_else(|| {
        std::io::Error::other("Hostname resolution failed")
    })?;
    Ok(addr.ip().to_string())
}
