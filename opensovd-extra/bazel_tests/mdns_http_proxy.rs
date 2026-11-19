// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#[test]
fn mdns_and_http_proxy_can_be_used_together() {
    let announcement = opensovd_extra_mdns::Announcement::new(([127, 0, 0, 1], 7690).into());
    let _: Option<opensovd_extra::UpstreamProxy> = None;
    let _ = announcement;
}
