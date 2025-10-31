# Testing the Server

You can test the server availability using `cargo run` or by starting multiple instances in different modes.

Example:
Start one instance in standalone mode and another in gateway mode to simulate HPC communication.

# Get components list
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components
        Response: {
            "items":[{"id":"chassis-hpc","name":"Chassis-HPC","href":"http://127.0.0.1:8000/v1/components/chassis-hpc"}]
            }

# Get details of a specific component
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components/chassis-hpc
        Response: {
                    "id":"chassis-hpc","name":"Chassis-HPC","data":"http://127.0.0.1:8000/v1/components/chassis-hpc/data"
                    }

# Get data for a specific category of a component
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components/chassis-hpc/data/chassis-hpc-cpu
        Response: {
            "id":"chassis-hpc-cpu","data":{"cpu_usage":"4.73%","description":"CPU usage for component chassis-hpc","name":"CPU"}
            }

# Get related apps for a specific component (e.g. chassis-hpc)
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components/chassis-hpc/related-apps
        Response: {
            {"id":"sovd-server-8572","name":"sovd-server","href":"http://127.0.0.1:8000/v1/apps/sovd-server-8572"}
        }

# Get details of a specific app (e.g., sovd_server)
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/apps/sovd-server-8572
        Response: {
            {"id":"sovd-server-8572","name":"sovd-server","data":"http://127.0.0.1:8000/v1/apps/sovd-server-8572/data}
        }

# Get data for a specific app (e.g., sovd_server)
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/apps/sovd-server-8572/data
        Response: {
            {"items":[{"id":"cpu","name":"Current CPU usage for apps sovd-server","category":"sysInfo"},
            {"id":"disk","name":"Current Disk usage for apps sovd-server","category":"sysInfo"},
            {"id":"memory","name":"Current Memory usage for apps sovd-server","category":"sysInfo"},
            {"id":"all","name":"Current All usage for apps sovd-server","category":"sysInfo"}]}
        }

# Get data for a specific category of an app (e.g., CPU data for sovd_server)
    curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/apps/sovd-server-8572/data/sovd-server-8572-cpu
        Response: {
            {"id":"sovd-server-8572-cpu","data":{"cpu_usage":"0.00%","description":"CPU usage for sovd-server","name":"CPU"}}
        }
