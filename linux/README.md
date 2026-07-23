# cmux Linux Port

This directory contains the Rust Linux port of cmux.

The Linux port centers on a shared Rust app core plus a `cmux` CLI that speaks
the same JSON-lines Unix socket protocol used by the macOS tests. The
display-free core remains the default build for contract tests and automation,
while optional GTK and Ghostty renderer paths layer on top of the same state
model for the native Linux app shell.

## Run

For the complete native app, run these commands from the `cmux` checkout with
the modified Ghostty checkout beside it:

```bash
(cd ../ghostty && zig build -Dapp-runtime=none -Doptimize=ReleaseSafe)
cargo build --locked --manifest-path linux/Cargo.toml --features gtk

export CMUX_GHOSTTY_LIBRARY="$PWD/../ghostty/zig-out/lib/libghostty-internal.so"
export CMUX_GHOSTTY_ROOT="$PWD/../ghostty"
linux/target/debug/cmux app --renderer ghostty
```

The process remains in the foreground and owns the normal control socket under
`$XDG_STATE_HOME/cmux` or `~/.local/state/cmux`. Run `cmux` commands from a
second terminal to control that same app. Use `--socket <path>` on the app and
CLI when an isolated development socket is preferable.

For a persistent user-local desktop install:

```bash
./linux/scripts/install-dev.sh
gtk-launch ai.manaflow.cmux
```

The display-free core can be run without GTK or Ghostty:

```bash
cargo run --manifest-path linux/Cargo.toml -- serve
```

In another terminal:

```bash
cargo run --manifest-path linux/Cargo.toml -- ping
cargo run --manifest-path linux/Cargo.toml -- --json identify
```

The app shell can also run against the fallback renderers:

```bash
cargo run --manifest-path linux/Cargo.toml -- app
cargo run --manifest-path linux/Cargo.toml --features gtk -- app --renderer gtk
CMUX_LINUX_RENDERER=ghostty-vt cargo run --manifest-path linux/Cargo.toml -- app
```

Feedback submitted through the running app is written to a private queue before
cmux posts it to `https://cmux.com/api/feedback`. Pending reports can be retried
after connectivity returns:

```bash
cargo run -- feedback --email you@example.com --body "Linux feedback"
cargo run -- feedback retry
```

`CMUX_FEEDBACK_API_URL` overrides the endpoint for development.
`CMUX_FEEDBACK_DELIVERY=local` disables network delivery while retaining the
private queue, and `CMUX_FEEDBACK_DIR` overrides its location. Email and message
limits match the production endpoint. Up to ten GIF, HEIC/HEIF, JPEG, PNG,
TIFF, or WebP attachments are accepted, with a 4 MB per-file and combined
attachment limit. Queue directories use mode `0700`; records and copied
attachments use mode `0600`. The queue accepts at most 100 pending reports,
retains at most 50 sent/rejected records, and removes copied attachment data
after successful delivery.

When the GTK shell is active, `cmux feedback` opens a native email/message form
with multi-image selection, attachment clearing, background submission, and
sent/queued/error status. Display-free sessions retain the compatibility
browser document and use the same socket/CLI submission methods.

### Browser Import

The Linux browser profile store imports cookies, history, bookmark folder
trees, and portable browser settings from detected Firefox, Chrome, Chromium,
Brave, and Edge profiles. Imported bookmarks participate in native omnibar
suggestions, and imported search providers apply only to their destination cmux
profile. The Browser settings page exposes detected source/profile and
destination selectors.

```bash
cmux browser import --from Firefox --profile default --scope all
cmux browser import --from Chrome --profile Default --scope bookmarks
cmux browser bookmarks search docs --profile default
```

Explicit imports accept JSON or native Chromium bookmark/preferences files via
`--bookmarks-file` and `--settings-file`; Firefox `prefs.js` is accepted for
portable settings. Browser profile state is bounded and persisted privately.
Encrypted Chromium cookies that require unavailable keyring decryption are
skipped with a warning. Firefox/Chromium extensions cannot execute inside
WebKitGTK and are reported as not imported.

### Custom Sidebars

The Linux GTK shell discovers custom left sidebars from
`~/.config/cmux/sidebars`, using the same name and file-precedence contract as
macOS (`<name>.swift` wins over `<name>.json`). Linux renders the declarative
JSON document format and interprets a bounded SwiftUI-style subset against live
workspace state. Swift sidebars support view/value helpers, loops and
conditionals, parameterized cmux actions, hot reload, last-good fallback, and
persisted `Reorderable` workspace rows. Simple `struct: View` declarations
support stored properties, defaults, synthesized labeled memberwise arguments,
`self` member reads, computed `body`, and nested state. Top-level and per-row `@State`
declarations survive re-evaluation and app restarts; dynamic rows derive
independent state from `ForEach`/`Reorderable` identity and retain it across
reordering. Button actions can assign, add, toggle, or append state, and GTK
renders two-way-bound `Toggle`, `TextField`, `Slider`, `Picker`, and `Stepper`
controls. `.onChange(of:)` callbacks run after any matching state write with
old/new closure values, and `.onSubmit` handlers run from descendant text
fields.

Install the included example and select it against a running app:

```bash
mkdir -p ~/.config/cmux/sidebars
cp linux/examples/custom-sidebar.json ~/.config/cmux/sidebars/linux-status.json
cp linux/examples/stateful-sidebar.swift ~/.config/cmux/sidebars/stateful.swift
cp linux/examples/enum-sidebar.swift ~/.config/cmux/sidebars/enum-sidebar.swift
cmux sidebar validate linux-status
cmux sidebar select linux-status
cmux sidebar clear-state linux-status
```

Use `cmux sidebar select workspaces` to restore the built-in workspace list.
The provider picker in the left-sidebar header exposes the same choices in
GTK while `customSidebars.beta.enabled` is enabled (the shared default is
`true`). Turning the beta off falls back to Workspaces without deleting the
saved provider selection. Selection is persisted privately under
`$XDG_STATE_HOME/cmux` or
`~/.local/state/cmux`; edits are re-read from renderer snapshots, and a broken
save keeps the last good document visible with an error banner. Sidebar state is
isolated by provider in private `custom-sidebar-state.json` storage. Changing a
declaration's value type resets that value to its new initializer; changing an
initializer without changing its type preserves the current value.
Declarative JSON control bindings use the same store: their document value seeds
the key once, and later renders reuse the persisted typed value.

JSON documents use schema version `1` and a recursive `root` node. Supported
node types are `vstack`, `hstack`, `zstack`, `text`, `button`, `image`,
`spacer`, `divider`, `progress`, `shape`, `toggle`, `textfield`, `slider`,
`picker`, and `stepper`.
Layout/style fields match the
shared declarative contract, including dimensions, opacity, and corner radius.
A button action may invoke a cmux method directly or carry ordered
parameterized commands. Documents are bounded to 1 MiB, 4,096 nodes, and 64
levels of nesting.

Swift interpretation defaults to the host process. Set the isolated worker in
`~/.config/cmux/cmux.json`, or use the native Custom Sidebars settings page:

```json
{"customSidebars":{"renderer":"remote"}}
```

Valid values are `inProcess` and `remote`. The remote lane re-executes the cmux
binary with a bounded JSON protocol, a two-second timeout, and bounded output;
worker failure leaves the last good document visible.

`CMUX_CUSTOM_SIDEBARS_DIR`, `CMUX_CUSTOM_SIDEBAR_SELECTION_PATH`, and
`CMUX_CUSTOM_SIDEBAR_STATE_PATH` override the source directory, selection file,
and state file for development and tests.

When `--renderer` is omitted, `cmux app` reads `CMUX_LINUX_RENDERER` and
falls back to `core` if it is unset. An explicit `--renderer` value overrides
the environment default.

The top-level `cmux renderer ...` CLI also exposes these operations, but that
form talks to `CMUX_SOCKET_PATH` and expects a running `cmux serve` or
`cmux app --socket ...` process. Use the `cmux app --script 'renderer ...'`
form above for local in-process diagnostics.

Generated Linux agent launcher shims and the OMO shadow OpenCode config are
written under `$XDG_CACHE_HOME/cmux/agent-launchers` or
`~/.cache/cmux/agent-launchers`.

### GTK/Ghostty Renderer Prerequisites

The display-free core, socket contract, and `ghostty-vt` renderer tests do not
require GTK. The GTK shell and full Ghostty GL host require GTK 4 and
WebKitGTK 6 development files so Cargo can compile the embedded request
extension and link against GTK:

