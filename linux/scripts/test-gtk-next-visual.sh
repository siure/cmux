#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
linux_dir="$(cd "$script_dir/.." && pwd)"
cmux="$linux_dir/target/debug/cmux"
golden_dir="$linux_dir/tests/visual/goldens/x11"
output_dir="${CMUX_VISUAL_OUTPUT:-$linux_dir/target/gtk-visual}"

mkdir -p "$output_dir" "$golden_dir"

if [[ "${1:-}" == "--capture" ]]; then
  fixture="$2"
  width="$3"
  height="$4"
  actual="$5"
  socket="$6"
  state_root="$7"

  export HOME="$state_root/home"
  export XDG_CONFIG_HOME="$state_root/config"
  export XDG_STATE_HOME="$state_root/state"
  export XDG_CACHE_HOME="$state_root/cache"
  export CMUX_LINUX_UI=next
  export GDK_BACKEND=x11
  export GSK_RENDERER=cairo
  export GDK_SCALE=1
  export GTK_THEME=Adwaita:dark
  export GTK_A11Y=none
  export NO_AT_BRIDGE=1
  export LC_ALL=C.UTF-8
  export TZ=UTC
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"

  "$cmux" app --renderer gtk --socket "$socket" >"$state_root/app.log" 2>&1 &
  app_pid=$!
  wait_until_stopped() {
    for _ in $(seq 1 200); do
      if ! kill -0 "$app_pid" 2>/dev/null; then
        return 0
      fi
      sleep 0.05
    done
    return 1
  }
  terminate_app() {
    if ! kill -0 "$app_pid" 2>/dev/null; then
      wait "$app_pid" >/dev/null 2>&1 || true
      return
    fi
    kill "$app_pid" >/dev/null 2>&1 || true
    if ! wait_until_stopped; then
      kill -KILL "$app_pid" >/dev/null 2>&1 || true
      for _ in $(seq 1 50); do
        kill -0 "$app_pid" 2>/dev/null || break
        sleep 0.02
      done
    fi
    if ! kill -0 "$app_pid" 2>/dev/null; then
      wait "$app_pid" >/dev/null 2>&1 || true
    fi
  }
  cleanup() {
    "$cmux" --socket "$socket" rpc app.quit.request '{}' >/dev/null 2>&1 || true
    terminate_app
  }
  trap cleanup EXIT INT TERM

  for _ in $(seq 1 200); do
    if "$cmux" --socket "$socket" ping >/dev/null 2>&1; then
      break
    fi
    kill -0 "$app_pid"
    sleep 0.05
  done
  "$cmux" --socket "$socket" ping >/dev/null

  rpc() {
    "$cmux" --socket "$socket" rpc "$1" "$2" >/dev/null
  }

  rpc workspace.rename '{"workspace_id":"workspace:1","title":"GTK UI rewrite"}'
  rpc sidebar.status.set '{"workspace_id":"workspace:1","key":"branch","value":"feat/linux-gtk-ui/sidebar-width-must-not-follow-an-extremely-long-path-derived-status-value-that-keeps-growing","priority":10}'
  rpc surface.send_text '{"surface_id":"surface:1","text":"echo PRIMARY-PANE; echo Linux-GTK-next-shell"}'
  rpc surface.send_key '{"surface_id":"surface:1","key":"enter"}'
  rpc surface.split '{"direction":"right"}'
  rpc surface.send_text '{"surface_id":"surface:2","text":"echo SECOND-PANE; echo compact-native-chrome"}'
  rpc surface.send_key '{"surface_id":"surface:2","key":"enter"}'

  expected_surfaces=2
  if [[ "$fixture" == "dense" ]]; then
    rpc surface.split '{"direction":"down"}'
    rpc surface.send_text '{"surface_id":"surface:3","text":"echo THIRD-PANE; echo notification-ready"}'
    rpc surface.send_key '{"surface_id":"surface:3","key":"enter"}'
    rpc surface.create '{"type":"terminal","focus":true}'
    rpc surface.send_text '{"surface_id":"surface:4","text":"echo SECOND-TAB; echo stable-surface-host"}'
    rpc surface.send_key '{"surface_id":"surface:4","key":"enter"}'
    rpc surface.focus '{"surface_id":"surface:3"}'
    rpc sidebar.right '{"action":"hide"}'
    expected_surfaces=4
  else
    rpc sidebar.right '{"action":"show","mode":"files","no_focus":true}'
    rpc debug.command_palette.toggle '{}'
  fi

  for _ in $(seq 1 100); do
    count="$($cmux --socket "$socket" tree --json | python3 -c 'import json,sys; d=json.load(sys.stdin); print(sum(len(w.get("panes", [])) for window in d.get("windows", []) for w in window.get("workspaces", [])))' 2>/dev/null || printf '0')"
    [[ "$count" -ge 2 ]] && break
    sleep 0.05
  done
  sleep 1

  "$cmux" --socket "$socket" renderer snapshot --json >"$state_root/snapshot.json"
  python3 - "$state_root/snapshot.json" "$expected_surfaces" "$fixture" <<'PY'
