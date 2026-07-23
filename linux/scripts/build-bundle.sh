#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
linux_dir=$(CDPATH='' cd -- "$script_dir/.." && pwd)
repo_root=$(CDPATH='' cd -- "$linux_dir/.." && pwd)

output_dir=${1:-"$repo_root/dist"}
build_artifacts=${CMUX_LINUX_BUNDLE_BUILD:-1}
validate_bundle=${CMUX_LINUX_BUNDLE_VALIDATE:-1}
strip_bundle=${CMUX_LINUX_BUNDLE_STRIP:-1}
cargo_profile=${CMUX_LINUX_CARGO_PROFILE:-release}
version=${CMUX_LINUX_BUNDLE_VERSION:-}
ghostty_checkout=

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

validate_metadata_value() {
    local name=$1
    local value=$2
    case "$value" in
        *$'\n'*|*$'\r'*) fail "$name must not contain newlines" ;;
    esac
}

validate_source_state() {
    local name=$1
    local value=$2
    case "$value" in
        true|false|unknown) ;;
        *) fail "$name must be true, false, or unknown" ;;
    esac
}

git_dirty_state() {
    local repository=$1
    local exclude_path=${2:-}
    git -C "$repository" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
        printf 'unknown\n'
        return
    }
    local status
    if [[ -n "$exclude_path" ]]; then
        status=$(git -C "$repository" status --porcelain --untracked-files=normal -- \
            . ":(exclude)$exclude_path" 2>/dev/null) || {
            printf 'unknown\n'
            return
        }
    else
        status=$(git -C "$repository" status --porcelain --untracked-files=normal 2>/dev/null) || {
            printf 'unknown\n'
            return
        }
    fi
    if [[ -n "$status" ]]; then
        printf 'true\n'
    else
        printf 'false\n'
    fi
}