```bash
# Fedora/RHEL
sudo dnf install gtk4-devel webkitgtk6.0-devel pkgconf-pkg-config

# Debian/Ubuntu
sudo apt install libgtk-4-dev libwebkitgtk-6.0-dev pkg-config

# Arch
sudo pacman -S gtk4 webkitgtk-6.0 pkgconf

# openSUSE
sudo zypper install gtk4-devel webkitgtk-6_0-devel pkgconf-pkg-config
```

Verify the native dependencies with `pkg-config --modversion gtk4` and
`pkg-config --modversion webkitgtk-web-process-extension-6.0`. Builds with
`--features gtk` fail early if either development package or the unversioned
`libgtk-4.so` library is missing. The renderer diagnostics report `gtk4.runtime_library`,
`gtk4.link_library_available`, `gtk4.development_files_available`, and an
install hint:

```bash
cargo run -- app --renderer ghostty --script $'renderer diagnostics\nquit'
```

GTK browser surfaces use the WebKitGTK 6 runtime when
`libwebkitgtk-6.0.so.4` is installed. WebKit is loaded dynamically, so it is
not required for display-free builds and a missing runtime falls back to the
bounded browser-model preview. Successful DOM interaction, input, scrolling,
script, and style automation commands are also applied to the live WebKit
document; sequence tracking preserves command order and defers actions while a
new document is loading. Configured headers and Basic credentials are applied
inside WebKit's web process to every document and subresource request; HTTP
authentication challenges use the same credentials. `browser.useragent.set`
updates the native WebKit settings, and `browser.addinitscript` registers
document-start scripts that remain active across subsequent navigation and
history loads. Locale,
timezone, media/color-scheme/reduced-motion, online status, device/touch,
geolocation, and permission emulation is installed at document start and
updated immediately in the active page. Offline emulation changes
`navigator.onLine` and page events and routes native WebKit requests through an
unreachable per-surface proxy until online mode is restored.
Explicit `browser.storage.set` and `browser.storage.clear` mutations, including
state restores, replace local/session storage in the live document without
clearing an untouched WebKit profile on initial load. Browser state files
contain the URL, cookies, local storage, session storage, and frame selector;
loading also accepts the legacy flat local-storage map. On Wayland, cmux
disables WebKit's DMA-BUF renderer by default to avoid blank web-content
surfaces on compositors or GPU drivers that cannot import its buffers; set
`WEBKIT_DISABLE_DMABUF_RENDERER` explicitly to override that compatibility
default. Socket `browser.eval`, `browser.evalhandle`, DOM-backed `browser.get`,
and visibility, enabled, and checked reads return values from the mounted
WebKit document, including resolved promises. `browser.snapshot` also traverses
the mounted document and installs its ephemeral `eN` selector refs for later
automation commands. These operations retain deterministic model results when
the native view is unavailable.

### Linux MVP Manual Smoke

Build the local Ghostty fork and cmux from the repository root, then launch the
GTK app on a private control socket:

```bash
(cd ../ghostty && zig build -Dapp-runtime=none -Doptimize=ReleaseSafe)
cargo build --locked --manifest-path linux/Cargo.toml --features gtk

export CMUX_GHOSTTY_LIBRARY="$PWD/../ghostty/zig-out/lib/libghostty-internal.so"
export CMUX_GHOSTTY_ROOT="$PWD/../ghostty"
linux/target/debug/cmux app --renderer ghostty --socket /tmp/cmux-mvp.sock
```

In another terminal, use the refs returned by each create command when the
session is not fresh. The refs below are from a fresh launch:

```bash
CMUX="linux/target/debug/cmux --socket /tmp/cmux-mvp.sock --json"

$CMUX renderer diagnostics --backend ghostty
$CMUX workspace create --name "MVP Workspace"
$CMUX workspace select workspace:2
$CMUX tab-action --workspace workspace:2 --action new-terminal-right
$CMUX new-split --workspace workspace:2 --direction right
$CMUX send --workspace workspace:2 --surface surface:3 "printf 'CMUX_GHOSTTY_OK\\n'"
$CMUX send-key --workspace workspace:2 --surface surface:3 enter
$CMUX read-screen --workspace workspace:2 --surface surface:3
$CMUX browser open https://example.com
$CMUX browser navigate https://example.org --surface surface:4
$CMUX browser back --surface surface:4
$CMUX browser forward --surface surface:4
$CMUX focus-panel --workspace workspace:2 --surface surface:3
$CMUX tree
```

The MVP covers the native app launch, workspace switching, terminal tabs,
split panes, embedded interactive Ghostty terminals, native WebKit browser
surfaces, socket-driven focus/history navigation, and native close-confirmation
dialogs for tab, window, and application quit requests. It does not claim
macOS feature parity. Stack authentication, cloud VM operations, SSH
workspaces, remote-daemon relaying, and the core mobile protocol are
implemented. Remaining work is concentrated in production desktop
distribution, advanced custom-sidebar controls and interpreted type-system
features, complete mobile client validation, provider-specific agent polish, and the remaining
macOS-only settings and system integrations. Full terminal embedding requires
this checkout's local Ghostty Linux ABI changes; an unmodified upstream Ghostty
build is not sufficient.

### Remaining Full-Port Work

The Linux port is usable, but full product parity still requires:

- Publishing the modified Ghostty checkout and pinning its immutable commit in
  Linux release CI. Local source builds already require and validate this ABI.
- Shipping and validating stable/nightly Linux archives through the normal
  release channel, including updater installation against real published
  assets and broader distro, compositor, and GPU coverage.
- Completing the remaining custom-sidebar interaction surface after the shipped
  persisted top-level/per-row `@State`, button mutation, native input controls,
  picker/tag matching, simple custom `View` structs, enums, and `switch`: custom
  initializers, generic `@ViewBuilder` composition, composed `@Binding` values,
  optional/guard control flow, and navigation/presentation controls.
- Browser-extension migration policy beyond the current explicit incompatibility
  result. Linux imports cookies, history, bookmark folder trees, homepage,
  bookmark-bar visibility, and search-provider settings from Firefox and
  Chromium-family profiles, but those browsers' native extensions cannot run
  inside WebKitGTK.
- End-to-end validation with the real mobile client across Tailscale routes,
  reconnects, long-running subscriptions, image transfer, and mixed desktop
  and mobile mutation races. The Linux host protocol and attach service are
  implemented.
- Provider-specific agent hardening against new Codex, Claude Code, and
  OpenCode protocol versions, plus broader recovery testing for interrupted
  approvals, partial streams, and provider upgrades.
- Remaining macOS-only desktop integrations and settings where Linux needs a
  native equivalent or an explicit unsupported policy, followed by visual,
  accessibility, input-method, and multi-monitor parity testing.

### Sidebar Extensions

Linux provides a process-isolated sidebar Extensions host alongside declarative
custom sidebars. Extensions are discovered under
`~/.config/cmux/extensions/<id>/manifest.json`, request the same read and action
scopes as the macOS API v2 manifest, and render the bounded native sidebar
document format from an executable `entrypoint`.

Access is denied until explicitly approved. Manifest changes invalidate prior
grants, every action is checked again at dispatch, and raw cmux methods and
reorder methods are rejected. The production lane uses Bubblewrap with no
network, no inherited environment, a private `/tmp`, read-only extension and
system runtime mounts, bounded I/O, and a two-second timeout. Manage installed
extensions with `sidebar.extension.list|select|grant|revoke|reload`; the native
sidebar provider picker and Beta Features settings use the same host.

Install the included example:

```bash
sudo dnf install bubblewrap # use the equivalent package on other distributions
mkdir -p ~/.config/cmux/extensions/dev.cmux.linux.workspace-list
cp linux/examples/sidebar-extension/* \
  ~/.config/cmux/extensions/dev.cmux.linux.workspace-list/
chmod +x \
  ~/.config/cmux/extensions/dev.cmux.linux.workspace-list/extension.py
```

Enable **Settings > Beta Features > Extensions**, choose **Linux Workspace
List** from the sidebar provider menu, and approve its requested access. The
same flow is available over the socket:

```bash
cmux --json rpc sidebar.extension.list '{}'
cmux --json rpc sidebar.extension.select \
  '{"id":"dev.cmux.linux.workspace-list"}'
cmux --json rpc sidebar.extension.grant \
  '{"id":"dev.cmux.linux.workspace-list"}'
```

