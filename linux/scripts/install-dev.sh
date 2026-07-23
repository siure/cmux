#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
linux_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
repo_root=$(CDPATH= cd -- "$linux_dir/.." && pwd)

prefix=${PREFIX:-"$HOME/.local"}
bin_dir=${CMUX_LINUX_BIN_DIR:-"$prefix/bin"}
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
config_dir=$data_home/cmux
applications_dir=$data_home/applications
icons_dir=$data_home/icons/hicolor/scalable/apps
app_id=${CMUX_LINUX_APP_ID:-ai.manaflow.cmux}
renderer=${CMUX_LINUX_RENDERER:-ghostty}
case "$renderer" in
    gtk4) renderer=gtk ;;
    vt|libghostty-vt) renderer=ghostty-vt ;;
esac
if [[ -v CMUX_LINUX_CARGO_FEATURES ]]; then
    cargo_features=$CMUX_LINUX_CARGO_FEATURES
else
    case "$renderer" in
        gtk|ghostty) cargo_features=gtk ;;
        core|ghostty-vt) cargo_features= ;;
        *) cargo_features=gtk ;;
    esac
fi
cargo_profile=${CMUX_LINUX_CARGO_PROFILE:-release}
build_ghostty=${CMUX_LINUX_BUILD_GHOSTTY:-1}
pkg_config=${PKG_CONFIG:-pkg-config}
ghostty_prefix=${CMUX_GHOSTTY_PREFIX:-"$data_home/cmux/ghostty"}
ghostty_library_path=${CMUX_GHOSTTY_LIBRARY:-}
ghostty_vt_library_path=${CMUX_GHOSTTY_VT_LIBRARY:-}
ghostty_root_path=${CMUX_GHOSTTY_ROOT:-}
launcher_socket_path=${CMUX_LINUX_SOCKET_PATH:-}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

has_feature() {
    local needle=$1
    local list=",$cargo_features,"
    [[ "$list" == *",$needle,"* ]]
}

validate_renderer() {
    case "$renderer" in
        core|gtk|ghostty|ghostty-vt) ;;
        *) fail "CMUX_LINUX_RENDERER must be one of: core, gtk, ghostty, ghostty-vt (got: $renderer)" ;;
    esac

    case "$renderer" in
        gtk|ghostty)
            has_feature gtk || fail "CMUX_LINUX_RENDERER=$renderer requires CMUX_LINUX_CARGO_FEATURES to include gtk"
            ;;
    esac
}

sed_escape() {
    printf '%s' "$1" | sed 's/[&|]/\\&/g'
}

shell_quote() {
    printf "'"
    printf '%s' "$1" | sed "s/'/'\\\\''/g"
    printf "'"
}

append_unique_path() {
    local path=$1
    [[ -n "$path" ]] || return 0
    local existing
    for existing in "${path_candidates[@]}"; do
        [[ "$existing" == "$path" ]] && return 0
    done
    path_candidates+=("$path")
}

append_colon_paths() {
    local paths=${1:-}
    local old_ifs=$IFS
    local path
    IFS=:
    for path in $paths; do
        append_unique_path "$path"
    done
    IFS=$old_ifs
}

append_pkg_config_library_paths() {
    local package=$1
    local token
    while IFS= read -r token; do
        case "$token" in
            -L?*) append_unique_path "${token#-L}" ;;
        esac
    done < <("$pkg_config" --libs-only-L "$package" 2>/dev/null | tr '[:space:]' '\n')
}

find_gtk4_link_library() {
    path_candidates=()
    append_pkg_config_library_paths gtk4
    append_colon_paths "${LIBRARY_PATH:-}"
    append_colon_paths "${LD_LIBRARY_PATH:-}"
    local candidate
    for candidate in \
        /usr/lib64 \
        /usr/lib \
        /usr/local/lib64 \
        /usr/local/lib \
        /lib64 \
        /lib \
        /usr/lib/x86_64-linux-gnu \
        /usr/lib/aarch64-linux-gnu
    do
        append_unique_path "$candidate"
    done

    local dir
    for dir in "${path_candidates[@]}"; do
        if [[ -f "$dir/libgtk-4.so" ]]; then
            printf '%s\n' "$dir/libgtk-4.so"
            return 0
        fi
    done
    return 1
}

