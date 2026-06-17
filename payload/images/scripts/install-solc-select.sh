#!/usr/bin/env bash
set -euo pipefail

: "${SOLC_SELECT_VERSION:?SOLC_SELECT_VERSION is required}"

python3 -m venv /opt/solc-select
/opt/solc-select/bin/pip install --no-cache-dir --upgrade pip setuptools wheel
/opt/solc-select/bin/pip install --no-cache-dir "solc-select==${SOLC_SELECT_VERSION}"

ln -sf /opt/solc-select/bin/solc-select /usr/local/bin/solc-select
ln -sf /opt/solc-select/bin/solc /usr/local/bin/solc

actual_version="$(
  /opt/solc-select/bin/python - <<'PY'
from importlib.metadata import version

print(version("solc-select"))
PY
)"

if [[ "${actual_version}" != "${SOLC_SELECT_VERSION}" ]]; then
  echo "installed solc-select ${actual_version}, expected ${SOLC_SELECT_VERSION}" >&2
  exit 1
fi

solc-select --help >/dev/null