### Relocatable Linux Bundle

Build a release archive from the cmux tree and the local Ghostty checkout:

```bash
./linux/scripts/build-bundle.sh
(cd dist && sha256sum -c cmux-linux-$(uname -m).tar.gz.sha256)
mkdir -p /tmp/cmux-linux
tar -xzf dist/cmux-linux-$(uname -m).tar.gz -C /tmp/cmux-linux
/tmp/cmux-linux/cmux-linux-$(uname -m)/bin/cmux-linux-app
```

The archive contains stripped `cmux` and `cmuxd-remote` binaries, the Linux
`libghostty-internal` embedding DSO, the portable `libghostty-vt` DSO when the
Ghostty build emits it, headers used for ABI diagnostics, themes, shell
integration, a desktop entry, and the application icon. The launcher resolves
the Ghostty tree relative to its own `bin` directory, so the extracted bundle
can be moved without regenerating absolute paths. Each archive also carries
`share/cmux/build-provenance.txt` with the schema, version, architecture, cmux
commit, Ghostty repository, and Ghostty commit used to build it. It also records
`cmux_dirty` and `ghostty_dirty`; local artifacts cannot imply that uncommitted
changes are contained in the recorded commits, while CI release artifacts
record both values as `false`.

The manually dispatchable `Build Linux Desktop App` GitHub Actions workflow
builds the same validated archive from a published `manaflow-ai/ghostty`
revision:

```bash
gh workflow run build-linux-app.yml \
  -f ghostty_ref=<published-ghostty-commit> \
  -f bundle_version=<version>
```

The workflow pins Rust and Zig, verifies the Zig download checksum, validates
the relocated bundle, and uploads the archive, checksum, and a JSON provenance
manifest. Stable and nightly desktop workflows call it automatically using the
repository variable `CMUX_LINUX_GHOSTTY_REF`; workflow dispatches may override
that value with `linux_ghostty_ref`. The value must be a published
40-character commit SHA from `manaflow-ai/ghostty`, not a branch or mutable tag.
The current local Linux Ghostty changes must therefore be committed and
published before configuring the variable. The workflow intentionally does not
execute untrusted arbitrary Ghostty repositories.

Stable releases publish `cmux-linux-x86_64.tar.gz`, its checksum, and
`cmux-linux-build.json` alongside the macOS and remote-daemon assets. Nightly
releases replace those same Linux asset names after the existing current-main
guard succeeds. Both lanes attest the archive, checksum, and provenance
manifest.

The local Linux release path has also been validated end to end with clean
optimized cmux and `ReleaseSafe` Ghostty prefixes: the resulting archive is
byte-reproducible for identical inputs, relocates through paths containing
spaces, launches the real GTK/Ghostty UI, accepts socket control, and installs
under an arbitrary prefix. This does not replace publication: the Ghostty
changes must still be committed and pushed before setting
`CMUX_LINUX_GHOSTTY_REF`, and `cmux update` remains unavailable until a stable
or nightly GitHub release contains the archive and checksum.

Run the included installer for desktop integration:

```bash
./cmux-linux-$(uname -m)/install.sh
PREFIX=/opt/cmux ./cmux-linux-$(uname -m)/install.sh
```

The default prefix is `~/.local`. The installer copies the already-built
artifacts and does not require Cargo, Zig, or GTK development headers. Running
the application requires the GTK 4 runtime; WebKitGTK 6 is optional and enables
native browser surfaces. The builder validates the source Ghostty ABI before
packaging and validates the relocated launcher again after stripping. It emits
`dist/cmux-linux-<arch>.tar.gz` and a matching `.sha256` file.

For packaging tests or reuse of previously validated artifacts, set
`CMUX_LINUX_BUNDLE_BUILD=0`, `CMUX_GHOSTTY_PREFIX`,
`CMUX_LINUX_BUNDLE_CMUX_BINARY`, and `CMUX_LINUX_BUNDLE_REMOTE_BINARY`.
`CMUX_LINUX_BUNDLE_VALIDATE=0` is reserved for fixture tests; production
archives should retain both validation passes.

Check the published stable Linux release without starting the app:

```bash
cmux update check
cmux --json update status
cmux update install
cmux update install --yes --prefix "$HOME/.local"
```

The checker uses the bundle's installed version metadata, selects
`cmux-linux-<arch>.tar.gz`, fetches its matching `.sha256` release asset, and
requires an exact valid SHA-256 entry before reporting the archive as
installable. Installation streams a bounded archive into a private XDG cache,
verifies the hash before extraction, rejects unsafe paths, links, special
entries, wrong roots, and oversized entry sets, and then runs the bundle's
existing installer. Non-interactive use requires `--yes`; `--force` permits a
verified reinstall when the current version is already equal or newer. Restart
the running app after installation. The Ghostty update action opens the check
result in a native GTK dialog.

### Development Desktop Install

For a user-local Linux app launcher, build and install the source port under
XDG paths:

```bash
./linux/scripts/install-dev.sh
gtk-launch ai.manaflow.cmux
```

The installer builds `cmux` and `cmuxd-remote`, installs a desktop entry, SVG
icon, and launcher defaults under `~/.local/share`, and installs `cmux`,
`cmuxd-remote`, and the `cmux-linux-app` launcher under `~/.local/bin`.
The desktop entry advertises directory, text, diff/patch, and URL launch
support through the wrapper; the installer refreshes the desktop database when
`update-desktop-database` is available, but it does not change your default
file or browser handlers. `http://`, `https://`, and `file://` activations open
browser/file surfaces, while `cmux://settings` and `cmux://settings/<target>`
route to the Linux settings surface.
Installing `cmuxd-remote` beside `cmux` keeps Linux SSH bootstrap and
`cmux remote-daemon-status` working from the desktop install without requiring
`CMUX_LINUX_REMOTE_DAEMON_PATH`. By default, the installer enables the Cargo
`gtk` feature only for `CMUX_LINUX_RENDERER=gtk` and
`CMUX_LINUX_RENDERER=ghostty`; `core` and `ghostty-vt` installs stay
display-free and do not require GTK development files unless
`CMUX_LINUX_CARGO_FEATURES=gtk` is set explicitly. When
`CMUX_LINUX_RENDERER=ghostty` (the default), it also builds the sibling Ghostty
checkout with `zig build -Dapp-runtime=none
-Doptimize=ReleaseSafe` into `~/.local/share/cmux/ghostty` so the launcher can
set `CMUX_GHOSTTY_LIBRARY` and `CMUX_GHOSTTY_ROOT` without depending on the
current working directory. When `CMUX_LINUX_RENDERER=ghostty-vt`, it builds
the same checkout with `zig build -Demit-lib-vt=true -Doptimize=ReleaseSafe`
into that prefix and sets `CMUX_GHOSTTY_VT_LIBRARY` plus `CMUX_GHOSTTY_ROOT`
for the portable VT renderer.
After resolving a full Ghostty or `ghostty-vt` artifact, the installer runs the
freshly built `cmux app --renderer ... --script 'renderer diagnostics ...'`
path with the same `CMUX_GHOSTTY_*` environment it is about to persist. Full
Ghostty installs must report `embedding_status=available`,
`linux_embedding_supported=true`, hidden Darwin-only/internal helper symbols,
no unexpected non-embedding dynamic exports, and complete runtime resources;
`ghostty-vt` installs must report `vt_supported=true`. If validation fails, the
installer prints the diagnostics payload and exits before writing the launcher
environment.
The desktop entry accepts local files and directories from file managers and
opens them through the same `cmux open` app-state path used by the CLI. The
launcher starts the app on the default control socket at
`$XDG_STATE_HOME/cmux/cmux.sock` or `~/.local/state/cmux/cmux.sock`, exports that
path to app terminals as `CMUX_SOCKET_PATH`, and routes later file-manager opens
to the already-running app when the socket is reachable. Set
`CMUX_LINUX_SOCKET_PATH` during installation to persist a different launcher
default while still allowing runtime `CMUX_SOCKET_PATH` overrides.

Useful overrides:

