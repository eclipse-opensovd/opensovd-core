# Service Discovery Design

This document describes the service discovery library delivered by OpenSOVD and
its default mDNS implementation.

## Goal

The gateway is the SOVD endpoint reachable from the infotainment or internet-facing side of the vehicle. Private HPCs or services may live on an in-car private network that infotainment cannot route to directly. mDNS helps the reachable side find the gateway and helps the gateway discover private-side services.

```text
Infotainment / external network
        |
        | discovers and calls gateway SOVD URL
        v
OpenSOVD Gateway
        |
        | discovers private SOVD services on in-car network
        v
Private HPC / ECU / service
```

The public client should call the gateway. It should not need direct network access to private HPC addresses.

## Service Discovery Library

`opensovd-providers` exposes a transport-agnostic service discovery lifecycle
for integrating the gateway with different platform mechanisms:

| Type | Responsibility |
|------|----------------|
| `ServiceDiscovery` | Starts a transport-specific discovery mechanism |
| `ServiceDiscoverySession` | Owns the running transport, supplies a `DiscoveryProvider`, and shuts it down |
| `DiscoveryProvider` | Produces topology changes for services that appear or disappear |

The contract does not prescribe the transport. An integrating project can
implement `ServiceDiscovery` for mDNS, IPC, D-Bus, or another platform-specific
mechanism. Each implementation converts its native events into the existing
`DiscoveryProvider` topology stream.

OpenSOVD uses `MdnsServiceDiscovery` as its default implementation. It is based
on `mdns-sd` and provides both private-side service discovery and the mDNS
operations required by the reference gateway.

The architecture is shown in
[service_discovery_architecture.puml](service_discovery_architecture.puml).

## mDNS Roles

### Gateway Advertisement

When the gateway starts with the `mdns` feature and `--mdns`, it creates an mDNS service record for itself using the `_sovd._tcp.local.` service type. The record contains:

| Field | Meaning |
|-------|---------|
| instance name | Human-readable service instance, configured with `--mdns-name` |
| host and port | The gateway address and port that infotainment can reach |
| `identification` TXT | VIN, device id, or deployment-specific gateway identity |
| `accessurl` TXT | The SOVD base URL for the gateway, such as `http://192.168.1.10:7690/sovd` |

The advertised URL is always the gateway URL. It must not be a private HPC URL.

The gateway refuses to advertise unspecified or loopback IPs such as `0.0.0.0` or `127.0.0.1`. When binding to `0.0.0.0`, pass the infotainment-facing address explicitly:

```bash
opensovd-gateway \
  --url http://0.0.0.0:7690/sovd \
  --mdns \
  --mdns-host 192.168.1.10 \
  --mdns-name vehicle-gateway
```

If TLS is enabled for the gateway listener, the advertised `accessurl` uses `https`.

The gateway uses the actual bound TCP port in its service record, including an
OS-assigned port when `--url` uses port `0`. With a Unix socket listener, mDNS
discovery remains enabled but gateway advertisement is skipped because there is
no TCP endpoint for mDNS clients to call.

### Gateway Private Discovery

The gateway can also browse `_sovd._tcp.local.` services on the network interfaces available to the process. This is used to notice private SOVD services on the in-car network.

Discovered private services are converted into topology components so the gateway can represent that something exists behind it. The private service's `accessurl` TXT record is intentionally not exposed as public component metadata. That address may only be reachable from the gateway, and leaking it to infotainment would create a misleading or unusable public API.

The mDNS provider filters out services registered by the same process to avoid adding the gateway as its own discovered component.

## Default Implementation

The default mDNS implementation has two pieces:

| Location | Responsibility |
|----------|----------------|
| `opensovd-cli/gateway/src/mdns.rs` | Gateway-specific mDNS setup: choose advertised host, scheme, port, and base URL |
| `opensovd-providers/src/service_discovery.rs` | Transport-agnostic service discovery lifecycle traits |
| `opensovd-providers/src/mdns.rs` | `MdnsServiceDiscovery`, mDNS wrapper, and discovery provider using `mdns-sd` |

The gateway starts `MdnsServiceDiscovery`, retains its
`MdnsServiceDiscoverySession` for the server lifetime, and obtains an
`MdnsDiscoveryProvider` through `ServiceDiscoverySession`. The session owns one
`MdnsWrapper`, so registration and browsing share the same mDNS daemon.

Gateway advertising is intentionally separate from generic discovery. mDNS can
advertise the public gateway endpoint, while an IPC or D-Bus implementation may
only discover private services. Alternative discovery mechanisms therefore do
not need to implement an mDNS-style network advertisement.

## What mDNS Does Not Do Yet

The mDNS provider does not currently implement full SOVD federation or proxying. In particular, it does not yet:

- connect to a discovered private HPC;
- fetch that HPC's `/sovd/v1/components`, `/apps`, or `/areas`;
- attach remote data providers to gateway topology entities;
- forward read/write/operation requests from infotainment through the gateway to the private HPC.

That proxy layer should be designed separately. It will likely need an internal remote SOVD client/provider that keeps private URLs inside the gateway and exposes only gateway-owned SOVD resource URLs externally.

## Expected Behavior

With the current implementation:

1. Gateway starts with `--mdns` and advertises itself.
2. Infotainment discovers the gateway mDNS service.
3. Infotainment calls the gateway's advertised `accessurl`.
4. Gateway may discover private `_sovd._tcp.local.` services.
5. Gateway adds discovered private services as internal topology components without exposing their private access URLs.

This is enough for gateway presence discovery and private-service awareness. It is not yet a full transparent SOVD bridge.

## Design Rules

- Public mDNS advertisement must point to the gateway, not private HPCs.
- Do not advertise `0.0.0.0`, loopback, or other addresses clients cannot use.
- Do not expose private-side access URLs in public SOVD responses.
- Keep mDNS discovery separate from request proxying.
- Treat private HPC routing as gateway-internal state.