find_ghostty_checkout() {
    local candidate
    if [[ -n "${CMUX_GHOSTTY_CHECKOUT:-}" ]]; then
        candidate=$CMUX_GHOSTTY_CHECKOUT
        [[ -f "$candidate/build.zig" ]] || return 1
        printf '%s\n' "$candidate"
        return 0
    fi

    for candidate in "$repo_root/../ghostty" "$repo_root/ghostty"; do
        if [[ -f "$candidate/build.zig" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

find_ghostty_library() {
    local prefix=$1
    local candidate
    for candidate in \
        "$prefix/lib/libghostty-internal.so" \
        "$prefix/lib/ghostty-internal.so" \
        "$prefix/zig-out/lib/libghostty-internal.so" \
        "$prefix/zig-out/lib/ghostty-internal.so"
    do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

find_ghostty_root() {
    local prefix=$1
    if [[ -f "$prefix/include/ghostty.h" && -d "$prefix/share/ghostty" ]]; then
        printf '%s\n' "$prefix"
        return 0
    fi
    if [[ -f "$prefix/zig-out/include/ghostty.h" && -d "$prefix/zig-out/share/ghostty" ]]; then
        printf '%s\n' "$prefix/zig-out"
        return 0
    fi
    return 1
}

case "$cargo_profile" in
    release|debug) ;;
    *) fail "CMUX_LINUX_CARGO_PROFILE must be 'release' or 'debug'" ;;
esac

case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) fail "unsupported Linux bundle architecture: $(uname -m)" ;;
esac

if [[ -z "$version" ]]; then
    version=$(git -C "$repo_root" describe --tags --always --dirty 2>/dev/null || printf 'development')
fi
cmux_commit=${CMUX_LINUX_BUNDLE_CMUX_COMMIT:-$(git -C "$repo_root" rev-parse HEAD 2>/dev/null || printf 'unknown')}
ghostty_repository=${CMUX_LINUX_BUNDLE_GHOSTTY_REPOSITORY:-local-checkout}

command -v tar >/dev/null 2>&1 || fail "tar is required"
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required"
mkdir -p "$output_dir"
output_dir=$(CDPATH='' cd -- "$output_dir" && pwd)

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/cmux-linux-bundle.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT INT TERM
bundle_name="cmux-linux-$arch"
bundle_root="$work_dir/$bundle_name"
ghostty_build_prefix="$work_dir/ghostty-prefix"

if [[ "$build_artifacts" != "0" ]]; then
    command -v cargo >/dev/null 2>&1 || fail "cargo is required to build cmux"
    command -v zig >/dev/null 2>&1 || fail "zig is required to build Ghostty"
    ghostty_checkout=$(find_ghostty_checkout) || \
        fail "Ghostty checkout not found; set CMUX_GHOSTTY_CHECKOUT"

    cargo_args=(build --locked --manifest-path "$linux_dir/Cargo.toml" --features gtk)
    if [[ "$cargo_profile" == "release" ]]; then
        cargo_args+=(--release)
    fi
    printf 'Building cmux Linux bundle binaries (%s)\n' "$cargo_profile"
    cargo "${cargo_args[@]}"

    printf 'Building Linux libghostty embedding\n'
    (
        cd "$ghostty_checkout"
        zig build -Dapp-runtime=none -Doptimize=ReleaseSafe -p "$ghostty_build_prefix"
    )
    ghostty_prefix=$ghostty_build_prefix
else
    ghostty_prefix=${CMUX_GHOSTTY_PREFIX:-}
    if [[ -z "$ghostty_prefix" ]]; then
        ghostty_checkout=$(find_ghostty_checkout) || \
            fail "Ghostty checkout not found; set CMUX_GHOSTTY_PREFIX"
        ghostty_prefix=$ghostty_checkout
    fi
fi

if [[ -n "${CMUX_LINUX_BUNDLE_GHOSTTY_COMMIT:-}" ]]; then
    ghostty_commit=$CMUX_LINUX_BUNDLE_GHOSTTY_COMMIT
elif [[ -n "$ghostty_checkout" ]]; then
    ghostty_commit=$(git -C "$ghostty_checkout" rev-parse HEAD 2>/dev/null || printf 'unknown')
else
    ghostty_commit=unknown
fi
cmux_dirty=${CMUX_LINUX_BUNDLE_CMUX_DIRTY:-$(git_dirty_state "$repo_root" dist)}
ghostty_dirty_checkout=$ghostty_checkout
if [[ -z "$ghostty_dirty_checkout" ]]; then
    ghostty_dirty_checkout=$(find_ghostty_checkout || true)
fi
if [[ -n "${CMUX_LINUX_BUNDLE_GHOSTTY_DIRTY:-}" ]]; then
    ghostty_dirty=$CMUX_LINUX_BUNDLE_GHOSTTY_DIRTY
elif [[ -n "$ghostty_dirty_checkout" ]]; then
    ghostty_dirty=$(git_dirty_state "$ghostty_dirty_checkout")
else
    ghostty_dirty=unknown
fi
validate_metadata_value "bundle version" "$version"
validate_metadata_value "cmux commit" "$cmux_commit"
validate_metadata_value "Ghostty repository" "$ghostty_repository"
validate_metadata_value "Ghostty commit" "$ghostty_commit"
validate_source_state "cmux dirty state" "$cmux_dirty"
validate_source_state "Ghostty dirty state" "$ghostty_dirty"

target_dir="$linux_dir/target/$cargo_profile"
cmux_binary=${CMUX_LINUX_BUNDLE_CMUX_BINARY:-"$target_dir/cmux"}
remote_binary=${CMUX_LINUX_BUNDLE_REMOTE_BINARY:-"$target_dir/cmuxd-remote"}
ghostty_root=$(find_ghostty_root "$ghostty_prefix") || \
    fail "Ghostty prefix does not contain installed headers and runtime resources: $ghostty_prefix"
ghostty_library=$(find_ghostty_library "$ghostty_prefix") || \
    fail "Ghostty prefix does not contain libghostty-internal.so: $ghostty_prefix"

[[ -x "$cmux_binary" ]] || fail "cmux binary is not executable: $cmux_binary"
[[ -x "$remote_binary" ]] || fail "cmuxd-remote binary is not executable: $remote_binary"

if [[ "$validate_bundle" != "0" ]]; then
    diagnostics="$work_dir/source-diagnostics.json"
    if ! env \
        CMUX_GHOSTTY_LIBRARY="$ghostty_library" \
        CMUX_GHOSTTY_ROOT="$ghostty_root" \
        "$cmux_binary" app --renderer ghostty --script $'renderer diagnostics --backend ghostty\nquit' \
        > "$diagnostics"; then
        cat "$diagnostics" >&2 || true
        fail "cmux rejected the source Ghostty embedding"
    fi
    grep -Eq '"embedding_status"[[:space:]]*:[[:space:]]*"available"' "$diagnostics" || \
        fail "Ghostty diagnostics did not report embedding_status=available"
    grep -Eq '"linux_embedding_supported"[[:space:]]*:[[:space:]]*true' "$diagnostics" || \
        fail "Ghostty diagnostics did not report Linux embedding support"
    grep -Eq '"runtime_resources_present"[[:space:]]*:[[:space:]]*true' "$diagnostics" || \
        fail "Ghostty diagnostics did not report complete runtime resources"
    grep -Eq '"embedding_unexpected_export_symbol_count"[[:space:]]*:[[:space:]]*0' "$diagnostics" || \
        fail "Ghostty diagnostics reported unexpected exported symbols"
fi

install -d \
    "$bundle_root/bin" \
    "$bundle_root/share/cmux/ghostty/lib" \
    "$bundle_root/share/cmux/ghostty/include" \
    "$bundle_root/share/cmux/ghostty/share" \
    "$bundle_root/share/applications" \
    "$bundle_root/share/icons/hicolor/scalable/apps"
install -m 0755 "$cmux_binary" "$bundle_root/bin/cmux"
install -m 0755 "$remote_binary" "$bundle_root/bin/cmuxd-remote"
install -m 0755 "$linux_dir/dist/cmux-linux-app" "$bundle_root/bin/cmux-linux-app"
install -m 0755 "$linux_dir/dist/install-bundle.sh" "$bundle_root/install.sh"
install -m 0755 "$ghostty_library" \
    "$bundle_root/share/cmux/ghostty/lib/libghostty-internal.so"
cp -a "$ghostty_root/include/." "$bundle_root/share/cmux/ghostty/include/"
cp -a "$ghostty_root/share/ghostty" "$bundle_root/share/cmux/ghostty/share/"

shopt -s nullglob
ghostty_vt_libraries=("$ghostty_root"/lib/libghostty-vt.so*)
if ((${#ghostty_vt_libraries[@]} > 0)); then
    cp -a "${ghostty_vt_libraries[@]}" "$bundle_root/share/cmux/ghostty/lib/"
fi
shopt -u nullglob

install -m 0644 "$linux_dir/dist/ai.manaflow.cmux.desktop.in" \
    "$bundle_root/share/cmux/ai.manaflow.cmux.desktop.in"
sed \
    -e 's|@CMUX_APP_WRAPPER@|cmux-linux-app|g' \
    -e 's|@APP_ID@|ai.manaflow.cmux|g' \
    "$linux_dir/dist/ai.manaflow.cmux.desktop.in" \
    > "$bundle_root/share/applications/ai.manaflow.cmux.desktop"
install -m 0644 "$repo_root/web/public/cmux-icon.svg" \
    "$bundle_root/share/icons/hicolor/scalable/apps/ai.manaflow.cmux.svg"
install -m 0644 "$repo_root/LICENSE" "$bundle_root/LICENSE"
printf '%s\n' "$version" > "$bundle_root/share/cmux/bundle-version"
cat > "$bundle_root/share/cmux/build-provenance.txt" <<EOF
schema=cmux.linux-bundle.provenance.v1
version=$version
architecture=$arch
cmux_commit=$cmux_commit
cmux_dirty=$cmux_dirty
ghostty_repository=$ghostty_repository
ghostty_commit=$ghostty_commit
ghostty_dirty=$ghostty_dirty
EOF
cat > "$bundle_root/share/cmux/linux-app.env" <<'EOF'
CMUX_LINUX_RENDERER=${CMUX_LINUX_RENDERER:-ghostty}
export CMUX_LINUX_RENDERER
EOF
cat > "$bundle_root/README.txt" <<'EOF'
cmux for Linux

Run bin/cmux-linux-app directly from the extracted directory, or run
./install.sh to install cmux under PREFIX (default: ~/.local) and register its
desktop entry. GTK 4 is required. WebKitGTK 6 enables native browser surfaces.
EOF

if [[ "$strip_bundle" != "0" ]]; then
    strip_tool=${STRIP:-strip}
    command -v "$strip_tool" >/dev/null 2>&1 || fail "strip tool not found: $strip_tool"
    "$strip_tool" --strip-unneeded \
        "$bundle_root/bin/cmux" \
        "$bundle_root/bin/cmuxd-remote" \
        "$bundle_root/share/cmux/ghostty/lib/libghostty-internal.so"
    for library in "$bundle_root"/share/cmux/ghostty/lib/libghostty-vt.so.*.*; do
        [[ -f "$library" ]] || continue
        "$strip_tool" --strip-unneeded "$library"
    done
fi

if [[ "$validate_bundle" != "0" ]]; then
    bundled_diagnostics="$work_dir/bundle-diagnostics.json"
    mkdir -p "$work_dir/home" "$work_dir/data" "$work_dir/state"
    HOME="$work_dir/home" \
    XDG_DATA_HOME="$work_dir/data" \
    XDG_STATE_HOME="$work_dir/state" \
    "$bundle_root/bin/cmux-linux-app" \
        --script $'renderer diagnostics --backend ghostty\nquit' \
        > "$bundled_diagnostics" \
        2>/dev/null || {
            cat "$bundled_diagnostics" >&2 || true
            fail "relocated bundle diagnostics failed"
        }
    grep -Eq '"embedding_status"[[:space:]]*:[[:space:]]*"available"' \
        "$bundled_diagnostics" || fail "relocated bundle could not discover libghostty"
fi

archive_tmp="$work_dir/$bundle_name.tar.gz"
epoch=${SOURCE_DATE_EPOCH:-$(git -C "$repo_root" log -1 --format=%ct 2>/dev/null || printf '0')}
if tar --help 2>/dev/null | grep -q -- '--sort'; then
    tar \
        --sort=name \
        --mtime="@$epoch" \
        --owner=0 \
        --group=0 \
        --numeric-owner \
        -C "$work_dir" \
        -czf "$archive_tmp" \
        "$bundle_name"
else
    tar -C "$work_dir" -czf "$archive_tmp" "$bundle_name"
fi

archive="$output_dir/$bundle_name.tar.gz"
checksum="$archive.sha256"
install -m 0644 "$archive_tmp" "$archive"
(
    cd "$output_dir"
    sha256sum "$(basename "$archive")" > "$(basename "$checksum")"
)

printf 'Built cmux Linux bundle:\n'
printf '  archive:  %s\n' "$archive"
printf '  checksum: %s\n' "$checksum"