```bash
PREFIX=/opt/cmux-dev ./linux/scripts/install-dev.sh
CMUX_GHOSTTY_CHECKOUT=/path/to/ghostty ./linux/scripts/install-dev.sh
CMUX_LINUX_BUILD_GHOSTTY=0 CMUX_GHOSTTY_ROOT=/path/to/ghostty-prefix ./linux/scripts/install-dev.sh
CMUX_LINUX_BUILD_GHOSTTY=0 CMUX_GHOSTTY_LIBRARY=/path/to/ghostty/zig-out/lib/libghostty-internal.so ./linux/scripts/install-dev.sh
CMUX_LINUX_BUILD_GHOSTTY=0 CMUX_GHOSTTY_LIBRARY=/path/to/libghostty-internal.so CMUX_GHOSTTY_ROOT=/path/to/ghostty-prefix ./linux/scripts/install-dev.sh
CMUX_LINUX_RENDERER=ghostty-vt CMUX_LINUX_BUILD_GHOSTTY=0 CMUX_GHOSTTY_VT_LIBRARY=/path/to/libghostty-vt.so ./linux/scripts/install-dev.sh
CMUX_LINUX_RENDERER=gtk ./linux/scripts/install-dev.sh
CMUX_LINUX_RENDERER=core ./linux/scripts/install-dev.sh
CMUX_LINUX_RENDERER=ghostty-vt ./linux/scripts/install-dev.sh
CMUX_LINUX_SOCKET_PATH=$HOME/.local/state/cmux/dev.sock ./linux/scripts/install-dev.sh
PKG_CONFIG=/path/to/pkg-config ./linux/scripts/install-dev.sh
```

`CMUX_LINUX_RENDERER` is validated as `core`, `gtk`, `ghostty`, or
`ghostty-vt`. The `gtk` and full `ghostty` renderers require
`CMUX_LINUX_CARGO_FEATURES` to include `gtk`; this is the installer default for
those renderers. The `core` and `ghostty-vt` renderer defaults leave
`CMUX_LINUX_CARGO_FEATURES` empty. The same renderer variable is honored by
direct `cmux app` invocations when `--renderer` is not passed. Set
`CMUX_LINUX_BUILD_GHOSTTY=0` with `CMUX_GHOSTTY_VT_LIBRARY` or
`CMUX_GHOSTTY_ROOT` to reuse an existing VT build for `ghostty-vt` installs.
The GTK dependency preflight and renderer diagnostics honor `PKG_CONFIG`,
matching Cargo's override contract for nonstandard toolchains and sysroots.

## Current Scope

- Unix socket JSON-line protocol v2.
- CLI global flags, socket discovery, refs/UUID output formatting.
- A first interactive Linux app shell (`cmux app`) backed by the same core state
  as the daemon, with window, Linux display assignment, workspace navigation,
  pane focus, split, surface focus/close, current-object readbacks, send/read,
  browser, settings, config, themes, and layout commands. When launched with `--socket`,
  external `cmux` CLI/RPC clients control the same running app state.
- A renderer-facing snapshot and diagnostics API (`renderer.snapshot` and
  `renderer.diagnostics`) exposed through `cmux renderer ...` and the app shell.
  This provides the stable layout/surface/chrome contract for the GTK UI layer,
  including sidebar status/log rows, right-sidebar visibility/mode/feed state,
  notifications, and command-palette state,
  and accepts an explicit `window_id` so each native window can render its own
  selected workspace without changing the process-wide focused-window model,
  while keeping the default build display-free. Diagnostics distinguish the
  full Ghostty embedding ABI from the portable `libghostty-vt` terminal core
  produced by `zig build -Demit-lib-vt=true` in the Ghostty checkout, verify the
  Linux app-thread draw contract exposed by `libghostty-internal`, and report
  the live embedded app's draw-thread requirement for GTK hosts.
