#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
linux_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
repo_root=$(CDPATH= cd -- "$linux_dir/.." && pwd)
ghostty_dir=$repo_root/ghostty

if [[ ! -f "$ghostty_dir/build.zig" ]]; then
    git -C "$repo_root" submodule update --init --recursive ghostty
fi

required_zig=$(sed -nE \
    's/^[[:space:]]*\.?minimum_zig_version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' \
    "$ghostty_dir/build.zig.zon" | head -n 1)
if [[ -z "$required_zig" ]]; then
    echo "error: could not read minimum_zig_version from $ghostty_dir/build.zig.zon" >&2
    exit 1
fi

zig_command=${CMUX_ZIG:-}
if [[ -n "$zig_command" ]]; then
    if [[ "$zig_command" != */* ]]; then
        zig_command=$(command -v -- "$zig_command" 2>/dev/null || true)
    fi
    if [[ -z "$zig_command" || ! -x "$zig_command" ]]; then
        echo "error: CMUX_ZIG is not executable: ${CMUX_ZIG}" >&2
        exit 1
    fi
    if [[ "$("$zig_command" version 2>/dev/null || true)" != "$required_zig" ]]; then
        echo "error: CMUX_ZIG must be zig $required_zig: ${CMUX_ZIG}" >&2
        exit 1
    fi
else
    path_zig=$(command -v zig 2>/dev/null || true)
    if [[ -n "$path_zig" && "$($path_zig version 2>/dev/null || true)" == "$required_zig" ]]; then
        zig_command=$path_zig
    else
        zig_arch=$(uname -m)
        [[ "$zig_arch" == arm64 ]] && zig_arch=aarch64
        zig_cache_root=${XDG_CACHE_HOME:-"$HOME/.cache"}/cmux/zig
        zig_install_name=zig-$zig_arch-linux-$required_zig
        zig_command=$zig_cache_root/$zig_install_name/zig
        if [[ ! -x "$zig_command" || "$($zig_command version 2>/dev/null || true)" != "$required_zig" ]]; then
            zig_download_url=https://ziglang.org/download/$required_zig/$zig_install_name.tar.xz
            ZIG_REQUIRED="$required_zig" \
            ZIG_FORCE_LOCAL_INSTALL=1 \
            ZIG_INSTALL_ROOT="$zig_cache_root" \
            RUNNER_TEMP="$zig_cache_root/tmp" \
            ZIG_MIRROR_URL="${ZIG_MIRROR_URL:-$zig_download_url}" \
            ZIG_SECONDARY_MIRROR_URL="${ZIG_SECONDARY_MIRROR_URL:-$zig_download_url}" \
                "$repo_root/scripts/install-zig-ci.sh"
        fi
    fi
fi
if [[ ! -x "$zig_command" || "$("$zig_command" version 2>/dev/null || true)" != "$required_zig" ]]; then
    echo "error: failed to provision zig $required_zig at $zig_command" >&2
    exit 1
fi
zig_command="$(CDPATH= cd -- "$(dirname -- "$zig_command")" && pwd)/$(basename -- "$zig_command")"
export PATH="$(dirname -- "$zig_command"):$PATH"

(
    cd "$ghostty_dir"
    "$zig_command" build \
        --cache-dir ".zig-cache/cmux-$required_zig" \
        -Dapp-runtime=none \
        -Doptimize=ReleaseSafe
)

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    case "$CARGO_TARGET_DIR" in
        /*) cargo_target_dir=$CARGO_TARGET_DIR ;;
        *) cargo_target_dir=$PWD/$CARGO_TARGET_DIR ;;
    esac
else
    cargo_target_dir=$linux_dir/target
fi

cargo build \
    --locked \
    --manifest-path "$linux_dir/Cargo.toml" \
    --features gtk \
    --target-dir "$cargo_target_dir"

export CMUX_GHOSTTY_LIBRARY="$ghostty_dir/zig-out/lib/libghostty-internal.so"
export CMUX_GHOSTTY_ROOT="$ghostty_dir"

state_home=${XDG_STATE_HOME:-"$HOME/.local/state"}
socket_path=${CMUX_LINUX_SOCKET_PATH:-"$state_home/cmux/cmux.sock"}
mkdir -p "$(dirname -- "$socket_path")"
export CMUX_SOCKET_PATH=$socket_path
unset CMUX_SOCKET || true

exec "$cargo_target_dir/debug/cmux" app --renderer ghostty --socket "$socket_path" "$@"
