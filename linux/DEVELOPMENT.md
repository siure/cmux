# Linux Development

This guide covers the supported contributor path for the Linux port. Read the
[architecture map](ARCHITECTURE.md) before moving code across subsystem
boundaries and use the [roadmap](ROADMAP.md) to decide whether work belongs in
the daily-driver path.

## Git workflow

Linux work integrates through `feat/linux-port`, not `main`:

```bash
git fetch origin feat/linux-port
git switch feat/linux-port
git pull --ff-only origin feat/linux-port
git switch -c <type>/<short-linux-topic>
```

Keep the integration checkout clean and use one focused topic branch per
change. The complete remote, worktree, and PR rules are in the
[Linux Git workflow](../docs/linux-port-git-workflow.md).

## Native prerequisites

The first release baseline is Ubuntu 26.04 LTS with a native GNOME Wayland
session. Install the compiler, native libraries, and tools used by the
documented bootstrap path:

```bash
sudo apt update
sudo apt install --no-install-recommends \
  gcc g++ libegl1-mesa-dev libgtk-4-dev libwebkitgtk-6.0-dev \
  mesa-utils pkg-config curl python3 xz-utils
```

Install `rustup` when it is not already available, then install and select the
Rust 1.92.0 Linux CI baseline for the current shell without changing the global
default:

```bash
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
  . "$HOME/.cargo/env"
fi
rustup toolchain install 1.92.0 --profile minimal --component rustfmt
export RUSTUP_TOOLCHAIN=1.92.0
rustc --version
cargo --version
```

Newer Rust toolchains may work, but 1.92.0 is the reproducible baseline.
`run-dev.sh` reads Ghostty's pinned Zig requirement and provisions that exact
version under the XDG cache when the current `zig` does not match. Set
`CMUX_ZIG=/absolute/path/to/zig` to use an existing exact version instead.

Install these additional packages only for Xvfb smoke tests:

```bash
sudo apt install --no-install-recommends dbus-x11 xauth xvfb
```

