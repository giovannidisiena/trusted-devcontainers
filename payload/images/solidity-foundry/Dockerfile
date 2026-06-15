ARG IMAGE_NAMESPACE=trusted
ARG VERSION
FROM ${IMAGE_NAMESPACE}/base:${VERSION}

ARG VERSION
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG FOUNDRY_VERSION=1.0.0
ARG SOLC_SELECT_VERSION=1.0.4

LABEL org.opencontainers.image.title="trusted/solidity-foundry" \
      org.opencontainers.image.description="Trusted Solidity profile with Foundry and solc-select only" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}"

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       python3 \
       python3-venv \
    && rm -rf /var/lib/apt/lists/*

COPY images/scripts/install-foundry.sh /tmp/install-foundry.sh
COPY images/scripts/install-solc-select.sh /tmp/install-solc-select.sh
RUN chmod +x /tmp/install-foundry.sh /tmp/install-solc-select.sh \
    && FOUNDRY_VERSION="${FOUNDRY_VERSION}" /tmp/install-foundry.sh \
    && SOLC_SELECT_VERSION="${SOLC_SELECT_VERSION}" /tmp/install-solc-select.sh \
    && rm -f /tmp/install-foundry.sh /tmp/install-solc-select.sh

USER auditor
WORKDIR /workspace
