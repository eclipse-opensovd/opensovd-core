# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

FROM gcr.io/distroless/cc-debian13:nonroot
ARG TARGETARCH

COPY target/release-small/${TARGETARCH}/opensovd-gateway /usr/local/bin/opensovd-gateway

EXPOSE 7690

ENV SOVD_URL=http://0.0.0.0:7690/sovd
ENTRYPOINT ["/usr/local/bin/opensovd-gateway"]
