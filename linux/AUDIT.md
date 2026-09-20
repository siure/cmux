# Linux daily-use audit

Audit date: 2026-09-20. Baseline: `e77e0e2bc` in `cmux-linux-clean`, including
Linux integration commit `169d65bf2` and the ownership cleanup. The original
implementation is the Swift source and reference screenshots in this same
checkout. This is a comparison with that checked-in version, not a claim of
parity with a newer upstream release.

The audit used five parallel source reviews, failing behavioral tests, the
existing socket contracts, six native GTK screenshot scenarios, and a real
embedded Ghostty session under Xvfb. macOS cannot run on this Linux host;
original behavior was verified against its implementation and checked-in
screenshots. Hardware Wayland behavior requires a separate desktop check.

## Verified problems and changes

| Area | Original behavior | Verified Linux problem | Change |
| --- | --- | --- | --- |
| Search and editor input | Foreign first responders retain keyboard input (`Sources/AppDelegate.swift`, focus-routing guard). | Window capture routed letters to the selected terminal before checking whether an entry or WebKit owned focus. | Let native editors and WebKit handle their input before terminal focus repair. A GTK signal test reproduced the failure. |
| Focus during updates | Model selection and native first responder are distinct. | Refreshing an unchanged selected Ghostty surface called `grab_focus`, stealing focus from search/sidebar fields. | Preserve foreign focus on metadata updates and retry callbacks; actual terminal selection changes still focus the terminal. |
| Live resize | Ghostty geometry updates during interactive resize (`Sources/GhosttyTerminalView.swift`). | Every resize invalidated a 50 ms delayed update. Continuous drags could postpone every update. | Coalesce resize work into one GTK idle callback using the latest allocation. |
| Session writes | Autosave checks fingerprints and writes on a persistence queue (`Sources/AppDelegate.swift`). | Successful `workspace.list`, screen reads, and terminal input could synchronously serialize and write the full session. | Save only after a presented-model mutation; preserve saves for topology-changing shortcuts, palette commands, confirmations, and history actions. |
| Workspace close | Workspace close paths protect running processes and pinned workspaces (`Sources/TabManager.swift`). | Sidebar X and keyboard close directly removed workspaces. Closing the last workspace by keyboard returned an error. | Shared workspace confirmation with cancellation, explicit confirmation, and last-workspace window-close handling. Explicit API close remains noninteractive. |
| Bulk workspace close | Workspace close policy also applies to multi-workspace actions. | Close-others/above/below bypassed confirmation. | Confirm the captured target set; retain pinned and subsequently created workspaces. |
| Command palette | Search results are selectable UI controls. | Linux rendered results as boxes, so opening by mouse did not allow executing by mouse. | Accessible buttons activate the current command by identity through the shared model path, scoped to the owning window; stale results are rejected. Click-away dismisses the palette. |
| Workspace scrolling | Selection is revealed without resetting the list (`Sources/ContentView.swift`, `SidebarWorkspaceTableController.swift`). | Sidebar refresh replaced its scroller; metadata changes lost the user's position. | Retain the viewport and restore its offset; reveal the selected workspace after selection/topology changes. |
| Tab scrolling | The selected tab remains reachable in the tab strip. | Selecting an overflow tab did not reveal it. | Reveal after selection/topology changes and preserve manual scrolling during title-only updates. |
| Header/menu access | Sidebar toggle and Settings are visible app controls. | A hidden left sidebar had no header toggle; Settings was missing from overflow. | Add both through existing model actions, with accessible names and English/Japanese strings. |
| Right sidebar | Fresh macOS state has the file sidebar hidden (`Sources/FileExplorerState.swift`). | Fresh Linux state opened it, and child expansion let it consume surplus terminal width. A GTK test measured 734 px in an 1180 px container. | Default to hidden while preserving saved visibility; constrain expansion when opened. |
| Dark appearance | App chrome and native controls follow a coherent dark appearance. | Application CSS was dark, but native controls could inherit a light desktop theme. The screenshot harness forced a dark theme and hid this discrepancy. | Request GTK's dark appearance before installing app styles. |
| Native validation | A passing test must execute its assertions. | Widget tests initialized global GTK from different test threads and silently returned when initialization failed. | Use the existing GTK test macro to own the GTK thread; supply Xvfb and D-Bus in CI. |
| Fixture isolation | Tests should exercise known input and private state. | An empty home triggered zsh onboarding; inherited browser paths could point outside the fixture; D-Bus services lacked the Xvfb display. | Deterministic Bash fixtures, isolated XDG/browser paths, Xvfb before D-Bus, and retained failure diagnostics. |

## Broader comparison

