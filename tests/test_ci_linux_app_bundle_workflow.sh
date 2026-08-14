#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
workflow="$root_dir/.github/workflows/build-linux-app.yml"
ci_workflow="$root_dir/.github/workflows/ci.yml"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "Linux desktop bundle workflow is missing"
contents=$(cat "$workflow")

require_token() {
    local token=$1
    local message=$2
    [[ "$contents" == *"$token"* ]] || fail "$message"
}

require_token "workflow_dispatch:" \
    "Linux desktop bundles must be manually dispatchable before release integration"
require_token "workflow_call:" \
    "stable and nightly workflows must be able to reuse the Linux bundle builder"
require_token "cmux_ref:" \
    "release callers must be able to pin the exact cmux commit"
require_token 'ref: ${{ inputs.cmux_ref || github.sha }}' \
    "the workflow must build the caller-selected cmux commit"
require_token "git submodule sync -- ghostty" \
    "the workflow must synchronize Ghostty configuration from the checked-out cmux revision"
require_token "git submodule update --init --depth 1 ghostty" \
    "the workflow must check out the Ghostty submodule pinned by cmux"
require_token 'pinned_ref="$(git ls-tree HEAD ghostty | awk' \
    "the workflow must read the Ghostty gitlink from the checked-out cmux revision"
require_token 'ghostty_repository="$(git config -f .gitmodules --get submodule.ghostty.url)"' \
    "the workflow must derive the Ghostty repository from synchronized submodule configuration"
require_token 'test "$ghostty_ref" = "$pinned_ref"' \
    "the workflow must reject a Ghostty checkout that differs from the cmux gitlink"
require_token 'echo "repository=$ghostty_repository" >> "$GITHUB_OUTPUT"' \
    "the workflow must pass the synchronized Ghostty repository to later steps"
require_token 'RUST_VERSION: "1.92.0"' \
    "the workflow must pin the validated Rust version"
require_token 'ZIG_VERSION: "0.16.0"' \
    "the workflow must pin the validated Zig version"
require_token 'ZIG_SHA256: "70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00"' \
    "the downloaded Zig archive must have a pinned checksum"
require_token "libgtk-4-dev" \
    "the workflow must install GTK development files"
require_token "libwebkitgtk-6.0-dev" \
    "the workflow must install WebKitGTK development files"
require_token 'CMUX_GHOSTTY_CHECKOUT="$GITHUB_WORKSPACE/ghostty"' \
    "the bundle build must use the checked-out Ghostty submodule"
require_token 'CMUX_LINUX_BUNDLE_GHOSTTY_REPOSITORY="$GHOSTTY_REPOSITORY"' \
    "the archive must record the repository derived from the cmux revision"
require_token "./linux/scripts/build-bundle.sh" \
    "the workflow must use the validated relocatable bundle builder"
require_token "sha256sum -c" \
    "the workflow must verify the generated archive checksum"
require_token "cmux.linux-bundle.provenance.v1" \
    "the workflow must record source and toolchain provenance"
require_token "git -C ghostty rev-parse HEAD" \
    "provenance must contain the resolved Ghostty commit"
require_token "cmux_dirty: \$cmux_dirty" \
    "provenance must state whether cmux sources were dirty"
require_token "ghostty_dirty: \$ghostty_dirty" \
    "provenance must state whether Ghostty sources were dirty"
require_token "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a" \
    "the workflow must upload the Linux desktop artifact with a pinned action"
require_token "dist/cmux-linux-build.json" \
    "the uploaded artifact must include its provenance manifest"

grep -Fq "./tests/test_ci_linux_app_bundle_workflow.sh" "$ci_workflow" || \
    fail "the main CI workflow must run the Linux bundle workflow guard"

[[ "$contents" != *"siure/ghostty"* ]] || \
    fail "the workflow must not hardcode a personal Ghostty fork"

printf 'PASS: Linux desktop bundle workflow pins toolchains, Ghostty, provenance, and artifacts\n'
