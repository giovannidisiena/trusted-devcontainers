FROM debian:bookworm-slim

ARG VERSION
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG USERNAME=auditor
ARG USER_UID=1000
ARG USER_GID=1000

LABEL org.opencontainers.image.title="trusted/base" \
      org.opencontainers.image.description="Minimal trusted base image for isolated client workspaces" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}"

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/home/${USERNAME}/.local/bin:${PATH}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       bash \
       ca-certificates \
       curl \
       file \
       git \
       gnupg \
       jq \
       less \
       make \
       openssh-client \
       procps \
       ripgrep \
       tar \
       unzip \
       vim-tiny \
       wget \
       xz-utils \
       zstd \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid "${USER_GID}" "${USERNAME}" \
    && useradd --uid "${USER_UID}" --gid "${USER_GID}" --create-home --shell /bin/bash "${USERNAME}" \
    && mkdir -p /workspace "/home/${USERNAME}/.local/bin" \
    && chown -R "${USERNAME}:${USERNAME}" /workspace "/home/${USERNAME}/.local"

USER ${USERNAME}
WORKDIR /workspace