| Capability | Assessment |
| --- | --- |
| Workspace, pane, and tab creation, focus, rename, reorder, split, close | Implemented through the shared Linux model and covered by socket/model contracts. Changes above address confirmed UI gaps instead of replacing the topology model. |
| Session restart | A native Ghostty launch, Unicode input, split, quit, and reopen preserved workspace names and pane topology. Reopening starts new shells; it is not continuation of an existing process. |
| Terminal rendering | The port embeds the pinned Ghostty fork, with ABI/resource diagnostics and renderer-driven wakeups. No per-keystroke drawing loop was added. |
| Keyboard shortcuts | Linux uses terminal-safe platform bindings rather than mechanically replacing macOS Command with Control. Existing shortcut settings and context guards remain authoritative. |
| Clipboard, selection, scrollback, find, IME | Existing implementations and contracts remain. Native search/focus and X11 system-clipboard paste are exercised; selection/copy and IME composition still need real-desktop validation. |
| Browser panes | WebKit panes, navigation controls, and a rendered page are present in the native browser fixture. This does not establish browser-extension/import or website compatibility parity. |
| Settings/configuration | Existing settings surface and parsers remain; Settings becomes discoverable through the menu. New visible strings are in both Linux catalogs. |
| Notifications | Workspace badges and pane attention states appear in the attention fixture. Desktop notification delivery depends on the desktop service and is not established by a screenshot. |
| Dense/narrow/high-DPI layout | Existing compact tabs and flat dark chrome are retained. Dense, 900 px narrow, and 2x scale fixtures exercise the layout. Narrow right-sidebar behavior remains an overlay. |
| Build/install | Locked Rust builds and the pinned Ghostty library are used. The development bundle and clean-prefix launcher checks pass; a fresh release build remains a separate gate. |
| Accessibility | Newly added controls have accessible names and keyboard focus. A complete screen-reader traversal remains unverified. |
| macOS-specific features | AppKit window chrome, Apple system integration, and macOS global-key behavior are platform differences; copying them is not a Linux daily-use requirement. |

## Remaining differences and limits

- The workspace sidebar has fixed normal/compact widths. The original supports
  drag resizing and persistence. This is a usability difference, but it does
  not block terminal work; a future change should add one persisted model
  setting rather than a GTK-only width that resets on reload.
- Reopen-closed currently restores browser entries; original history also
  handles terminal/workspace closures. Session restart is a separate feature.
  Restoring a shell must not silently rerun arbitrary former commands.
- Durable topology mutations still serialize sessions synchronously. The
  confirmed read/input hot path is fixed; background persistence would require
  ordered writes and shutdown/error handling and should follow profiling.
- Native GNOME Wayland, hardware GPU latency, IME, multi-monitor scaling,
  screen-reader use, and cross-application clipboard ownership cannot be
  certified by Xvfb. The audit does not call this a release-ready daily driver
  until the [roadmap](ROADMAP.md) gates are satisfied.
- Remote/cloud/mobile flows, browser migration, and custom sidebar extensions
  were checked only for their interaction with the touched core paths. Their
  complete feature parity is outside the existing daily-driver contract.

## Validation

Baseline display-free suite: **843 passed, four existing ignored tests**.
Baseline native Ghostty run passed Unicode output (`λ-é-猫`), split I/O,
workspace/pane topology reopen, and clean quit. Renderer snapshots reported
live surfaces. Six baseline GTK screenshot scenarios completed.

Behavioral regressions were committed before their fixes and observed failing
for input routing, focus preservation, scroll continuity, sidebar width,
workspace close, bulk close, session writes, and palette access. Final checks:

- Display-free suite: **852 passed**, four existing ignored subprocess helpers.
- GTK suite under isolated Xvfb/D-Bus: **1,050 passed** (717 library, one daemon,
  332 integration), four existing ignored subprocess helpers. No test failures.
- Locked GTK build, Rust formatting, diff checks, CI-lane guard, and shell
  syntax checks pass.
- New search-input, focus, scrolling, palette, close-protection, and persistence
  regressions fail on the baseline and pass after their fixes. Independent
  review caught and corrected transient palette shortcuts entering persistence;
  that path has its own regression test.
- Real X11 keyboard events type `needle` into find and append `x` after a
  metadata refresh. The baseline left the search query empty. The fixed app
  retains `needlex` in the field without sending it to the shell.
- Native Ghostty smoke passes Unicode output, split I/O, continuous resize,
  paste from a separate GTK clipboard owner, topology reopen, and clean quit.
  Model size samples change while resize events continue. This is not a
  measurement of hardware input-to-photon latency.
- All six screenshot comparisons pass against the reviewed updated goldens.
- The 34 MB development bundle validates Ghostty ABI/resources, installs into
  a fresh temporary prefix, and its relocated launcher passes Unicode, split,
  session reopen, and quit checks without source-tree Ghostty overrides.
- Eight new UI strings have nonempty English and Japanese translations; both
  catalogs parse and contain the same 47 keys. Existing untranslated surfaces
  outside this change were not claimed as localized.

The six updated, reviewed references are in
[the X11 goldens](tests/visual/goldens/x11). Examples:
[dense panes](tests/visual/goldens/x11/gtk-next-dense-1180x760.png),
[narrow palette](tests/visual/goldens/x11/gtk-next-narrow-900x700.png).

The native probe scripts, raw diagnostics, screenshots, suite logs, and bundle
checksum are retained in `../../cmux-daily-audit-2026-09-20/` relative to this
file. The bundle is a development build, not a published release.