find_ghostty_checkout() {
    if [[ -n "${CMUX_GHOSTTY_CHECKOUT:-}" ]]; then
        if [[ -f "$CMUX_GHOSTTY_CHECKOUT/include/ghostty.h" ]]; then
            printf '%s\n' "$CMUX_GHOSTTY_CHECKOUT"
            return 0
        fi
        return 1
    fi

    local candidate
    for candidate in "$repo_root/../ghostty" "$repo_root/ghostty"; do
        if [[ -f "$candidate/include/ghostty.h" ]]; then
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
        "$prefix/zig-out/lib/ghostty-internal.so" \
        "$prefix/zig-out/lib/libghostty.so"
    do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

find_ghostty_vt_library() {
    local prefix=$1
    local candidate
    for candidate in \
        "$prefix/lib/libghostty-vt.so" \
        "$prefix/lib/libghostty-vt.a" \
        "$prefix/zig-out/lib/libghostty-vt.so" \
        "$prefix/zig-out/lib/libghostty-vt.a"
    do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    local dir
    for dir in "$prefix/lib" "$prefix/zig-out/lib"; do
        if [[ -d "$dir" ]]; then
            candidate=$(find "$dir" -maxdepth 1 -type f -name 'libghostty-vt.so.*' | sort -V | tail -n 1)
            if [[ -n "$candidate" ]]; then
                printf '%s\n' "$candidate"
                return 0
            fi
        fi
    done

    return 1
}

find_ghostty_root_from_library() {
    local library=$1
    local root
    root=$(CDPATH= cd -- "$(dirname -- "$library")/.." 2>/dev/null && pwd) || return 1
    if ghostty_root_has_any_header "$root"; then
        printf '%s\n' "$root"
        return 0
    fi
    if [[ "$(basename -- "$root")" == "zig-out" ]]; then
        local checkout_root
        checkout_root=$(CDPATH= cd -- "$root/.." 2>/dev/null && pwd) || return 1
        if ghostty_root_has_any_header "$checkout_root"; then
            printf '%s\n' "$checkout_root"
            return 0
        fi
    fi
    return 1
}

ghostty_root_has_header() {
    local root=$1
    [[ -f "$root/include/ghostty.h" || -f "$root/zig-out/include/ghostty.h" ]]
}

ghostty_root_has_vt_header() {
    local root=$1
    [[ -f "$root/include/ghostty/vt.h" || -f "$root/zig-out/include/ghostty/vt.h" ]]
}

ghostty_root_has_any_header() {
    local root=$1
    ghostty_root_has_header "$root" || ghostty_root_has_vt_header "$root"
}

validate_ghostty_renderer() {
    local backend=$1
    local diagnostics_tmp
    diagnostics_tmp=$(mktemp)

    local -a env_args=()
    if [[ "$backend" == "ghostty" ]]; then
        env_args+=(CMUX_GHOSTTY_LIBRARY="$ghostty_library_path")
    else
        env_args+=(CMUX_GHOSTTY_VT_LIBRARY="$ghostty_vt_library_path")
    fi
    if [[ -n "$ghostty_root_path" ]]; then
        env_args+=(CMUX_GHOSTTY_ROOT="$ghostty_root_path")
    fi

    if ! env "${env_args[@]}" "$cmux_binary" app --renderer "$backend" --script "renderer diagnostics --backend $backend
quit" > "$diagnostics_tmp"; then
        printf 'Ghostty %s renderer diagnostics command failed:\n' "$backend" >&2
        cat "$diagnostics_tmp" >&2 || true
        rm -f "$diagnostics_tmp"
        fail "resolved Ghostty $backend artifact could not be validated"
    fi

    if [[ "$backend" == "ghostty" ]]; then
        if ! grep -Eq '"embedding_status"[[:space:]]*:[[:space:]]*"available"' "$diagnostics_tmp" ||
           ! grep -Eq '"linux_embedding_supported"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp" ||
           ! grep -Eq '"embedding_darwin_symbols_hidden"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp" ||
           ! grep -Eq '"embedding_internal_symbols_hidden"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp" ||
           ! grep -Eq '"embedding_unexpected_export_symbols_hidden"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp" ||
           ! grep -Eq '"embedding_unexpected_export_symbol_count"[[:space:]]*:[[:space:]]*0' "$diagnostics_tmp" ||
           ! grep -Eq '"runtime_resources_present"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp"; then
            printf 'Ghostty full renderer diagnostics did not report a usable Linux embedding:\n' >&2
            cat "$diagnostics_tmp" >&2
            rm -f "$diagnostics_tmp"
            fail "resolved Ghostty full renderer artifact failed diagnostics"
        fi
    else
        if ! grep -Eq '"vt_supported"[[:space:]]*:[[:space:]]*true' "$diagnostics_tmp"; then
            printf 'Ghostty VT diagnostics did not report a usable libghostty-vt:\n' >&2
            cat "$diagnostics_tmp" >&2
            rm -f "$diagnostics_tmp"
            fail "resolved Ghostty VT artifact failed diagnostics"
        fi
    fi

    rm -f "$diagnostics_tmp"
}

validate_renderer

command -v cargo >/dev/null 2>&1 || fail "cargo is required"

if has_feature gtk; then
    command -v "$pkg_config" >/dev/null 2>&1 || fail "pkg-config is required for the GTK launcher build (resolved command: $pkg_config)"
    if ! "$pkg_config" --exists gtk4; then
        fail "GTK4 development files were not found. Install gtk4-devel/libgtk-4-dev and retry."
    fi
    if ! "$pkg_config" --exists webkitgtk-web-process-extension-6.0; then
        fail "WebKitGTK 6 development files were not found. Install webkitgtk6.0-devel/libwebkitgtk-6.0-dev and retry."
    fi
    if ! find_gtk4_link_library >/dev/null; then
        fail "gtk4.pc was found, but libgtk-4.so was not found on pkg-config or common linker paths. Install gtk4-devel/libgtk-4-dev and retry."
    fi
fi

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    cargo_target_root=$CARGO_TARGET_DIR
    case "$cargo_target_root" in
        /*) ;;
        *) cargo_target_root=$PWD/$cargo_target_root ;;
    esac
else
    cargo_target_root=$linux_dir/target
fi

cargo_args=(
    build
    --locked
    --manifest-path "$linux_dir/Cargo.toml"
    --target-dir "$cargo_target_root"
)
if [[ -n "$cargo_features" ]]; then
    cargo_args+=(--features "$cargo_features")
fi
if [[ "$cargo_profile" == "release" ]]; then
    cargo_args+=(--release)
elif [[ "$cargo_profile" != "debug" ]]; then
    fail "CMUX_LINUX_CARGO_PROFILE must be 'release' or 'debug'"
fi

printf 'Building cmux Linux binary (%s, features: %s)\n' "$cargo_profile" "${cargo_features:-none}"
cargo "${cargo_args[@]}"

target_dir=$cargo_target_root/$cargo_profile
cmux_binary=$target_dir/cmux
remote_daemon_binary=$target_dir/cmuxd-remote
[[ -x "$cmux_binary" ]] || fail "built cmux binary not found at $cmux_binary"
[[ -x "$remote_daemon_binary" ]] || fail "built cmuxd-remote binary not found at $remote_daemon_binary"

if [[ "$renderer" == "ghostty" && "$build_ghostty" != "0" ]]; then
    command -v zig >/dev/null 2>&1 || fail "zig is required to build Ghostty for the ghostty renderer"
    ghostty_checkout=$(find_ghostty_checkout) || fail "Ghostty checkout not found; set CMUX_GHOSTTY_CHECKOUT or CMUX_LINUX_BUILD_GHOSTTY=0"
    printf 'Building Ghostty internal library into %s\n' "$ghostty_prefix"
    (cd "$ghostty_checkout" && zig build -Dapp-runtime=none -Doptimize=ReleaseSafe -p "$ghostty_prefix")
    ghostty_library_path=$(find_ghostty_library "$ghostty_prefix") || fail "Ghostty library was not installed at $ghostty_prefix/lib/libghostty-internal.so or $ghostty_prefix/lib/ghostty-internal.so"
    ghostty_root_path=$ghostty_prefix
elif [[ "$renderer" == "ghostty-vt" && "$build_ghostty" != "0" ]]; then
    command -v zig >/dev/null 2>&1 || fail "zig is required to build Ghostty VT for the ghostty-vt renderer"
    ghostty_checkout=$(find_ghostty_checkout) || fail "Ghostty checkout not found; set CMUX_GHOSTTY_CHECKOUT or CMUX_LINUX_BUILD_GHOSTTY=0"
    printf 'Building Ghostty VT library into %s\n' "$ghostty_prefix"
    (cd "$ghostty_checkout" && zig build -Demit-lib-vt=true -Doptimize=ReleaseSafe -p "$ghostty_prefix")
    ghostty_vt_library_path=$(find_ghostty_vt_library "$ghostty_prefix") || fail "Ghostty VT library was not installed at $ghostty_prefix/lib/libghostty-vt.so"
    ghostty_root_path=$ghostty_prefix
elif [[ "$renderer" == "ghostty" ]]; then
    if [[ -n "$ghostty_root_path" && -z "$ghostty_library_path" ]]; then
        ghostty_library_path=$(find_ghostty_library "$ghostty_root_path") || fail "Ghostty library was not found under CMUX_GHOSTTY_ROOT=$ghostty_root_path"
    elif [[ -z "$ghostty_library_path" ]]; then
        discovered_ghostty_library=$(find_ghostty_library "$ghostty_prefix") || fail "Ghostty library was not found. Set CMUX_GHOSTTY_LIBRARY, set CMUX_GHOSTTY_ROOT, or omit CMUX_LINUX_BUILD_GHOSTTY=0 so the installer can build Ghostty."
        ghostty_library_path=$discovered_ghostty_library
        if [[ -z "$ghostty_root_path" && -f "$ghostty_prefix/include/ghostty.h" ]]; then
            ghostty_root_path=$ghostty_prefix
        fi
    fi
    if [[ -z "$ghostty_root_path" && -n "$ghostty_library_path" ]]; then
        if discovered_ghostty_root=$(find_ghostty_root_from_library "$ghostty_library_path"); then
            ghostty_root_path=$discovered_ghostty_root
        fi
    fi
    [[ -f "$ghostty_library_path" ]] || fail "Ghostty library path does not exist: $ghostty_library_path"
    if [[ -n "$ghostty_root_path" ]]; then
        ghostty_root_has_header "$ghostty_root_path" || fail "Ghostty root does not contain include/ghostty.h or zig-out/include/ghostty.h: $ghostty_root_path"
    fi
elif [[ "$renderer" == "ghostty-vt" ]]; then
    if [[ -n "$ghostty_root_path" && -z "$ghostty_vt_library_path" ]]; then
        ghostty_vt_library_path=$(find_ghostty_vt_library "$ghostty_root_path") || fail "Ghostty VT library was not found under CMUX_GHOSTTY_ROOT=$ghostty_root_path"
    elif [[ -z "$ghostty_vt_library_path" ]]; then
        discovered_ghostty_vt_library=$(find_ghostty_vt_library "$ghostty_prefix") || fail "Ghostty VT library was not found. Set CMUX_GHOSTTY_VT_LIBRARY, set CMUX_GHOSTTY_ROOT, or omit CMUX_LINUX_BUILD_GHOSTTY=0 so the installer can build Ghostty VT."
        ghostty_vt_library_path=$discovered_ghostty_vt_library
        if [[ -z "$ghostty_root_path" && -f "$ghostty_prefix/include/ghostty/vt.h" ]]; then
            ghostty_root_path=$ghostty_prefix
        fi
    fi
    if [[ -z "$ghostty_root_path" && -n "$ghostty_vt_library_path" ]]; then
        if discovered_ghostty_root=$(find_ghostty_root_from_library "$ghostty_vt_library_path"); then
            ghostty_root_path=$discovered_ghostty_root
        fi
    fi
    [[ -f "$ghostty_vt_library_path" ]] || fail "Ghostty VT library path does not exist: $ghostty_vt_library_path"
    if [[ -n "$ghostty_root_path" ]]; then
        ghostty_root_has_vt_header "$ghostty_root_path" || fail "Ghostty root does not contain include/ghostty/vt.h or zig-out/include/ghostty/vt.h: $ghostty_root_path"
    fi
fi

case "$renderer" in
    ghostty)
        printf 'Validating Ghostty full renderer diagnostics\n'
        validate_ghostty_renderer ghostty
        ;;
    ghostty-vt)
        printf 'Validating Ghostty VT renderer diagnostics\n'
        validate_ghostty_renderer ghostty-vt
        ;;
esac

install -d "$bin_dir" "$config_dir" "$applications_dir" "$icons_dir"
install -m 0755 "$cmux_binary" "$bin_dir/cmux"
install -m 0755 "$remote_daemon_binary" "$bin_dir/cmuxd-remote"
install -m 0755 "$linux_dir/dist/cmux-linux-app" "$bin_dir/cmux-linux-app"

desktop_tmp=
config_tmp=$(mktemp)
trap 'rm -f "${desktop_tmp:-}" "${config_tmp:-}"' EXIT
{
    printf '# Generated by linux/scripts/install-dev.sh\n'
    printf 'CMUX_LINUX_RENDERER=${CMUX_LINUX_RENDERER:-%s}\n' "$(shell_quote "$renderer")"
    printf 'export CMUX_LINUX_RENDERER\n'
    if [[ "$renderer" == "ghostty" ]]; then
        printf 'CMUX_GHOSTTY_LIBRARY=${CMUX_GHOSTTY_LIBRARY:-%s}\n' "$(shell_quote "$ghostty_library_path")"
        printf 'export CMUX_GHOSTTY_LIBRARY\n'
        if [[ -n "$ghostty_root_path" ]]; then
            printf 'CMUX_GHOSTTY_ROOT=${CMUX_GHOSTTY_ROOT:-%s}\n' "$(shell_quote "$ghostty_root_path")"
            printf 'export CMUX_GHOSTTY_ROOT\n'
        fi
    elif [[ "$renderer" == "ghostty-vt" ]]; then
        printf 'CMUX_GHOSTTY_VT_LIBRARY=${CMUX_GHOSTTY_VT_LIBRARY:-%s}\n' "$(shell_quote "$ghostty_vt_library_path")"
        printf 'export CMUX_GHOSTTY_VT_LIBRARY\n'
        if [[ -n "$ghostty_root_path" ]]; then
            printf 'CMUX_GHOSTTY_ROOT=${CMUX_GHOSTTY_ROOT:-%s}\n' "$(shell_quote "$ghostty_root_path")"
            printf 'export CMUX_GHOSTTY_ROOT\n'
        fi
    fi
    if [[ -n "$launcher_socket_path" ]]; then
        printf 'CMUX_SOCKET_PATH=${CMUX_SOCKET_PATH:-%s}\n' "$(shell_quote "$launcher_socket_path")"
        printf 'export CMUX_SOCKET_PATH\n'
    fi
} > "$config_tmp"
install -m 0644 "$config_tmp" "$config_dir/linux-app.env"
development_version=$(git -C "$repo_root" describe --tags --always --dirty 2>/dev/null || printf 'development')
printf '%s\n' "$development_version" > "$config_dir/bundle-version"

icon_source=$repo_root/web/public/cmux-icon.svg
[[ -f "$icon_source" ]] || fail "cmux icon not found at $icon_source"
install -m 0644 "$icon_source" "$icons_dir/$app_id.svg"

desktop_tmp=$(mktemp)
sed \
    -e "s|@CMUX_APP_WRAPPER@|$(sed_escape "$bin_dir/cmux-linux-app")|g" \
    -e "s|@APP_ID@|$(sed_escape "$app_id")|g" \
    "$linux_dir/dist/ai.manaflow.cmux.desktop.in" > "$desktop_tmp"
install -m 0644 "$desktop_tmp" "$applications_dir/$app_id.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications_dir" >/dev/null 2>&1 || true
fi

printf '\nInstalled cmux Linux development app:\n'
printf '  binary:   %s/cmux\n' "$bin_dir"
printf '  remote:   %s/cmuxd-remote\n' "$bin_dir"
printf '  launcher: %s/cmux-linux-app\n' "$bin_dir"
printf '  config:   %s/linux-app.env\n' "$config_dir"
printf '  desktop:  %s/%s.desktop\n' "$applications_dir" "$app_id"
printf '  icon:     %s/%s.svg\n' "$icons_dir" "$app_id"
if [[ -n "$launcher_socket_path" ]]; then
    printf '  socket:   %s\n' "$launcher_socket_path"
else
    printf '  socket:   default control socket (override at runtime with CMUX_SOCKET_PATH)\n'
fi
if [[ "$renderer" == "ghostty" ]]; then
    printf '  ghostty:  %s\n' "$ghostty_library_path"
    if [[ -n "$ghostty_root_path" ]]; then
        printf '  root:     %s\n' "$ghostty_root_path"
    fi
elif [[ "$renderer" == "ghostty-vt" ]]; then
    printf '  ghostty-vt: %s\n' "$ghostty_vt_library_path"
    if [[ -n "$ghostty_root_path" ]]; then
        printf '  root:       %s\n' "$ghostty_root_path"
    fi
fi
printf '\nRun with: %s/cmux-linux-app\n' "$bin_dir"
printf 'Or from a desktop shell: gtk-launch %s\n' "$app_id"
