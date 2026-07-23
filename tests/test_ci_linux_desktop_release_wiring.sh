#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
release_workflow="$root_dir/.github/workflows/release.yml"
nightly_workflow="$root_dir/.github/workflows/nightly.yml"
asset_guard="$root_dir/scripts/release_asset_guard.js"
ci_workflow="$root_dir/.github/workflows/ci.yml"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

require_file_token() {
    local file=$1
    local token=$2
    local message=$3
    grep -Fq -- "$token" "$file" || fail "$message"
}

for workflow in "$release_workflow" "$nightly_workflow"; do
    require_file_token "$workflow" \
        'uses: ./.github/workflows/build-linux-app.yml' \
        "$(basename "$workflow") must call the validated Linux bundle workflow"
    require_file_token "$workflow" \
        'vars.CMUX_LINUX_GHOSTTY_REF' \
        "$(basename "$workflow") must use the configured immutable Linux Ghostty revision"
    require_file_token "$workflow" \
        'name: cmux-linux-desktop-${{ github.run_id }}' \
        "$(basename "$workflow") must download the Linux artifact from the reusable workflow"
    require_file_token "$workflow" \
        'linux-desktop/cmux-linux-x86_64.tar.gz' \
        "$(basename "$workflow") must publish the Linux x86_64 archive"
    require_file_token "$workflow" \
        'linux-desktop/cmux-linux-x86_64.tar.gz.sha256' \
        "$(basename "$workflow") must publish the Linux archive checksum"
    require_file_token "$workflow" \
        'linux-desktop/cmux-linux-build.json' \
        "$(basename "$workflow") must publish Linux source provenance"
    require_file_token "$workflow" \
        'actions/attest-build-provenance@a2bbfa25375fe432b6a289bc6b6cd05ecd0c4c32' \
        "$(basename "$workflow") must attest Linux release assets with the pinned action"
done

require_file_token "$release_workflow" \
    'cmux_ref: ${{ github.sha }}' \
    "stable Linux releases must build the tagged cmux commit"
require_file_token "$release_workflow" \
    'bundle_version: ${{ github.ref_name }}' \
    "stable Linux releases must record the release tag as their version"
require_file_token "$nightly_workflow" \
    'cmux_ref: ${{ needs.decide.outputs.head_sha }}' \
    "nightly Linux releases must build the same selected commit as macOS"
require_file_token "$nightly_workflow" \
    'bundle_version: nightly-${{ needs.decide.outputs.short_sha }}' \
    "nightly Linux releases must record the selected nightly commit"

for asset in \
    cmux-linux-x86_64.tar.gz \
    cmux-linux-x86_64.tar.gz.sha256 \
    cmux-linux-build.json
do
    require_file_token "$asset_guard" \
        "\"$asset\"" \
        "stable release reruns must treat $asset as immutable"
done

require_file_token "$ci_workflow" \
    './tests/test_ci_linux_desktop_release_wiring.sh' \
    "main CI must run the Linux desktop release wiring guard"

printf 'PASS: stable and nightly releases publish pinned, attested Linux desktop bundles\n'
