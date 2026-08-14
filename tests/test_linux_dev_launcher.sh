#!/usr/bin/env bash
set -euo pipefail

root_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/cmux-linux-dev-launcher-test.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

fixture=$tmp_dir/fixture
fake_bin=$tmp_dir/bin
capture=$tmp_dir/capture
git_state=$tmp_dir/git-state
expected_ghostty_commit=1111111111111111111111111111111111111111
stale_ghostty_commit=2222222222222222222222222222222222222222
mkdir -p "$fixture/linux/scripts" "$fixture/ghostty" "$fake_bin" "$git_state"
cp "$root_dir/linux/scripts/run-dev.sh" "$fixture/linux/scripts/run-dev.sh"
printf '.minimum_zig_version = "0.16.0",\n' > "$fixture/ghostty/build.zig.zon"
printf '%s\n' "$expected_ghostty_commit" > "$git_state/expected"

cat > "$fake_bin/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'git cwd=%s args=%s\n' "$PWD" "$*" >> "$CMUX_TEST_CAPTURE"

[[ "${1:-}" == -C ]]
git_dir=$2
shift 2
case "$git_dir:$*" in
    "$CMUX_TEST_REPO_ROOT:rev-parse HEAD:ghostty")
        cat "$CMUX_TEST_GIT_STATE/expected"
        ;;
    "$CMUX_TEST_GHOSTTY_DIR:rev-parse HEAD")
        cat "$CMUX_TEST_GIT_STATE/actual"
        ;;
    "$CMUX_TEST_GHOSTTY_DIR:status --porcelain --untracked-files=normal")
        [[ ! -f "$CMUX_TEST_GIT_STATE/dirty" ]] || cat "$CMUX_TEST_GIT_STATE/dirty"
        ;;
    "$CMUX_TEST_GHOSTTY_DIR:symbolic-ref --quiet --short HEAD")
        if [[ -s "$CMUX_TEST_GIT_STATE/branch" ]]; then
            cat "$CMUX_TEST_GIT_STATE/branch"
        else
            exit 1
        fi
        ;;
    "$CMUX_TEST_REPO_ROOT:submodule update --init --recursive ghostty")
        cp "$CMUX_TEST_GIT_STATE/expected" "$CMUX_TEST_GIT_STATE/actual"
        : > "$CMUX_TEST_GHOSTTY_DIR/.git"
        : > "$CMUX_TEST_GHOSTTY_DIR/build.zig"
        ;;
    *)
        printf 'unexpected fake git invocation: -C %s %s\n' "$git_dir" "$*" >&2
        exit 2
        ;;
esac
EOF

cat > "$fake_bin/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == version ]]; then
    printf '%s\n' "${CMUX_TEST_ZIG_VERSION:-0.16.0}"
    exit 0
fi
printf 'zig cwd=%s args=%s\n' "$PWD" "$*" >> "$CMUX_TEST_CAPTURE"
mkdir -p zig-out/lib
: > zig-out/lib/libghostty-internal.so
EOF