- An optional GTK4 app shell (`cargo run --features gtk -- app --renderer gtk`)
  that renders the live workspace, pane, surface, and terminal model from the
  shared Rust core, plus sidebar activity, unread notifications, and the
  command-palette model. Terminal surfaces prefer the Ghostty VT cell grid from
  `renderer.snapshot --backend ghostty-vt` when `libghostty-vt` is available and
  fall back to the Rust ANSI/control-aware render-grid preview otherwise, with
  GTK markup for render-grid block, underline, and bar cursor previews. Browser
  surfaces embed a cached native WebKitGTK 6 view when the runtime is installed,
  synchronize in-page URL and document-title changes back into the shared
  app/socket model, and retain the bounded snapshot preview as a runtime
  fallback. Settings surfaces use native GTK navigation and controls for
  configuration, Ghostty themes, font sizes, browser availability, shortcuts,
  and `cmux.json`. Project surfaces parse `.xcodeproj` and `.xcworkspace`
  OpenStep metadata on Linux and render native Files, Targets, Build Settings,
  and Schemes tabs; their selected tab, target, configuration, scheme, file,
  and filter survive session restoration. Native file-preview, markdown, and
  diff surfaces carry persisted document metadata in renderer snapshots and
  retain compatibility HTML for existing socket automation. Agent-session
  surfaces provide native provider selection, process start/stop/interrupt,
  streamed transcripts, and a prompt composer for Codex, Claude Code, and
  OpenCode. Codex uses its structured app-server JSON-RPC transport, including
  initialization, thread/turn lifecycle, activity events, approval responses,
  assistant deltas, and the same Default, Auto-review, Full access, and Custom
  permission modes as the macOS composer. The selected Codex permission mode
  applies to later turns. Claude Code uses the macOS-compatible bidirectional
  `stream-json` transport, including partial assistant deltas, final-message
  deduplication, result fallback, and turn-completion tracking. OpenCode uses
  the macOS-compatible authenticated loopback HTTP transport: cmux launches the
  server on an ephemeral port, creates a private session, sends asynchronous
  prompts, and consumes its bounded SSE event stream. The native composer
  accepts multiple files and directories, retains pending attachments in the
  session snapshot, and sends the same escaped Markdown path links as the
  macOS agent-session composer. Its multiline editor sends with Enter or
  Ctrl+Enter, inserts a newline with Shift+Enter or Alt+Enter, and persists the
  text draft so runtime refreshes and attachment picking do not discard input.
  Failed sends keep the pending draft and successful sends consume it.
  Provider, renderer, working-directory, permission mode, text draft, pending
  attachments, and restart state survive session restoration without requiring
  WebKit.
  It includes
  workspace/surface focus clicks, workspace-group headers with
  collapse/expand and in-group creation controls plus header and plus-button
  context menus for rename, pin/unpin, config, docs, ungroup, guarded delete,
  and supported configured group actions,
  workspace row context actions for creating groups, moving workspaces into
  groups, and removing group members, plus native workspace and group-header
  drag-and-drop. Leaf reorders stay within their pinned/group partition,
  center-drops add eligible workspaces to a group, and group drags move the
  complete anchor/member block. Each pane also exposes its ordered surfaces as
  a native scrollable tab strip with focus, close, and same-pane terminal
  creation controls. Notification focus clicks, group-aware
  toolbar and shortcut actions for new workspaces, splits,
  terminals, browsers, and palette toggling, and forwards text, navigation, and
  Ctrl-letter key input to the focused PTY or command palette as appropriate. It
  accepts Linux `Super`/`Meta` shortcut aliases while preserving the
  macOS-compatible `cmd` socket/debug spelling, and renders shortcut hints with
  Linux modifier labels in GTK, palette output, and plain text shortcut-help
  rows. Runtime shortcut overrides drive the same dispatcher as GTK key input:
  remapping disables the old default, clearing a binding lets the key continue
  to the terminal, resetting restores the default, and modifier aliases/order
  are canonicalized. Linux-only directional aliases remain active only while
  their action uses its default binding. Layered `shortcuts.bindings` values
  load from `cmux.json` at startup and on config reload; the native Keyboard
  Shortcuts settings surface edits, unbinds, and resets the primary file using
  the shared macOS action IDs. Two-stroke arrays and recorder object forms use
  the macOS immediate-next-key chord contract, including bare second strokes
  routed through GTK without stealing ordinary terminal input. Command-palette
  navigation uses configurable `commandPaletteNext` and
  `commandPalettePrevious` actions only while the palette is visible, with no
  hardcoded Ctrl-key fallback after unbinding. `toggleFullScreen` synchronizes
  per-window model/session state with GTK, and `quit` checks close confirmation
  across every window before terminating the GTK application.
  `reopenPreviousSession` preserves the immutable launch-time session snapshot
  and appends cloned windows through the normal GTK window-host reconciler,
  without replacing current-launch topology. `reopenClosedBrowserPanel` keeps
  a bounded LIFO history of closed browser surfaces and restores URL, title,
  navigation history, zoom, workspace, pane/tab placement, and focus through
  the same model-to-GTK reconciliation path. Removed browser-only panes are
  recreated relative to a surviving split neighbor, while entries whose
  workspace was deleted are skipped. `globalSearch` opens a dedicated
  cross-window palette backed by live title, browser, and Markdown content;
  result activation reconciles the destination GTK window and propagates an
  escaped inline-search needle to WebKitGTK or the native Markdown view.
  The GTK shell registers `globalSearch` system-wide and can opt into the
  `showHideAllWindows` global action with `app.systemWideHotkeyEnabled`. It
  prefers the XDG GlobalShortcuts portal, falls back to passive X11 grabs, and
  rebinds both actions after a config reload without restarting the app. The
  Global Hotkey settings page exposes enablement, bindings, and backend status.
  Right-sidebar
  focus and visibility use the shared `focusRightSidebar` and
  `toggleFileExplorer` IDs; `Ctrl+1` through `Ctrl+5` switch its modes while
  sidebar focus owns the key event. Browser navigation, reload, page zoom,
  address focus, and WebKit inspector shortcuts use the shared macOS action
  IDs and dispatch to the live WebKitGTK control while browser focus owns the
  key event. Markdown zoom in/out/reset uses the shared macOS IDs while
  Markdown focus owns the key event, mutating the persisted native document
  font size in one-point steps across the 8–96 point range. The shared
  `saveFilePreview` action saves the live native document editor buffer and
  follows shortcut rebind/unbind changes. Terminal surfaces expose the beta
  TextBox as a cached native multiline composer below Ghostty, with ordered
  file attachments, shell-safe path insertion, Ctrl+Enter/send-button
  submission through the renderer-owned Ghostty input queue, two-step Escape
  hiding, and per-terminal draft/session restoration. `focusTextBoxInput`
  (`Super+Shift+A`) toggles composer/terminal focus and `attachTextBoxFile`
  (`Super+Alt+Shift+A`) opens the native multiple-file picker. The TextBox
  settings page edits `terminal.showTextBoxOnNewTerminals`,
  `terminal.focusTextBoxOnNewTerminals`, and `terminal.textBoxMaxLines`.
  The command palette also mirrors the macOS contextual workspace and tab
  management entries: clear names/descriptions, pin/unpin, mark read/unread,
  move or close neighboring workspaces, detach a tab to a new workspace, cycle
  pane tabs, and copy workspace/pane/surface IDs or `cmux://` deep links. Copy
  commands write through to the native GTK clipboard while retaining their
  exact text payload in the shared command result for socket automation.
  `editWorkspaceDescription` opens a
  multiline command-palette draft for the focused workspace, with save, clear,
  cancel, and persisted restart behavior. `newBrowserWorkspace`,
  `splitBrowserRight`, and `splitBrowserDown` create browser-only topology with
  the macOS defaults and
  route address focus to each newly embedded WebKitGTK view. Numbered surface
  and workspace bindings cover the complete
  `1…9` family from one stored action, support chord second strokes, and keep
  right-sidebar `Ctrl+1` through `Ctrl+5` priority while sidebar focus owns the
  event. The shared
  diff-viewer `J`, `K`, `Shift+G`, `G G`, and `/` actions operate on a cached
  native GTK scroller and changed-file search control, preserving scroll/search
  state across snapshot rebuilds without taking bare keys outside diff focus.
  The shared
  `closeOtherTabsInPane`, `toggleFocusedWorkspaceGroupCollapsed`, and
  `groupSelectedWorkspaces` actions operate on live pane/group topology. GTK
  sidebar rows support additive and range selection, with hidden collapsed
  children excluded from ranges. The grouping shortcut falls through to React
  Grab when fewer than two eligible workspaces are selected. `sendCtrlFToTerminal` and
  `clearScreenKeepScrollback` deliver named keys through either the core PTY
  or renderer-owned Ghostty queue. Shared Find actions drive Ghostty's live
  search state and GTK search controls for terminals, fall through to WebKit
  for browser-native find, and focus right-sidebar search for directory find.
  `Super+[` and `Super+]` traverse a
  per-window, 50-entry history of focused workspace surfaces; a new focus
  after navigating back truncates the
  forward branch, while closed surfaces resolve to the workspace's current
  surface. Socket snapshots expose both compatibility `shortcut_hint` glyphs and
  Linux `shortcut_label` text for the same rows. It also reports live terminal preview
  allocations through
  `renderer.apply_size` so PTYs track GTK widget size without feeding those
  allocations back into split geometry. GTK reconstructs nested `GtkPaned`
  trees from the renderer's pane frames, preserving horizontal, vertical, and
  nested split proportions instead of flattening them into a fixed card list.
  Native dividers are draggable; their constrained ratios persist in the shared
  workspace model and session snapshot, while programmatic widget allocation
  remains one-way. Canvas workspaces use a separate native GTK fixed-coordinate
  surface: pane positions and sizes follow the Canvas model, magnification
  scales the complete composition, and the scroll viewport centers on the
  model's saved focal point. Dragging a pane's top chrome updates its position
  without intercepting terminal/browser content, then commits the frame to the
  shared model. Six-pixel edge bands and widened corner targets resize panes on
  either axis, expose directional cursors, and enforce the same 200x120 minimum
  pane size as the macOS Canvas model. Moves and resizes snap independently on
  each axis to neighboring edges, centers, and the canonical pane gap, with
  dashed guides showing the active targets; holding Super suppresses snapping.
  New panes and broken-out tabs use collision-aware placement around the
  focused pane, while distribute and tidy operations preserve pane sizes and
  pack them at that same gap.
  Panning or dragging reveals an auto-hiding bottom-right minimap that draws
  every pane, highlights the focused pane and visible viewport, and accepts
  click-drag recentering without persisting intermediate pointer updates.
  Pane content outside a half-viewport render margin remains mounted at its
  fixed Canvas size but is explicitly occluded through the Ghostty Linux ABI;
  browser content follows the same lifecycle and resumes when it approaches
  the viewport again.
  Capture-phase pointer navigation keeps plain scroll local to terminals and
  browsers, while Super-scroll pans the Canvas anywhere and Alt-scroll or a
  trackpad pinch zooms toward the pointer without child widgets intercepting
  the gesture.
  The native General settings page and layered `cmux.json` configuration expose
  `app.newWorkspacePlacement`, `app.workspaceInheritWorkingDirectory`,
  `app.keepWorkspaceOpenWhenClosingLastSurface`, `canvas.paneGap`, and
  `canvas.snappingEnabled`. Interactive workspace creation preserves the pinned
  prefix, supports top/after-current/end placement, and optionally inherits the
  active workspace cwd. Final-surface close handling distinguishes socket,
  shortcut, tab-button, Ghostty runtime, and terminal child-exit sources so
  workspace replacement, closure, and application quit match the macOS
  lifecycle. The native Terminal settings page and `settings.terminal.*`
  methods expose `terminal.showScrollBar`, `terminal.copyOnSelect`, and
  `terminal.autoResumeAgentSessions`. Signed `terminal.resumeCommands`
  approvals support manual, GTK-prompted, and automatic restore policies;
  arbitrary original terminal startup commands are not repeated during
  relaunch. Agent Hibernation reads and writes the layered
  `terminal.agentHibernation` `cmux.json` object, protects visible terminals,
  requires idle lifecycle and a stable output/process settle window, terminates
  only scoped background agent processes, persists native placeholders across
  restart, and resumes through focus, direct input, the placeholder button, or
  `agent.hibernation.resume`. Native and agent-hook sessions retain their prior
  running intent across an idle restore while automatic agent resume is
  disabled. Ghostty
  scrollback metrics drive an interactive overlay scrollbar that stays hidden
  for alternate-screen TUIs, while copy-on-select is layered into the live
  Ghostty app configuration and updates on config reload. Settled scrollbar
  movement similarly persists the viewport center without a renderer feedback
  loop. Entering
  Canvas seeds pane frames from the current split geometry, matching the macOS
  transition. Toolbar
  controls switch between Canvas and Splits, zoom the Canvas, and fit an
  overview. Reveal scrolls only far enough to expose an offscreen pane with a
  24-point margin, while a second overview toggle restores the exact prior
  center and zoom, including across session restoration. Pane tabs cycle with
  the macOS `Cmd+Shift+]` and `Cmd+Shift+[` shortcuts and wrap within the
  focused pane without changing its Canvas position. The macOS Canvas defaults
  for layout toggle, reveal, overview, zoom in/out/reset, and tidy are available
  through Linux `Super` shortcuts and executable command-palette rows backed by
  one dispatcher. The eight alignment, equalization, and distribution actions
  that are unbound by default on macOS are also executable palette rows on
  Linux and use that dispatcher. Canvas mode, pane frames,
  zoom, and viewport center persist in the
  Linux session snapshot, while Canvas frames do not affect split-mode
  geometry. The GTK right
  sidebar honors `right-sidebar show|hide|toggle|set`, exposes native mode
  controls, and renders bounded Files/Find filesystem views, resumable Vault
  entries, feed activity, and terminal Dock targets from the shared snapshot.
  Feed and Dock follow the shared beta defaults and are omitted until
  `rightSidebar.beta.feed.enabled` or `rightSidebar.beta.dock.enabled` is
  enabled. The native Beta Features page also controls Custom Sidebars and
  gates every Remote tmux socket/CLI entry point.
  New unread notifications are also delivered through `GNotification`, with
  banner activation focusing the target workspace/surface and read or dismissed
  items withdrawn from the desktop notification service. GTK window activity is
  forwarded into the shared notification-suppression policy.
  The shell projects every model window into an independent GTK
  `ApplicationWindow` with its own Ghostty, browser, pane-allocation, and
  snapshot caches. Socket or Ghostty window creation/focus/close actions are
  synchronized with native window visibility and activation, and titlebar close
  requests honor Ghostty process-confirmation state.
  This GTK-only renderer is
  an app-shell layer; the separate `ghostty` renderer path owns terminal drawing
  through the embedded GLArea host when enabled.
