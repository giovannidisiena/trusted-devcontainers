#!/usr/bin/env bash
set -euo pipefail

: "${FOUNDRY_VERSION:?FOUNDRY_VERSION is required}"

export FOUNDRY_DIR=/opt/foundry
export PATH="${FOUNDRY_DIR}/bin:${PATH}"

mkdir -p "${FOUNDRY_DIR}"

curl -fsSL https://foundry.paradigm.xyz | bash
"${FOUNDRY_DIR}/bin/foundryup" --install "${FOUNDRY_VERSION}"

ln -sf "${FOUNDRY_DIR}/bin/forge" /usr/local/bin/forge
ln -sf "${FOUNDRY_DIR}/bin/cast" /usr/local/bin/cast
ln -sf "${FOUNDRY_DIR}/bin/anvil" /usr/local/bin/anvil
ln -sf "${FOUNDRY_DIR}/bin/chisel" /usr/local/bin/chisel

forge --version
cast --version
anvil --version
chisel --version