cat > "$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo cwd=%s args=%s\n' "$PWD" "$*" >> "$CMUX_TEST_CAPTURE"
target_dir=
while (($#)); do
    if [[ "$1" == "--target-dir" ]]; then
        target_dir=$2
        break
    fi
    shift
done
[[ -n "$target_dir" ]]
mkdir -p "$target_dir/debug"
cat > "$target_dir/debug/cmux" <<'INNER'
#!/usr/bin/env bash
set -euo pipefail
{
    printf 'cmux cwd=%s argc=%s\n' "$PWD" "$#"
    index=0
    for argument in "$@"; do
        printf 'cmux arg%s=%s\n' "$index" "$argument"
        index=$((index + 1))
    done
    printf 'CMUX_GHOSTTY_LIBRARY=%s\n' "$CMUX_GHOSTTY_LIBRARY"
    printf 'CMUX_GHOSTTY_ROOT=%s\n' "$CMUX_GHOSTTY_ROOT"
    printf 'CMUX_SOCKET_PATH=%s\n' "$CMUX_SOCKET_PATH"
    printf 'CMUX_SOCKET=%s\n' "${CMUX_SOCKET-unset}"
} >> "$CMUX_TEST_CAPTURE"
INNER
chmod +x "$target_dir/debug/cmux"
EOF

chmod +x "$fake_bin/git" "$fake_bin/zig" "$fake_bin/cargo"

caller_dir=$tmp_dir/caller
target_dir=$tmp_dir/target
state_dir=$tmp_dir/state
open_target="$tmp_dir/target with spaces"
error_log=$tmp_dir/error.log
mkdir -p "$caller_dir"
(
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CARGO_TARGET_DIR="$target_dir" \
    XDG_STATE_HOME="$state_dir" \
    CMUX_SOCKET=/ambient/cmux.sock \
        "$fixture/linux/scripts/run-dev.sh" "$open_target"
)

grep -F "git cwd=$caller_dir args=-C $fixture submodule update --init --recursive ghostty" "$capture"
grep -F "zig cwd=$fixture/ghostty args=build --cache-dir .zig-cache/cmux-0.16.0 -Dapp-runtime=none -Doptimize=ReleaseSafe" "$capture"
grep -F "cargo cwd=$caller_dir args=build --locked --manifest-path $fixture/linux/Cargo.toml --features gtk --target-dir $target_dir" "$capture"
grep -F "cmux cwd=$caller_dir argc=6" "$capture"
grep -F "cmux arg0=app" "$capture"
grep -F "cmux arg1=--renderer" "$capture"
grep -F "cmux arg2=ghostty" "$capture"
grep -F "cmux arg3=--socket" "$capture"
grep -F "cmux arg4=$state_dir/cmux/cmux.sock" "$capture"
grep -F "cmux arg5=$open_target" "$capture"
grep -F "CMUX_GHOSTTY_LIBRARY=$fixture/ghostty/zig-out/lib/libghostty-internal.so" "$capture"
grep -F "CMUX_GHOSTTY_ROOT=$fixture/ghostty" "$capture"
grep -F "CMUX_SOCKET_PATH=$state_dir/cmux/cmux.sock" "$capture"
grep -F "CMUX_SOCKET=unset" "$capture"

: > "$capture"
: > "$fixture/ghostty/build.zig"
(
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CARGO_TARGET_DIR="$target_dir" \
    XDG_STATE_HOME="$state_dir" \
        "$fixture/linux/scripts/run-dev.sh"
)
if grep -F 'submodule update' "$capture" >/dev/null; then
    echo "FAIL: current Ghostty submodule was updated during launch" >&2
    exit 1
fi

: > "$capture"
printf '%s\n' "$stale_ghostty_commit" > "$git_state/actual"
rm -f "$git_state/dirty" "$git_state/branch"
(
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CARGO_TARGET_DIR="$target_dir" \
    XDG_STATE_HOME="$state_dir" \
        "$fixture/linux/scripts/run-dev.sh"
)
grep -F "git cwd=$caller_dir args=-C $fixture submodule update --init --recursive ghostty" "$capture"
grep -Fx "$expected_ghostty_commit" "$git_state/actual"

: > "$capture"
printf '%s\n' "$stale_ghostty_commit" > "$git_state/actual"
printf ' M src/dirty.zig\n' > "$git_state/dirty"
if (
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CARGO_TARGET_DIR="$target_dir" \
    XDG_STATE_HOME="$state_dir" \
        "$fixture/linux/scripts/run-dev.sh"
) > /dev/null 2> "$error_log"; then
    echo "FAIL: launcher accepted a stale dirty Ghostty submodule" >&2
    exit 1
fi
grep -F "error: Ghostty submodule is at $stale_ghostty_commit but cmux pins $expected_ghostty_commit; refusing to update a checkout with local changes" "$error_log"
grep -Fx "$stale_ghostty_commit" "$git_state/actual"
if grep -F 'submodule update' "$capture" >/dev/null; then
    echo "FAIL: stale dirty Ghostty submodule was updated" >&2
    exit 1
fi

: > "$capture"
rm -f "$git_state/dirty"
printf 'main\n' > "$git_state/branch"
if (
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CARGO_TARGET_DIR="$target_dir" \
    XDG_STATE_HOME="$state_dir" \
        "$fixture/linux/scripts/run-dev.sh"
) > /dev/null 2> "$error_log"; then
    echo "FAIL: launcher accepted a stale attached Ghostty submodule" >&2
    exit 1
fi
grep -F "error: Ghostty submodule is at $stale_ghostty_commit on branch main but cmux pins $expected_ghostty_commit; refusing to move an attached branch" "$error_log"
grep -Fx "$stale_ghostty_commit" "$git_state/actual"
if grep -F 'submodule update' "$capture" >/dev/null; then
    echo "FAIL: stale attached Ghostty submodule was updated" >&2
    exit 1
fi
rm -f "$git_state/branch"

printf 'not a Ghostty manifest\n' > "$fixture/ghostty/build.zig.zon"
if (
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
        "$fixture/linux/scripts/run-dev.sh"
) > /dev/null 2> "$error_log"; then
    echo "FAIL: launcher accepted a Ghostty manifest without minimum_zig_version" >&2
    exit 1
fi
grep -F "error: could not read minimum_zig_version from $fixture/ghostty/build.zig.zon" "$error_log"

printf '.minimum_zig_version = "0.16.0",\n' > "$fixture/ghostty/build.zig.zon"
if (
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_ZIG_VERSION=0.15.2 \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CMUX_ZIG="$fake_bin/zig" \
        "$fixture/linux/scripts/run-dev.sh"
) > /dev/null 2> "$error_log"; then
    echo "FAIL: launcher accepted an incompatible explicit CMUX_ZIG" >&2
    exit 1
fi
grep -F "error: CMUX_ZIG must be zig 0.16.0: $fake_bin/zig" "$error_log"

non_executable_zig=$tmp_dir/non-executable-zig
: > "$non_executable_zig"
if (
    cd "$caller_dir"
    PATH="$fake_bin:/usr/bin:/bin" \
    CMUX_TEST_CAPTURE="$capture" \
    CMUX_TEST_GIT_STATE="$git_state" \
    CMUX_TEST_REPO_ROOT="$fixture" \
    CMUX_TEST_GHOSTTY_DIR="$fixture/ghostty" \
    CMUX_ZIG="$non_executable_zig" \
        "$fixture/linux/scripts/run-dev.sh"
) > /dev/null 2> "$error_log"; then
    echo "FAIL: launcher accepted a non-executable CMUX_ZIG" >&2
    exit 1
fi
grep -F "error: CMUX_ZIG is not executable: $non_executable_zig" "$error_log"

echo "PASS: Linux development launcher builds and runs the pinned Ghostty submodule"
