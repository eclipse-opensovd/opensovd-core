# FAQ

**Q: What is the purpose of OpenSOVD Core?**
A: It provides a unified diagnostic interface for in-vehicle components using the SOVD standard.

**Q: What modes are supported?**
A: Gateway and Standalone.

**Q: How do I configure the server?**
A: Use the `sovd_server_apps.conf` file and pass parameters via CLI.

**Q: How do I build the project?**
A: Run bash script build.sh start

**Q: How do I start the server in Standalone mode?**
A: cargo run 127.0.0.1 8000 chassis-hpc --sovd-mode standalone.
