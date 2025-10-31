# Configuration

The main configuration file is `sovd_server_apps.conf`. It defines operational parameters for the SOVD server.

Ensure the following fields are correctly set:
- component_id 
    your ECU (e.g. telematics or chassis-hpc)
- apps
    where to run your component (e.g. sovd-server)
- instance_name
    currently unused
- mode
    gateway or standalone