- Windows, workspaces, panes, surfaces, splits, focus, and close operations.
- Linux display listing/assignment plus a config-backed default display for new
  windows via `cmux window default-display`.
- PTY-backed terminal surfaces for shell input and `surface.read_text`, with a
  validated local `xterm-ghostty` terminfo entry generated from
  `infocmp`/`tic` instead of a placeholder when the tools are available.
- ANSI/control-aware terminal text cleanup for read-screen and GTK previews,
  including carriage-return rewrites, clear-line/screen sequences, and common
  cursor movement.
- Default `renderer.snapshot` text fallback emits `cmux.render-grid.v1` style
  records and row spans for common SGR foreground/background colors, truecolor,
  256-color, bold, faint, italic, underline, blink, inverse, invisible,
  strikethrough, and overline attributes. It also models primary/alternate
  screen switches, cursor visibility, partial display erases, cursor
  shape/blink, save/restore, reset, index controls, bracketed paste,
  application cursor/keypad, focus, mouse, and wraparound modes for full-screen
  terminal applications.
- Optional runtime-loaded `libghostty-vt` formatting through
  `renderer.ghostty_vt.format` and `surface.read_text` with
  `parser=ghostty-vt`, using the shared library from
  `zig build -Demit-lib-vt=true` when present and preserving the Rust fallback
  when it is not requested.
- Ghostty VT render-state snapshots through `renderer.ghostty_vt.snapshot` and
  `renderer.snapshot --backend ghostty-vt`, currently exposing grid dimensions,
  dirty state, cursor location, and styled non-empty UTF-8 cells for visible
  terminal surfaces. This gives the GTK/GPU layer a Ghostty-backed frame
  contract before the full embedded surface renderer is available.
- Renderer-driven PTY resize through `pane.set_size` /
  `renderer.apply_size`, with pane layout dimensions reflected in terminal
  rows/columns and live PTY winsize updates.
- Layout creation from the v2 `workspace.create` layout schema.
- Versioned Linux app-session snapshots under `$XDG_STATE_HOME/cmux/` or
  `~/.local/state/cmux/`, with atomic writes after app layout/browser/session
  mutations and startup hydration for saved windows, workspace groups,
  workspaces, panes, surfaces, focus, terminal/browser reopen metadata,
  scrollback, and surface resume bindings.
- Workspace group socket/CLI support, including `cmux.json`
  `workspaceGroups.byCwd` placement, color, icon, and context-menu metadata in
  renderer-facing group snapshots.
- Surface/tab rename actions and ref-compatible CLI rename aliases.
- Sidebar metadata commands for status entries, progress, logs, cwd, and git
  branch state.
- Settings surfaces for CLI/socket clients and command-palette rows for opening
  Settings, cmux.json, and Ghostty config targets inside the Linux app shell.
  The native Account section drives hosted browser sign-in/sign-out, reports
  the active credential store, and persists team selection. Mobile reports the
  Linux host routes and creates copyable short-lived pairing links. Automation
  opens the interactive Claude Code, Codex, and OpenCode integration
  installers in app terminals. Sidebar edits title wrapping, compact mode,
  branch/path layout, descriptions, notifications, SSH targets, ports,
  progress, metadata, logs, and right-sidebar width; those controls drive the
  native GTK workspace rows. Workspace Colors edits the active-row indicator,
  selection and unread colors, and the complete named palette used by native
  workspace context-menu swatches.
- Ghostty theme listing, setting, clearing, and the macOS-compatible bare
  `cmux themes` behavior: captured/non-TTY invocations list themes, while a TTY
  opens a small interactive picker that writes the cmux-managed theme override.
- Command-palette rows for Linux auth sign-in/sign-out and mobile host connect
  status, backed by the existing socket methods.
- A Linux feedback composer surface opened through `cmux feedback` /
  `feedback.open`, rendered as a native GTK form with multi-image selection
  when the desktop shell is active. `feedback.submit` validates the production
  payload contract, copies attachments into a private XDG state queue, and
  sends a bounded multipart request to the existing `/api/feedback` service
  without blocking the GTK thread. Timeouts, network failures, rate limits, and
  server failures remain pending without losing the report; `feedback.retry` /
  `cmux feedback retry` retries pending records. Permanent client rejections
  are recorded as rejected rather than retried.
- Declarative JSON and interpreted Swift-style custom left sidebars discovered
  from `~/.config/cmux/sidebars`, with macOS-compatible `.swift`-before-`.json`
  precedence, validation/reload/select socket and CLI methods, a native GTK
  provider picker and recursive renderer, cmux method and external URL button
  actions, private persisted selection, bounded source/tree evaluation, and
  last-good rendering across broken edits. The Swift subset includes live
  workspace context, helpers, loops, conditionals, common stacks/views/styles,
  parameterized actions, simple stored-property custom `View` structs, tagged
  enums and `switch` in views/value helpers/actions, persisted top-level and
  identity-scoped per-row `@State`,
  assignment/toggle/append button mutations, native GTK
  `Toggle`/`TextField`/`Slider`/`Picker`/`Stepper` bindings, picker `.tag`
  matching, post-write `.onChange`, inherited `.onSubmit`, and GTK drag
  reordering.
  `customSidebars.renderer` selects host-process evaluation or a
  bounded, timed out-of-process worker, with the same state bag passed through
  either renderer.
- Native Markdown, text/image file-preview, and styled diff surfaces that do
  not require WebKit or browser creation; compatibility HTML remains available
  to socket automation and diff review annotations render in GTK.
  `cmux diff` supports patch files, stdin, and git `unstaged`/`staged`/`branch`
  sources, and the Linux app core exposes `diff.open` for command-palette and
  socket clients. Linux hook ingestion records and reads the macOS-compatible
  agent turn baseline store for `last-turn` diffs. Linux renders read-only
  review comments from `cmux diff --comments <json-file>`, `--comments-json`,
  and `diff.open` `comments`/`review_comments` payloads using the macOS comment
  JSON shape. Linux also persists review comments through `diff.comments.*`
  socket methods, auto-renders stored comments in repository `diff.open` views,
  and appends pending comment submission text through `workspace.prompt_submit`.
- Native agent-session surfaces created with `surface.create type=agent-session`.
  Linux exposes
  `agent_session.get_state|set_provider|set_permission_mode|start|send|interrupt|stop|output`
  plus `agent_session.attachment.add|remove|clear` and
  `agent_session.draft.set`,
  uses Codex `app-server --listen stdio://` JSON-RPC for structured Codex turns,
  exposes the four macOS Codex permission modes in the native GTK composer,
  uses Claude Code's bidirectional `stream-json` JSONL protocol with the same
  launch arguments and user-message shape as macOS, and uses OpenCode's
  generated Basic-auth loopback server, session, `prompt_async`, abort, and SSE
  APIs. GTK exposes a persisted multiline editor, separate multiple-file and
  multiple-folder pickers, and a removable pending-attachment tray. Structured
  provider output streams into the GTK transcript, and active sessions plus
  their selected permission mode, draft text, and pending attachments restore
  after a normal app relaunch.
