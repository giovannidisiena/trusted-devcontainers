ARG IMAGE_NAMESPACE=trusted
ARG VERSION=0.1.0
FROM ${IMAGE_NAMESPACE}/base:${VERSION}

ARG VERSION=0.1.0
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG NODE_VERSION=22.11.0
ARG PNPM_VERSION=9.15.4

LABEL org.opencontainers.image.title="trusted/node" \
      org.opencontainers.image.description="Trusted Node.js profile without global project frameworks" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}"

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential \
       python3 \
    && rm -rf /var/lib/apt/lists/*

COPY images/scripts/install-node.sh /tmp/install-node.sh
RUN chmod +x /tmp/install-node.sh \
    && NODE_VERSION="${NODE_VERSION}" PNPM_VERSION="${PNPM_VERSION}" /tmp/install-node.sh \
    && rm -f /tmp/install-node.sh

USER auditor
WORKDIR /workspace

