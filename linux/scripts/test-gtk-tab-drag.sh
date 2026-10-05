#!/usr/bin/env bash
set -euo pipefail

linux_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$linux_dir/target/debug/cmux}"
if [[ $# -gt 0 ]]; then shift; fi

exec xvfb-run -a -s '-screen 0 1400x900x24' dbus-run-session -- \
  python3 "$linux_dir/tests/gtk_tab_drag.py" "$binary" "$@"
