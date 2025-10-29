#[cfg(test)]
mod sovd_handlers_tests {
    use std::collections::BTreeMap;

    use openapi_client::models::EntityCollectionGet200ResponseItemsInner;
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};

    #[test]
    fn find_single_process() {
        let base_uri = String::from("http://127.0.0.1:8080/v1");
        let process_name = String::from("systemd-journal");

        let system = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );

        let mut processes: BTreeMap<u32, &sysinfo::Process> = BTreeMap::new();
        for process in system.processes_by_exact_name(&process_name) {
            processes.insert(process.pid().as_u32(), process);
        }
        assert_ne!(processes.len(), 0);

        for process in processes {
            let mut id = String::new();
            id.push_str(&process_name);
            id.push_str("-");
            id.push_str(&process.0.to_string());

            let mut href = String::new();
            href.push_str(&base_uri);
            href.push_str("/apps/");
            href.push_str(&process_name);
            href.push('-');
            href.push_str(&process.0.to_string());

            let result = sovd_handlers::find_single_process(
                &process_name,
                &process.0.to_string(),
                &base_uri,
            );
            let expected_result =
                EntityCollectionGet200ResponseItemsInner::new(id, process_name.to_string(), href);
            assert_eq!(result.unwrap(), expected_result);
        }
    }
}
