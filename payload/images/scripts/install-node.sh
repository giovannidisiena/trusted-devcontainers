#!/usr/bin/env bash
set -euo pipefail

: "${NODE_VERSION:?NODE_VERSION is required}"
: "${PNPM_VERSION:?PNPM_VERSION is required}"

arch="$(dpkg --print-architecture)"
case "${arch}" in
  amd64)
    node_arch="x64"
    ;;
  arm64)
    node_arch="arm64"
    ;;
  *)
    echo "Unsupported architecture for Node.js install: ${arch}" >&2
    exit 1
    ;;
esac

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

cd "${tmpdir}"
base_url="https://nodejs.org/dist/v${NODE_VERSION}"
archive="node-v${NODE_VERSION}-linux-${node_arch}.tar.xz"

curl -fsSLO "${base_url}/${archive}"
curl -fsSLO "${base_url}/SHASUMS256.txt"
grep " ${archive}$" SHASUMS256.txt | sha256sum -c -

tar -xJf "${archive}" -C /usr/local --strip-components=1
corepack enable
corepack prepare "pnpm@${PNPM_VERSION}" --activate

npm config set fund false --global
npm config set audit false --global

node --version
npm --version
pnpm --version

