# Introduction 

This repository will contain the In-Vehicle SOVD Gateway and Server Implementation of the Eclipse OpenSOVD project and its documentation.

In the SOVD (Service-Oriented Vehicle Diagnostics) context, the In-Vehicle SOVD Gateway and Server provides a central point for handling diagnostic requests within the vehicle. It exposes standardized SOVD interfaces to clients (such as diagnostic tools or cloud services) while coordinating access to multiple ECUs through various adapters (for e.g. classic UDS adapters or newer vehicle-specific protocols).

The gateway receives SOVD-REST requests, authenticates and routes them to the appropriate in-vehicle components, and aggregates responses before returning them to the client. 

# Goals:

Provide a unified, in-vehicle SOVD interface for all diagnostic clients.

Translate and route SOVD requests to the appropriate adapters or ECUs.

Aggregate and standardize responses from other HPCs.


# Getting Started
The source code has parts generated with OpenAPI generator (https://github.com/OpenAPITools/openapi-generator )

# Notes 
- The software is in beta development. Please be aware of unexpected changes.
- Due to copyright, the sovd schema files cannot be provided. The user is responsible get it.
 
 Please read the schema [Notice](sovd-interfaces/NOTICE)

# Build Instructions
Currently builds on Linux x86 are supported.

Build:
```
./build.sh start
```

Clean:
```
./build.sh clean
```
# Starting sovd_server
```
Copy from config folder sovd_server_apps.conf to target/debug/

sovd_server <ip_address> <port> <hostname> --sovd-mode <sovd_mode>
<ip_address> : IP address to bind the server to, use localhost for local testing or a public IP for external access.
<port>       : Any available port in the range 1024-49151, typically used by user applications and services.
<hostname>   : A desriptive name e.g. chassis-hpc.
<sovd_mode>  : Operation mode either gateway or standalone.
<logs>       : Optional configure saving location for log file.

e.g. ./sovd_server <ip-address> <port> chassis-hpc --sovd_mode standalone
e.g. ./sovd_server <ip-address> <port> telematics --sovd_mode gateway
```

# Testing sovd_server availability

In this example we use cargo run, and we will start two instances of HPC, one as standalone and another one as gateway that are running in background.

 - cargo run 127.0.0.1 8000 chassis-hpc --sovd-mode standalone &
 - cargo run 127.0.0.2 8001 telematics --sovd-mode gateway &
 - check if servers are online with command jobs in terminal

Then following specifications start to interogate sovd_server for CPU consumption for component.

1. Get components list
      Specification example: curl --noproxy '*' -X GET http://<your_host>:<your_port>/v1/components
      Actual example:        curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components
                              Response: {
                                    "items":[{"id":"chassis-hpc","name":"Chassis-HPC","href":"http://127.0.0.1:8000/v1/components/chassis-hpc"}]
                                    }
                              The response contain href for next specific component command.

2. Get details of a specific component
      Specification example: curl --noproxy '*' -X GET http://<your_host>:<your_port>/v1/components/<component_id>
      Actual example:        curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components/chassis-hpc
                              Response: {
                                          "id":"chassis-hpc","name":"Chassis-HPC","data":"http://127.0.0.1:8000/v1/components/chassis-hpc/data"
                                          }
                              The response contain href for next specific component command.

3. Get data for a specific category of a component
      Specification example: curl --noproxy '*' -X GET http://<your_host>:<your_port>/v1/components/<component_id>/data/<data_id>
      Actual example:        curl --noproxy '*' -X GET http://127.0.0.1:8000/v1/components/chassis-hpc/data/chassis-hpc-cpu
                              Response: {
                                    "id":"chassis-hpc-cpu","data":{"cpu_usage":"4.73%","description":"CPU usage for component chassis-hpc","name":"CPU"}
                                    }
                              The response contain actual value of CPU load.


#NOTE: 
   <data_id>-cpu: This resource provides information on the current CPU usage for the specified <data_id>.
   <data_id>-disk: This resource provides information on the current disk usage for the specified <data_id>.
   <data_id>-memory: This resource provides information on the current memory usage for the specified <data_id>.
   <data_id>-all: This resource provides a comprehensive overview of all available resources (CPU, Disk, Memory) for the specified <data_id>.
   The same applies to components.

   Please note to replace <data_id> with the appropriate identifier or ID for the data or component you wish to access.


   # Test integration

   1. Run command: cargo test -p sovd-server