#!/usr/bin/env bash
set -euo pipefail

: "${SOLC_SELECT_VERSION:?SOLC_SELECT_VERSION is required}"

python3 -m venv /opt/solc-select
/opt/solc-select/bin/pip install --no-cache-dir --upgrade pip setuptools wheel
/opt/solc-select/bin/pip install --no-cache-dir "solc-select==${SOLC_SELECT_VERSION}"

ln -sf /opt/solc-select/bin/solc-select /usr/local/bin/solc-select
ln -sf /opt/solc-select/bin/solc /usr/local/bin/solc

solc-select --version