Package names for Fedora, Arch, and openSUSE remain in the
[detailed reference](REFERENCE.md#gtkghostty-renderer-prerequisites), but those
distributions are contributor-tested until promoted in the roadmap's supported
environment matrix.

## Build and launch

From the repository root:

```bash
./linux/scripts/run-dev.sh
```

This command:

1. resolves the Ghostty SHA pinned by the parent repository
2. initializes the submodule when needed and refuses to overwrite local
   submodule work
3. obtains the exact Zig version required by Ghostty
4. builds the Linux embedding library with `ReleaseSafe`
5. builds cmux with locked dependencies and the `gtk` feature
6. launches `cmux app --renderer ghostty` on the selected socket

The app remains in the foreground. Control it from another terminal with
`linux/target/debug/cmux`.

For an isolated development state and socket:

```bash
dev_root=$(mktemp -d)
printf 'Isolated state: %s\n' "$dev_root"
XDG_CONFIG_HOME="$dev_root/config" \
XDG_STATE_HOME="$dev_root/state" \
XDG_CACHE_HOME="$dev_root/cache" \
CMUX_LINUX_SOCKET_PATH="$dev_root/cmux.sock" \
./linux/scripts/run-dev.sh
```

Keep the printed `dev_root` path until you have inspected any state needed for
debugging. Remove it only when you are sure it contains no work you need.

## Display-free development

The default build does not link GTK and is the quickest loop for the shared
model, socket protocol, and CLI:

```bash
cargo build --locked --manifest-path linux/Cargo.toml
cargo run --locked --manifest-path linux/Cargo.toml -- serve
```

From a second terminal:

```bash
cargo run --locked --manifest-path linux/Cargo.toml -- ping
cargo run --locked --manifest-path linux/Cargo.toml -- --json identify
```

## Tests and checks

Run formatting and the display-free suite for every Rust change:

```bash
cargo fmt --manifest-path linux/Cargo.toml -- --check
cargo test --locked --manifest-path linux/Cargo.toml
```

When GTK-gated code or a module boundary used by GTK changes, also run:

```bash
cargo test --locked --manifest-path linux/Cargo.toml --features gtk
cargo build --locked --manifest-path linux/Cargo.toml --features gtk
```

The native suite requires the GTK and WebKitGTK development files. A passing
display-free suite does not validate the embedded terminal, clipboard, IME,
GL, or compositor behavior.

The 2026-08-20 ownership audit found a pre-existing test-harness limitation:
the complete GTK feature test binary can initialize process-global GTK from
different test threads or query the icon theme without a display. On the audit
host this makes the full command above nondeterministic and can end in a
headless SIGSEGV. The headless serial SIGSEGV independently reproduces at the
pre-cleanup integration commit. On the ownership branch, each reported GTK
test passes alone and the live Xvfb app smoke passes. Focused tests are useful
for diagnosis, but they do not make the full GTK release gate green. Fix the
harness to run display-backed GTK tests on one owned GTK thread or in isolated
test processes before treating that gate as reliable.

## Renderer diagnostics

With Ghostty already built, inspect the exact embedding library and resources:

```bash
export CMUX_GHOSTTY_LIBRARY="$PWD/ghostty/zig-out/lib/libghostty-internal.so"
export CMUX_GHOSTTY_ROOT="$PWD/ghostty"
linux/target/debug/cmux app --renderer ghostty \
  --script $'renderer diagnostics --backend ghostty\nquit'
```

The daily-driver gate expects the embedding to be available, required symbols
and runtime resources to be present, unexpected exported symbols to be absent,
and ABI layout and constant fingerprints to match.

For a repeatable Xvfb smoke that keeps the native app alive while a second
process verifies the live Ghostty surface:

```bash
smoke_root=$(mktemp -d)
socket_path="$smoke_root/cmux.sock"
XDG_CONFIG_HOME="$smoke_root/config" \
XDG_STATE_HOME="$smoke_root/state" \
CMUX_LINUX_SOCKET_PATH="$socket_path" \
GDK_BACKEND=x11 \
timeout 600s dbus-run-session -- xvfb-run -a sh -ceu '
  launcher=$1
  client=$2
  socket=$3
  evidence=$4
  "$launcher" >"$evidence/app.log" 2>&1 &
  app_pid=$!
  trap '\''kill "$app_pid" 2>/dev/null || true'\'' EXIT INT TERM
  attempt=0
  until "$client" --socket "$socket" ping >/dev/null 2>&1; do
    kill -0 "$app_pid"
    attempt=$((attempt + 1))
    test "$attempt" -lt 1200
    sleep 0.1
  done
  "$client" --socket "$socket" --json \
    renderer diagnostics --backend ghostty >"$evidence/diagnostics.json"
  "$client" --socket "$socket" send \
    --workspace workspace:1 --surface surface:1 "printf CMUX_GHOSTTY_LIVE"
  "$client" --socket "$socket" send-key \
    --workspace workspace:1 --surface surface:1 enter
  attempt=0
  until "$client" --socket "$socket" read-screen \
      --workspace workspace:1 --surface surface:1 \
      >"$evidence/screen.txt" \
      && grep -q CMUX_GHOSTTY_LIVE "$evidence/screen.txt"; do
    kill -0 "$app_pid"
    attempt=$((attempt + 1))
    test "$attempt" -lt 100
    sleep 0.1
  done
  "$client" --socket "$socket" --json rpc \
    app.quit.request '{"source":"socket"}' >"$evidence/quit.json"
  wait "$app_pid"
' sh ./linux/scripts/run-dev.sh ./linux/target/debug/cmux \
  "$socket_path" "$smoke_root"
printf 'Smoke evidence: %s\n' "$smoke_root"
```

This verifies a running GTK process and live Ghostty-backed terminal I/O, not
only pre-launch diagnostics. Xvfb still does not replace the native GNOME
Wayland smoke required by the roadmap.

## Ghostty changes

Most cmux changes should consume the pinned Ghostty commit without modifying
it. Before intentional fork work, read
[`docs/ghostty-fork.md`](../docs/ghostty-fork.md) and the repository's submodule
rules.

At minimum:

- create an attached branch in `ghostty/`
- commit the Ghostty change inside the submodule
- push that commit to the configured fork before changing the parent pointer
- verify the commit is reachable from the fork's intended integration branch
- update the fork notes when the Linux ABI or conflict surface changes

Never leave a parent commit pointing to an unpublished or detached submodule
commit.

## Development install and bundle

Unlike `run-dev.sh`, the install and bundle scripts currently invoke `zig`
from `PATH`. After `run-dev.sh` has provisioned the pinned version, expose that
cached binary in the current shell before packaging:

```bash
required_zig=$(sed -nE \
  's/^[[:space:]]*\.?minimum_zig_version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' \
  ghostty/build.zig.zon | head -n 1)
zig_arch=$(uname -m)
test "$zig_arch" != arm64 || zig_arch=aarch64
cached_zig="${XDG_CACHE_HOME:-$HOME/.cache}/cmux/zig/zig-${zig_arch}-linux-${required_zig}/zig"
test -x "$cached_zig"
test "$("$cached_zig" version)" = "$required_zig"
export PATH="$(dirname "$cached_zig"):$PATH"
```

If the cached binary is absent, run `run-dev.sh` first or install the exact
version yourself. A different system `zig` is not a supported substitute.

Install a development build into the user-local XDG paths:

```bash
./linux/scripts/install-dev.sh
gtk-launch ai.manaflow.cmux
```

Build the relocatable archive and checksum:

```bash
./linux/scripts/build-bundle.sh
```

The release contract requires installing the resulting bundle into a clean
user-local prefix and launching its desktop entry without depending on paths
from the source checkout. Detailed environment overrides and archive contents
are in [Reference](REFERENCE.md#relocatable-linux-bundle).

## Debugging checklist

When the app builds but the terminal does not render:

1. confirm `git -C ghostty rev-parse HEAD` matches `git rev-parse HEAD:ghostty`
2. run renderer diagnostics and inspect the selected library and resource root
3. confirm the GTK development files are visible through `pkg-config`
4. distinguish Xvfb, X11/XWayland, and native Wayland behavior
5. retain the isolated XDG state and application log with the reproduction

GPU or Vulkan warnings under Xvfb are not automatically Ghostty ABI failures.
The useful signal is whether the live surface initializes and the renderer
diagnostics pass.

## Change checklist

Before handing off a Linux change:

- keep the change inside one documented subsystem boundary
- preserve the shared action path across CLI, socket, and GTK entry points
- run focused tests plus the suites required by the touched feature gates
- run a live native smoke for Ghostty, GTK, clipboard, IME, GL, or compositor
  changes
- verify no unintended Ghostty pointer or lockfile change is present
- update architecture, development, roadmap, or reference text when its claim
  changed
