ARG IMAGE_NAMESPACE=trusted
ARG VERSION
FROM ${IMAGE_NAMESPACE}/solidity-foundry:${VERSION}

ARG VERSION
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG NODE_VERSION=22.11.0
ARG PNPM_VERSION=9.15.4

LABEL org.opencontainers.image.title="trusted/solidity-foundry-node" \
      org.opencontainers.image.description="Trusted Solidity profile with Foundry, solc-select, Node.js, and pnpm" \
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
