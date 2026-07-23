#!/usr/bin/env bash
set -euo pipefail

root_dir=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/cmux-linux-bundle-test.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

fixture_dir="$tmp_dir/fixture"
ghostty_prefix="$fixture_dir/ghostty"
output_dir="$tmp_dir/output"
second_output_dir="$tmp_dir/output-second"
mkdir -p \
    "$fixture_dir/bin" \
    "$ghostty_prefix/include/ghostty" \
    "$ghostty_prefix/lib" \
    "$ghostty_prefix/share/ghostty/shell-integration/bash" \
    "$ghostty_prefix/share/ghostty/themes" \
    "$output_dir" \
    "$second_output_dir"

cat > "$fixture_dir/bin/cmux" <<'EOF'
#!/usr/bin/env sh
set -eu
case " $* " in
    *" ping "*) exit 1 ;;
esac
if [ -n "${CMUX_TEST_CAPTURE:-}" ]; then
    {
        printf 'library=%s\n' "${CMUX_GHOSTTY_LIBRARY:-}"
        printf 'vt_library=%s\n' "${CMUX_GHOSTTY_VT_LIBRARY:-}"
        printf 'root=%s\n' "${CMUX_GHOSTTY_ROOT:-}"
        printf 'bundle_root=%s\n' "${CMUX_LINUX_BUNDLE_ROOT:-}"
        printf 'bundle_version=%s\n' "${CMUX_LINUX_BUNDLE_VERSION:-}"
        printf 'args=%s\n' "$*"
    } > "$CMUX_TEST_CAPTURE"
fi
EOF
cat > "$fixture_dir/bin/cmuxd-remote" <<'EOF'
#!/usr/bin/env sh
exit 0
EOF
chmod +x "$fixture_dir/bin/cmux" "$fixture_dir/bin/cmuxd-remote"

printf 'fixture ghostty header\n' > "$ghostty_prefix/include/ghostty.h"
printf 'fixture ghostty vt header\n' > "$ghostty_prefix/include/ghostty/vt.h"
printf 'fixture shared object\n' > "$ghostty_prefix/lib/libghostty-internal.so"
printf 'fixture vt shared object\n' > "$ghostty_prefix/lib/libghostty-vt.so.0.1.0"
ln -s libghostty-vt.so.0.1.0 "$ghostty_prefix/lib/libghostty-vt.so.0"
ln -s libghostty-vt.so.0 "$ghostty_prefix/lib/libghostty-vt.so"
printf 'fixture shell integration\n' \
    > "$ghostty_prefix/share/ghostty/shell-integration/bash/ghostty.bash"
printf 'palette = 0=#000000\n' > "$ghostty_prefix/share/ghostty/themes/Fixture"

CMUX_LINUX_BUNDLE_BUILD=0 \
CMUX_LINUX_BUNDLE_VALIDATE=0 \
CMUX_LINUX_BUNDLE_STRIP=0 \
CMUX_LINUX_BUNDLE_VERSION=test-version \
CMUX_LINUX_BUNDLE_CMUX_COMMIT=cmux-test-commit \
CMUX_LINUX_BUNDLE_CMUX_DIRTY=false \
CMUX_LINUX_BUNDLE_GHOSTTY_REPOSITORY=manaflow-ai/ghostty \
CMUX_LINUX_BUNDLE_GHOSTTY_COMMIT=ghostty-test-commit \
CMUX_LINUX_BUNDLE_GHOSTTY_DIRTY=false \
CMUX_LINUX_BUNDLE_CMUX_BINARY="$fixture_dir/bin/cmux" \
CMUX_LINUX_BUNDLE_REMOTE_BINARY="$fixture_dir/bin/cmuxd-remote" \
CMUX_GHOSTTY_PREFIX="$ghostty_prefix" \
SOURCE_DATE_EPOCH=1700000000 \
    "$root_dir/linux/scripts/build-bundle.sh" "$output_dir" >/dev/null

CMUX_LINUX_BUNDLE_BUILD=0 \
CMUX_LINUX_BUNDLE_VALIDATE=0 \
CMUX_LINUX_BUNDLE_STRIP=0 \
CMUX_LINUX_BUNDLE_VERSION=test-version \
CMUX_LINUX_BUNDLE_CMUX_COMMIT=cmux-test-commit \
CMUX_LINUX_BUNDLE_CMUX_DIRTY=false \
CMUX_LINUX_BUNDLE_GHOSTTY_REPOSITORY=manaflow-ai/ghostty \
CMUX_LINUX_BUNDLE_GHOSTTY_COMMIT=ghostty-test-commit \
CMUX_LINUX_BUNDLE_GHOSTTY_DIRTY=false \
CMUX_LINUX_BUNDLE_CMUX_BINARY="$fixture_dir/bin/cmux" \
CMUX_LINUX_BUNDLE_REMOTE_BINARY="$fixture_dir/bin/cmuxd-remote" \
CMUX_GHOSTTY_PREFIX="$ghostty_prefix" \
SOURCE_DATE_EPOCH=1700000000 \
    "$root_dir/linux/scripts/build-bundle.sh" "$second_output_dir" >/dev/null

archive="$output_dir/cmux-linux-x86_64.tar.gz"
checksum="$archive.sha256"
[[ -f "$archive" ]] || { echo "FAIL: bundle archive was not created" >&2; exit 1; }
[[ -f "$checksum" ]] || { echo "FAIL: bundle checksum was not created" >&2; exit 1; }
(cd "$output_dir" && sha256sum -c "$(basename "$checksum")" >/dev/null)
cmp "$archive" "$second_output_dir/$(basename "$archive")"

