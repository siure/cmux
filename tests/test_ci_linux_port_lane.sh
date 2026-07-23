#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CI_FILE="$ROOT_DIR/.github/workflows/ci.yml"

job_section() {
  local job="$1"
  awk -v job="$job" '
    $0 ~ "^  "job":" { in_job=1; next }
    in_job && /^  [^[:space:]#][^:]*:[[:space:]]*(#.*)?$/ { exit }
    in_job { print }
  ' "$CI_FILE"
}

section="$(job_section linux-port-tests)"
if [[ -z "$section" ]]; then
  echo "FAIL: CI must define the linux-port-tests job" >&2
  exit 1
fi

require_token() {
  local token="$1" message="$2"
  if [[ "$section" != *"$token"* ]]; then
    echo "FAIL: $message" >&2
    exit 1
  fi
}

require_token "runs-on: ubuntu-latest" \
  "the Linux port lane must run on an Ubuntu host"
require_token 'RUST_VERSION: "1.92.0"' \
  "the Linux port lane must pin its validated Rust toolchain"
require_token "libgtk-4-dev libwebkitgtk-6.0-dev pkg-config xauth xvfb" \
  "the Linux port lane must install GTK4 and Xvfb development/runtime dependencies"
require_token "libwebkitgtk-6.0-dev pkg-config xauth xvfb dbus-x11" \
  "the Linux port lane must install WebKitGTK and a D-Bus session helper"
require_token 'rustup toolchain install "$RUST_VERSION" --profile minimal --component rustfmt' \
  "the Linux port lane must provision Rust and rustfmt explicitly"
require_token "cargo fmt --all -- --check" \
  "the Linux port lane must enforce Rust formatting"
require_token "cargo test --locked" \
  "the Linux port lane must test the display-free core with Cargo.lock"
require_token "cargo test --locked --features gtk" \
  "the Linux port lane must test the GTK app with Cargo.lock"
require_token "--example webkit-cookie-smoke" \
  "the Linux port lane must exercise native WebKit cookie persistence"
require_token "--example webkit-runtime-smoke" \
  "the Linux port lane must exercise native WebKit user-agent and init-script persistence"
require_token "cargo build --locked --features gtk" \
  "the Linux port lane must build the executable used by the GTK smoke test"
require_token "timeout 30s xvfb-run -a target/debug/cmux app --renderer gtk" \
  "the Linux port lane must launch the native GTK app under a bounded Xvfb session"
require_token "XDG_STATE_HOME=\"\$smoke_root/state\"" \
  "the GTK smoke test must use isolated app state"
require_token "split right" \
  "the GTK smoke test must exercise native split reconciliation"
require_token "pane=pane:2 surface=surface:2" \
  "the GTK smoke test must verify focus moved to the new pane"
require_token "grep -Fx 'bye'" \
  "the GTK smoke test must verify clean scripted shutdown"
require_token 'app --renderer gtk --socket "$socket"' \
  "the GTK smoke test must launch a socket-owned native app"
require_token 'rpc app.quit.request "{}"' \
  "the GTK smoke test must request shutdown from a second process"
require_token 'while kill -0 "$app_pid"' \
  "the GTK smoke test must verify that the native app process exits"
require_token "bash -n linux/scripts/install-dev.sh" \
  "the Linux port lane must syntax-check the installer"
require_token "bash -n linux/scripts/build-bundle.sh" \
  "the Linux port lane must syntax-check the bundle builder"
require_token "sh -n linux/dist/cmux-linux-app" \
  "the Linux port lane must syntax-check the POSIX launcher"
require_token "sh -n linux/dist/install-bundle.sh" \
  "the Linux port lane must syntax-check the bundle installer"
require_token "./tests/test_linux_bundle.sh" \
  "the Linux port lane must verify that the release bundle is relocatable and installable"

if ! grep -Fq "./tests/test_ci_linux_port_lane.sh" "$CI_FILE"; then
  echo "FAIL: workflow-guard-tests must invoke the Linux port lane guard" >&2
  exit 1
fi

echo "PASS: Linux port CI lane covers locked headless and GTK tests"