- Browser automation socket/CLI coverage for navigation, selectors, DOM
  interaction, screenshots, storage/cookies, tab state, console/error
  inspection, browser-window creation, connect/set compatibility aliases,
  native drag/drop and WebKit file-input selection with model fallback, device
  presets, raw input aliases, Linux cookie import
  and normalized browsing-history import from Firefox and Chromium-family host
  profile stores, explicit payloads, or exported cookie files. Per-profile
  history is bounded to 5,000 entries and exposed through
  `browser history list|search|clear`. Every browser surface stores its profile
  ID in session snapshots. The native browser toolbar lists profiles, switches
  the active tab between them, and creates new profiles without requiring the
  CLI; `browser profiles select <profile> [--surface <surface>]` exposes the
  same transition for automation. Each workspace remembers its preferred
  profile for subsequent browser tabs and restores that preference with the
  app session. GTK tabs in the same profile share a persistent
  WebKitGTK network session, while different profiles use isolated website-data
  and cache directories. Clearing a profile advances its native-data generation
  and recreates affected GTK views against a fresh store. Profile metadata,
  native-data generation, and history persist atomically with mode `0600` under
  `$XDG_STATE_HOME/cmux/browser-profiles.json` or
  `~/.local/state/cmux/browser-profiles.json`; set
  `CMUX_BROWSER_PROFILES_PATH` for an isolated development store. Native WebKit
  data defaults to `$XDG_DATA_HOME/cmux/browser-profiles` and cache data to
  `$XDG_CACHE_HOME/cmux/browser-profiles`; use
  `CMUX_BROWSER_WEBKIT_DATA_DIR` and `CMUX_BROWSER_WEBKIT_CACHE_DIR` for
  isolated native smoke tests. Profile storage directory chains use mode
  `0700`, and startup removes deleted profiles plus generations made stale by a
  profile clear. Linux also
  provides a smart native omnibar that resolves bare hosts, localhost
  addresses, explicit URLs, and search queries using `browser.defaultSearchEngine`
  plus the custom search-engine settings from `cmux.json`. Its bounded popup
  uses the active profile's recent and matching history, labels matching open
  browser tabs across workspaces/windows, and switches to an existing tab when
  that row is committed. For Google, DuckDuckGo, Bing, Kagi, and Startpage it
  fetches optional query predictions on a debounced background worker and
  discards stale results; `browser.showSearchSuggestions` controls this network
  behavior. The popup supports keyboard selection without leaving the address
  field. Linux also provides native
  WebKit screenshots, PDFs, and periodic
  screencast/video recording frames with deterministic model fallback, live
  WebKit DOM/script side effects when the GTK runtime is active,
  and explicit `not_supported` responses for WebKit/CDP platform gaps.
  `system.capabilities` advertises
  `browser_model`, `browser_automation`, and `browser_profile_isolation`; the
  old `browser_stub` key remains
  present only as a deprecated `false` compatibility flag.
- Hosted Stack Auth browser sign-in using the same nested
  `/handler/native-sign-in` -> `/handler/after-sign-in` ->
  `cmux://auth-callback` contract as macOS. Linux issues and validates a
  one-time callback state, accepts the Stack access-cookie formats used by the
  web handler, and redacts credential-bearing callback URLs from socket
  responses. Normal desktop sessions persist access/refresh tokens in the
  freedesktop Secret Service through `secret-tool`, scoped to the active Stack
  project. The first successful keyring lookup migrates and removes an existing
  `auth-credentials.json` file. If the session bus or `secret-tool` is
  unavailable, automatic mode falls back to the existing atomic mode-`0600`
  file; explicit auth state/credential paths also use the file backend so
  headless automation remains deterministic. `CMUX_AUTH_CREDENTIAL_STORE`
  accepts `auto`, `secret-service`, or `file`, and `CMUX_SECRET_TOOL` overrides
  the helper path. `auth.status` reports the active `credential_store` and a
  bounded fallback reason when automatic keyring discovery fails. Desktop
  opening uses `xdg-open`, `gio`, or another installed opener by default;
  `CMUX_LINUX_AUTH_OPEN_COMMAND`, `CMUX_AUTH_WWW_ORIGIN`, and
  `CMUX_AUTH_SIGN_IN_URL` provide development overrides. The callback validates
  the Stack session against the current-user endpoint and resolves available and
  selected teams before persisting it. Restored sessions refresh stale access
  tokens and revalidate the user/team cache; permanent refresh-token rejection
  clears the local session while transient network/service failures preserve it.
  Cloud requests use the same proactive refresh path. Sign-out clears local
  auth state before attempting a bounded best-effort Stack session revocation.
  If Secret Service cannot clear immediately, cmux persists only a SHA-256
  refresh-token tombstone, refuses to restore the matching keyring entry, and
  retries deletion on later launches, so an offline keyring or auth service
  cannot silently sign the desktop back in.
- Notification APIs and renderer/sidebar notification state, including OSC
  777/99 terminal notifications and focus/read/flash behavior.
- Mobile host status and attach-ticket RPCs for Linux routes, including
  configured Tailscale endpoints and an opt-in debug loopback route, with
  neutral `device_id`/`display_name`/`host_platform` fields and Linux host-wide
  attach scopes while preserving the iOS-compatible `mac_*` payload names. A
  configured route starts the iOS-compatible four-byte big-endian
  length-prefixed TCP service. It enforces same-account Stack authentication,
  validates optional short-lived attach-token workspace/terminal scope,
  multiplexes requests over persistent connections, limits frames and active
  clients, falls back to an advertised ephemeral port when necessary, and
  publishes workspace, render-grid, notification, and agent-chat events only
  while a client subscribes. Mobile workspace mutations require explicit UUID
  targets, restrict actions to pin, unpin, rename, mark-read, and mark-unread,
  protect pinned and last workspaces from remote close, and expose the pin,
  unread, focused-terminal, latest-notification preview, and stable
  last-activity fields consumed by the iOS workspace list.
  Agent-chat subscriptions emit state, descriptor, and terminal-block frames
  using the shared `CmuxAgentChat` wire shape without treating activity
  timestamps as meaningful state changes.
- A Linux-local `cmux remotes` / `cmux remote` registry for iOS-visible remote
  hosts, with persisted names, device ids, Tailscale-only routes, tags, and
  list/add/remove CLI coverage.
- Mobile terminal replay and scroll prefetch RPCs emit `cmux.render-grid.v1`
  frames through the same ANSI/control-aware fallback renderer used by GTK
  snapshots, including styles, cursor metadata, modes, visible rows, and
  bounded scrollback spans for iOS-compatible cold attach and local scrollback
  restore.
- Authenticated Cloud VM service integration plus Linux `cmux vm` / `cmux
  cloud` CLI wrappers for list, create, destroy, exec, ssh-info, and attach
  metadata flows. Every `/api/vm` request carries the Stack access and refresh
  tokens and selected team, create requests forward a stable idempotency key,
  and SSH and WebSocket attach responses use the macOS-compatible socket shape.
  `CMUX_VM_API_BASE_URL` overrides the default `https://cmux.com` service for
  local development.
- Feed socket storage, soft-wait reply delivery, JSONL workstream audit
  persistence with restart hydration, list/clear commands, reply methods, and a
  built-in Linux terminal UI (`cmux feed tui`) for snapshot and interactive
  approval flows.
- SSH remote workspace metadata, reverse relay, reconnect, SOCKS/HTTP CONNECT
  proxying, remote command relay, remote port detection, image drop upload,
  and detachable-session socket flows.
- `cmux ssh-tmux` routes to the Linux remote tmux socket model and implements
  the macOS-compatible interactive authentication handoff when the app reports
  `auth_required` with `ssh_argv`: the CLI runs vetted `/usr/bin/ssh` in the
  caller's terminal and retries once. `cmux ssh-tmux --live` and socket
  `remote.tmux.*` calls with `live: true` probe `tmux list-sessions` over SSH,
  create a default remote session for an empty reachable server, classify
  BatchMode authentication failures into the handoff response, and then retain
  a persistent `ssh -tt ... tmux -CC` process for each mirrored session.
  Control-mode window lists create primary cmux terminal tabs. The selected
  tmux window's recursive layout is projected into native cmux split panes,
  `capture-pane` seeds each pane independently, `%output` streams into
  process-free Ghostty manual-I/O surfaces, and Ghostty or socket input returns
  to the targeted remote pane via binary-safe `send-keys -H`. Window-tab and
  pane focus route through `select-window` and `select-pane`; new windows,
  right/down splits, window renames, pane/window closes, detach, and client-size
  updates are routed back to tmux. Switching windows replaces the projected
  split tree while retaining stable primary tabs. Unexpected control-process
  exits use bounded exponential reconnect and refresh topology after reattach.

