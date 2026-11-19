<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Local systemd journal logs

This example attaches a `JournalctlLogProvider` to a systemd app (logs for
one unit) and a component (the entire **local** journal). It uses the generic
SOVD log routes implemented by the server; no log storage is added to SOVD.
Run on the Linux device whose journal you want to expose. `journalctl` must
be installed, the server process must have permission to read the relevant
entries, and the application must write its logs to journald.

```bash
SOVD_JOURNAL_UNIT=save-the-spoiler.service \
  cargo run -p opensovd-examples-server --features journalctl --example journal

curl -s http://127.0.0.1:7691/sovd/v1/apps/save-the-spoiler/logs
curl -s http://127.0.0.1:7691/sovd/v1/apps/save-the-spoiler/logs/entries
curl -s http://127.0.0.1:7691/sovd/v1/components/local-ecu/logs/entries
curl -N http://127.0.0.1:7691/sovd/v1/apps/save-the-spoiler/logs/x-opensovd-log-streams
```

Set `SOVD_JOURNAL_APP_ID` to change the app ID (otherwise the `.service`
suffix is removed from the unit name). Set `SOVD_JOURNAL_COMPONENT_ID` to
name the component, for example `proconnect` when running **on ProConnect**.
The component's journal includes all entries readable by the server, not just
the app's unit. Configure access control before exposing it beyond localhost:
system logs may contain sensitive information.

The read-only provider supports severity and `created-after`/`created-before`
filters for both snapshots and streams. A stream with `created-before` ends
after its snapshot. The `/logs/config` read returns an empty list; PUT and
DELETE configuration requests are unsupported. Binary journal messages are
decoded as UTF-8 with replacement characters for invalid bytes; journal
entries without a `MESSAGE` field are omitted. To read logs from a *remote*
ECU, deploy the server on that ECU or supply a separate provider/transport
for its journal; renaming the local component does not connect to that ECU.