import json, sys
snapshot = json.load(open(sys.argv[1]))
expected = int(sys.argv[2])
fixture = sys.argv[3]
surfaces = snapshot.get("window_surfaces", snapshot.get("surfaces", []))
if len(surfaces) != expected:
    raise SystemExit(f"expected {expected} surfaces, got {len(surfaces)}")
right_visible = bool(snapshot.get("right_sidebar", {}).get("visible"))
if right_visible != (fixture == "narrow"):
    raise SystemExit(f"unexpected right sidebar visibility: {right_visible}")
if fixture == "narrow" and not snapshot.get("command_palette", {}).get("visible"):
    raise SystemExit("command palette is not visible in narrow fixture")
PY

  python3 - "$actual" "$width" "$height" <<'PY'
from PIL import ImageGrab
import sys
path, width, height = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
image = ImageGrab.grab()
if image.size != (width, height):
    raise SystemExit(f"unexpected screenshot size {image.size}, expected {(width, height)}")
colors = image.convert("RGB").getcolors(maxcolors=width * height)
if not colors or len(colors) < 16:
    raise SystemExit("GTK screenshot is blank or has too little visual detail")
image.save(path, optimize=True)
PY

  "$cmux" --socket "$socket" rpc app.quit.request '{}' >/dev/null
  if ! wait_until_stopped; then
    cp "$state_root/app.log" "${actual%.png}.log"
    terminate_app
    echo "GTK app did not exit after app.quit.request" >&2
    exit 1
  fi
  wait "$app_pid"
  trap - EXIT INT TERM
  exit 0
fi

if [[ "${CMUX_SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --locked --manifest-path "$linux_dir/Cargo.toml" --features gtk
elif [[ ! -x "$cmux" ]]; then
  echo "CMUX_SKIP_BUILD=1 requires an existing GTK-enabled binary at $cmux" >&2
  exit 1
fi

run_fixture() {
  local fixture="$1"
  local width="$2"
  local height="$3"
  local state_root
  state_root="$(mktemp -d)"
  local socket="$state_root/cmux.sock"
  local actual="$output_dir/gtk-next-$fixture-${width}x${height}.png"
  local golden="$golden_dir/gtk-next-$fixture-${width}x${height}.png"

  dbus-run-session -- xvfb-run -a -s "-screen 0 ${width}x${height}x24" \
    "$0" --capture "$fixture" "$width" "$height" "$actual" "$socket" "$state_root"

  if [[ "${CMUX_UPDATE_GOLDENS:-0}" == "1" ]]; then
    cp "$actual" "$golden"
  elif [[ ! -f "$golden" ]]; then
    echo "missing GTK visual golden: $golden" >&2
    return 1
  elif [[ "${CMUX_VISUAL_SMOKE_ONLY:-0}" != "1" ]]; then
    python3 - "$golden" "$actual" "$output_dir/gtk-next-$fixture-diff.png" <<'PY'
from PIL import Image, ImageChops
import sys, warnings
warnings.filterwarnings("ignore", category=DeprecationWarning)
expected = Image.open(sys.argv[1]).convert("RGB")
actual = Image.open(sys.argv[2]).convert("RGB")
if expected.size != actual.size:
    raise SystemExit(f"golden size {expected.size} differs from actual size {actual.size}")
diff = ImageChops.difference(expected, actual)
diff.save(sys.argv[3], optimize=True)
changed = sum(1 for pixel in diff.getdata() if max(pixel) > 12)
ratio = changed / (actual.width * actual.height)
if ratio > 0.02:
    raise SystemExit(f"visual diff changed {ratio:.2%} of pixels (limit 2.00%)")
PY
  fi
  rm -rf "$state_root"
}

run_fixture dense 1180 760
run_fixture narrow 900 700
printf 'GTK next-shell screenshots: %s\n' "$output_dir"