The Linux GTK shell and local libghostty renderer path are available in this
checkout. The full Ghostty GL renderer is enabled through the GTK `GLArea` host
when the sibling Ghostty checkout reports the expected Linux embedding ABI,
runtime resources, and C layout contract. GTK4 is available as an optional shell
feature and probed at runtime through diagnostics. The local Ghostty checkout
now exposes a Linux embedding ABI slice with a `GHOSTTY_PLATFORM_LINUX` tag,
host-owned OpenGL context make-current/proc-address/done-current callbacks, and
display-realized/unrealized surface hooks plus the cmux-compatible
renderer-realized boolean alias; build it with
`zig build -Dapp-runtime=none` to produce `ghostty-internal` for cmux
diagnostics. cmux verifies the required full-embedding symbols from
`ghostty-internal`, including the direct surface close/split/binding,
split-toggle-zoom, userdata, config string loading/update/inheritance,
mouse-pressure, selection cursor-cell/select/clear helpers, the Linux OpenGL inspector
init/render/shutdown hooks,
rejects Darwin-only display-id, quicklook, Metal inspector, and window blur
symbols in Linux builds,
confirms the shared
library can be dynamically loaded, and requires bundled runtime resources before
reporting the full Ghostty backend as available. Renderer diagnostics also
report the selected runtime resource directory and whether it came from
Ghostty's `ghostty_resources_dir`, a library-relative prefix,
`GHOSTTY_RESOURCES_DIR`, or a checkout-relative fallback. The loaded library's
own resource view is preferred over ambient host environment resources, and
diagnostics require the staged header's `GHOSTTY_EMBEDDING_ABI_VERSION`,
`GHOSTTY_PLATFORM_LINUX` enum value, `GHOSTTY_SURFACE_MAX_ENV_VARS` bound, and
physical-key marker constants plus the Linux redraw callback,
read-only surface env-var array, `ghostty_init` argv, IPC new-window argv, and
surface metadata `ghostty_string_s` return contracts to match the Rust FFI
constants before reporting full Linux embedding support.
The loaded `ghostty-internal` library
also self-reports its embedding ABI version, target platform, environment bound,
high-risk C struct layout sizes/alignments plus a field-offset layout
fingerprint, the Ghostty enum/constant values cmux mirrors, Linux support flag,
and app-thread draw requirement through
`ghostty_embedding_info_query`, and cmux diagnostics require
those runtime values to match the staged header and Rust FFI contract before
reporting full embedding support. cmux
also has a typed runtime loader for
initializing the library, config, app, Linux platform callback ABI, and explicit
surface visibility updates, plus an
initial GTK `GLArea` host inside terminal surface cards
for `cargo run --features gtk -- app --renderer ghostty` that forwards keyboard,
keyboard-map changes, app/surface keybinds, mouse, scroll with modifiers, GTK
IME preedit/commit, file/text drops, app and surface focus,
visibility, renderer-realized occlusion, and color-scheme state to the embedded
surface and bridges the GTK standard and primary clipboards for paste/copy
flows. Unsafe paste and OSC 52 clipboard access requests use a native modal
confirmation that shows the requested text and defaults to denial when its
originating surface is no longer live. Ghostty's start-search action opens a native GTK find bar for the
embedded surface; text changes, previous/next navigation, match highlighting,
live selected/total counts, and end-search are driven through Ghostty binding
actions, with Escape returning focus to the terminal. The Ghostty C boundary
sanitizes host-provided sizes, scales, pointer/scroll values, pressure values,
raw key/mouse/split enum values, UTF-8 text payloads, and binding-action strings
plus surface-option strings for cwd, startup command, environment, initial
input, and the host-provided environment variable count before applying those
events to core surface or inspector state. It also clears text readback outputs
before failed selection or viewport reads return, so embedders do not observe
stale Ghostty-owned text pointers after failed readback calls. The fork runs
those C-boundary tests through the embedded runtime under
`app-runtime=none` without requiring GTK modules. Its installed-library C/EGL
smoke also pumps a real child PTY through the host wakeup/redraw contract,
injects preedit and text input, reads the live viewport and title/PWD/TTY
metadata, observes process exit, and verifies balanced per-surface OpenGL
context callbacks. It also keeps embedded Ghostty
and WebKit surface widgets stable across GTK snapshot refreshes, pane-tab
switches, and workspace switches by retaining them against a window-wide
surface inventory, while closed surfaces are removed from the native caches.
GTK paints are queued only by Ghostty redraw requests; the periodic app tick
does not continuously repaint idle `GLArea` surfaces or accumulate DMA-BUF
descriptors. It guards queued GTK callbacks against stale embedded surface
userdata, pins stored Ghostty callback,
GTK action, and pane allocation targets to stable cmux UUIDs, drops stale
presentation targets and forces a fresh frame after Linux OpenGL display
re-realization rebuilds the swap chain, honors Ghostty close-confirmation state
by persisting it per embedded terminal surface in the cmux model, exposing it in
renderer/mobile terminal snapshots, and blocking
app-level quit/close-all, close-window, and close-tab requests when any affected
surface still requires confirmation, passes through the cmux terminal cwd,
startup command, wait-after-command, startup input, environment, and inherited
Ghostty font size for embedded tab/split creation, mirrors the
`GHOSTTY_SURFACE_MAX_ENV_VARS` header bound before passing GTK surface
environment overrides into libghostty,
and maps Ghostty runtime title, pwd, size-limit, initial-size,
cell-size, renderer-health, prompt-title, quit-timer, float-window,
secure-input, color-change, config-change, desktop-notification including
OSC 99 kitty notifications, bell,
open-url, progress, command-finished, search, scrollbar, command-palette,
config open/reload, readonly, copy-title-to-clipboard, child-exited, and
cursor shape/visibility actions back into the cmux surface model and GTK
surface widget. cmux-originated socket/CLI config reloads advance a renderer
snapshot generation so GTK Ghostty hosts reload the app and surface config with
the same libghostty update APIs used by Ghostty-originated reload actions.
Ghostty new-window, close-window, goto-window, new-tab,
move-tab, goto-tab, new-split, present-terminal, close-tab, goto-split,
resize-split, equalize-splits, and toggle-split-zoom actions are routed through the cmux
window/surface/pane focus model and exposed as structured terminal action
metadata. Ghostty GTK/window actions for the GTK
inspector, render requests, quit/close-all-windows with structured app-action
metadata, Ghostty fullscreen compatibility requests as native fullscreen on Linux, maximize,
visibility/quick-terminal toggles, reset-window-size, window decorations,
tab overview as the Linux command switcher, and on-screen keyboard focus requests
are applied to the host GTK window, Ghostty key-sequence/key-table actions are
tracked as structured terminal state for renderer/debug consumers, while
UI-only Ghostty actions such as background opacity, undo/redo, and
update checks are recorded as terminal metadata. The GTK Ghostty host now also creates the local Ghostty Linux OpenGL
inspector as an overlay in the embedded `GLArea`, renders it from Ghostty
`render_inspector` callbacks, and routes focus, mouse, scroll, text, and common
navigation/editing keys into the inspector. Full `ghostty` renderer snapshots skip the Rust text-grid
fallback path so GTK refreshes do not start a duplicate core PTY for surfaces
owned by embedded Ghostty, and `cmux app --renderer ghostty` defers automatic
core PTY startup so the embedded Ghostty surface owns the initial terminal
process. App-shell and socket text/key input for renderer-owned terminals is
queued in the cmux surface model and drained by the GTK Ghostty host into the
embedded surface through Ghostty's text and key input APIs instead of waking the
fallback PTY, and the GTK Ghostty host
periodically syncs embedded viewport text, syncs selection state/readback on
Ghostty selection-change events, and syncs TTY name, process-exit state, plus
the foreground process PID, mouse-capture state, and live grid/cell pixel size back into the cmux surface model for
read/debug/process APIs. The portable Ghostty VT core remains available as
`ghostty-vt` for parser/render-state parity and headless diagnostics alongside
the full GL renderer path.