archive_listing=$(tar -tzf "$archive")
for required in \
    cmux-linux-x86_64/bin/cmux \
    cmux-linux-x86_64/bin/cmux-linux-app \
    cmux-linux-x86_64/bin/cmuxd-remote \
    cmux-linux-x86_64/install.sh \
    cmux-linux-x86_64/share/cmux/ghostty/lib/libghostty-internal.so \
    cmux-linux-x86_64/share/cmux/ghostty/lib/libghostty-vt.so \
    cmux-linux-x86_64/share/cmux/ghostty/include/ghostty.h \
    cmux-linux-x86_64/share/cmux/ghostty/share/ghostty/themes/Fixture \
    cmux-linux-x86_64/share/cmux/bundle-version \
    cmux-linux-x86_64/share/cmux/build-provenance.txt \
    cmux-linux-x86_64/share/applications/ai.manaflow.cmux.desktop
do
    if [[ "$archive_listing" != *"$required"* ]]; then
        echo "FAIL: bundle archive is missing $required" >&2
        exit 1
    fi
done

relocated_parent="$tmp_dir/relocated bundle"
mkdir -p "$relocated_parent"
tar -xzf "$archive" -C "$relocated_parent"
relocated="$relocated_parent/cmux-linux-x86_64"
provenance="$relocated/share/cmux/build-provenance.txt"
grep -Fx "schema=cmux.linux-bundle.provenance.v1" "$provenance"
grep -Fx "version=test-version" "$provenance"
grep -Fx "architecture=x86_64" "$provenance"
grep -Fx "cmux_commit=cmux-test-commit" "$provenance"
grep -Fx "cmux_dirty=false" "$provenance"
grep -Fx "ghostty_repository=manaflow-ai/ghostty" "$provenance"
grep -Fx "ghostty_commit=ghostty-test-commit" "$provenance"
grep -Fx "ghostty_dirty=false" "$provenance"
capture="$tmp_dir/relocated.capture"
mkdir -p "$tmp_dir/home" "$tmp_dir/data" "$tmp_dir/state"
mkdir -p "$tmp_dir/data/cmux/ghostty/lib"
printf 'stale user library\n' \
    > "$tmp_dir/data/cmux/ghostty/lib/libghostty-internal.so"
HOME="$tmp_dir/home" \
XDG_DATA_HOME="$tmp_dir/data" \
XDG_STATE_HOME="$tmp_dir/state" \
CMUX_TEST_CAPTURE="$capture" \
    "$relocated/bin/cmux-linux-app" --script quit

grep -Fx "library=$relocated/share/cmux/ghostty/lib/libghostty-internal.so" "$capture"
grep -Fx "vt_library=$relocated/share/cmux/ghostty/lib/libghostty-vt.so" "$capture"
grep -Fx "root=$relocated/share/cmux/ghostty" "$capture"
grep -Fx "bundle_root=$relocated" "$capture"
grep -Fx "bundle_version=test-version" "$capture"
grep -F 'args=app --renderer ghostty --socket ' "$capture" >/dev/null

install_root="$tmp_dir/installed prefix"
HOME="$tmp_dir/install-home" PREFIX="$install_root" \
    "$relocated/install.sh" >/dev/null
[[ -x "$install_root/bin/cmux-linux-app" ]]
[[ -f "$install_root/share/cmux/ghostty/lib/libghostty-internal.so" ]]
[[ -f "$install_root/share/cmux/bundle-version" ]]
[[ -f "$install_root/share/cmux/build-provenance.txt" ]]
[[ -f "$install_root/share/applications/ai.manaflow.cmux.desktop" ]]
grep -F "Exec=$install_root/bin/cmux-linux-app %U" \
    "$install_root/share/applications/ai.manaflow.cmux.desktop" >/dev/null
sh -n "$install_root/share/cmux/linux-app.env"

installed_capture="$tmp_dir/installed.capture"
HOME="$tmp_dir/installed-home" \
XDG_STATE_HOME="$tmp_dir/installed-state" \
CMUX_TEST_CAPTURE="$installed_capture" \
    "$install_root/bin/cmux-linux-app" --script quit
grep -Fx "library=$install_root/share/cmux/ghostty/lib/libghostty-internal.so" \
    "$installed_capture"
grep -Fx "root=$install_root/share/cmux/ghostty" "$installed_capture"
grep -Fx "bundle_version=test-version" "$installed_capture"

custom_prefix="$tmp_dir/custom prefix"
custom_data="$tmp_dir/custom data"
HOME="$tmp_dir/custom-home" \
PREFIX="$custom_prefix" \
XDG_DATA_HOME="$custom_data" \
    "$relocated/install.sh" >/dev/null
[[ -f "$custom_data/cmux/bundle-version" ]]
[[ -f "$custom_prefix/share/cmux/bundle-version" ]]
[[ -f "$custom_prefix/share/cmux/build-provenance.txt" ]]
"$custom_prefix/bin/cmux" --version >/dev/null

if grep -R -F "$tmp_dir/fixture" "$relocated" >/dev/null; then
    echo "FAIL: bundle contains an absolute fixture build path" >&2
    exit 1
fi

echo "PASS: Linux bundle is complete, relocatable, checksummed, and installable"
