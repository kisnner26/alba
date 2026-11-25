#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
make build disk >&2
python3 -B tools/recursos.py "$@"
