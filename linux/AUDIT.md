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
screenshots. Follow-up passes add private software Weston, LXDE, actual
IBus/Anthy, virtual RandR outputs, and Radeon hardware-rendered Weston checks.

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

## Follow-up corrections

The second audit added behavioral regressions for these differences:

| Area | Verified difference | Correction |
| --- | --- | --- |
| Sidebars | Fixed widths prevented adapting the layout to paths and file lists. | Drag handles on both edges; per-window widths survive restart, hide/show, and temporary compact layouts. The terminal retains usable space. |
| Closed history | Only browser panels could reopen. | Terminal panels, workspaces, and windows restore layout, order, and cwd with fresh shells. History survives restart and supports choosing an older item. Saved commands are not rerun. |
| Persistence | A topology change serialized and synced a full session on the GTK thread. | Immutable snapshots enter a serial worker with one coalesced pending write. Explicit save/quit and socket mutation replies retain their durable boundary. Errors remain visible. |
| Palette editing | Query and rename text were labels with append/backspace handling. | Persistent native editors provide caret movement, selection, clipboard, undo, and input-method composition. Command results reach native clipboard, browser, and document handlers. |
| Browser address bar | Refresh replaced an in-progress address and selection because focus belonged to the entry's child. | Descendant focus checks preserve drafts and native selection; suggestions respect editor focus. |
| Browser navigation | Forward was only accessible through a menu. | Forward appears beside Back in the toolbar. |
| Settings | Long pages overflowed; no cross-section search; cmux.json displayed the Ghostty configuration; Reset was a placeholder. | Scrollable pages, native search over real settings rows, opening the correct JSON file, and confirmed reset of managed settings while preserving unrelated configuration. |
| New terminals | Switching panes then creating a tab/split could inherit another pane's cwd. | Shared creation resolves cwd and font from the source terminal, with explicit cwd taking priority. |
| Terminal EOF | Ctrl+D immediately deleted a split instead of reaching its running program. | Route EOF through normal terminal input. A real `cat` contract checks that its shell and pane survive. |
| Open Directory | The action created a workspace without selecting a directory. | A native folder chooser targets the owning window; cancellation leaves workspaces unchanged. |
| Installed resources | The bundle omitted Ghostty's sibling terminfo data while ABI-only validation passed. | Package terminfo aliases and locale data, and require complete runtime resources after relocation. |

Review also identified failed-history restoration leaving partial objects,
consecutive split restores losing placement anchors, and palette results losing
native side effects. These have focused regression coverage.

The background-save profile uses 24 terminal snapshots and about 11.7 MB of
encoded JSON. In a debug build, capture took 4.1 ms and serialization took
304.7 ms; serialization now runs on the worker. This measures snapshot work,
not end-to-end keyboard or rendering latency.

## Broader comparison

| Capability | Assessment |
| --- | --- |
| Workspace, pane, and tab creation, focus, rename, reorder, split, close | Implemented through the shared Linux model and covered by socket/model contracts. Changes above address confirmed UI gaps instead of replacing the topology model. |
| Session restart | A native Ghostty launch, Unicode input, split, quit, and reopen preserved workspace names and pane topology. Reopening starts new shells; it is not continuation of an existing process. |
| Terminal rendering | The port embeds the pinned Ghostty fork, with ABI/resource diagnostics and renderer-driven wakeups. No per-keystroke drawing loop was added. |
| Keyboard shortcuts | Linux uses terminal-safe platform bindings rather than mechanically replacing macOS Command with Control. Existing shortcut settings and context guards remain authoritative. |
| Clipboard, selection, scrollback, find, IME | Existing implementations and contracts remain. Native search/focus, pointer selection, and cross-process X11 clipboard copy/paste are exercised. Real IBus/Anthy composition is exercised under LXDE; see the desktop validation below. |
| Browser panes | WebKit panes, navigation controls, and a rendered page are present in the native browser fixture. This does not establish browser-extension/import or website compatibility parity. |
| Settings/configuration | Existing settings surface and parsers remain; Settings is discoverable through the menu, scrollable, searchable, and can open/reset its configuration. New visible strings are in both Linux catalogs. |
| Notifications | Workspace badges and pane attention states appear in the attention fixture. Desktop notification delivery depends on the desktop service and is not established by a screenshot. |
| Dense/narrow/high-DPI layout | Existing compact tabs and flat dark chrome are retained. Dense, 900 px narrow, and 2x scale fixtures exercise the layout. Narrow right-sidebar behavior remains an overlay. |
| Build/install | Locked Rust builds and the pinned Ghostty library are used. The development bundle and clean-prefix launcher checks pass; a fresh release build remains a separate gate. |
| Accessibility | Newly added controls have accessible names and keyboard focus. A complete screen-reader traversal remains unverified. |
| macOS-specific features | AppKit window chrome, Apple system integration, and macOS global-key behavior are platform differences; copying them is not a Linux daily-use requirement. |

## Remaining differences and limits

The follow-up removes the three implementation gaps recorded in the first pass:
resizable persisted sidebars, terminal/workspace/window closed history, and
background UI autosaves. The remaining limits concern platform validation and
features outside the checked daily-use workflows:

- Native GNOME Wayland, hardware input-to-photon latency, physical monitor
  hotplug, mixed-DPI scaling, and screen-reader use still require checks.
  The tested GTK/IBus stack also loses the first IME character when replacing
  selected text on X11; the independent control and workaround are below.
  LXDE/IBus, virtual RandR outputs, and a hardware-rendered headless Weston
  session now have separate evidence below. Cross-process X11 clipboard copy
  and paste pass, but primary-desktop ownership still needs checking. The
  [roadmap](ROADMAP.md) retains the outstanding release gates.
- Remote/cloud/mobile flows, browser migration, and custom sidebar extensions
  were checked only for their interaction with the touched core paths. Their
  complete feature parity is outside the existing daily-driver contract.

## Initial validation

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
- The initial 34 MB bundle passed ABI, installation, and native interaction
  checks. Follow-up inspection found that its validation missed absent
  terminfo resources; the corrected bundle and stronger gate are described below.
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

## Follow-up validation

- Full display-free suite: **877 passed** (542 library, one daemon, 334 socket
  contracts). Four existing subprocess helpers and one manual profiling test
  are ignored.
- Full GTK suite: **1,092 passed** (759 library, one daemon, 332 socket
  contracts), with the same five explicit ignores. Native folder-dialog
  tests exercise deferred presentation, cancellation, and window cleanup.
- Native X11 events verify pointer sidebar resizing, palette select-all and
  text replacement, palette clipboard output, terminal pointer selection and
  clipboard copy to another process, paste from another process, Ctrl+D
  inside a program, input focus after metadata refresh, and continuous resize.
- A private software Weston compositor runs the installed Ghostty application
  through Unicode output, split I/O, session restart, and clean quit. This is
  Wayland protocol/launch evidence, not hardware or GNOME desktop certification.
- The package regression failed on the old bundle. A rebuilt and relocated
  package reports complete Ghostty resources; `infocmp` resolves both
  `xterm-ghostty` and its `ghostty` alias from the installed terminfo directory.
- Six reviewed screenshot fixtures cover dense panes, attention, browser,
  settings, narrow palette/sidebar, and 2x scaling. The mounted compact-drawer
  regression verifies visible file content after opening a previously hidden sidebar.
- The English and Japanese catalogs each contain 66 nonempty keys.

Follow-up logs, native probes, and screenshots are stored beside the original
evidence under `../../cmux-daily-audit-2026-09-20/`.

## LXDE, IME, and hardware validation

The next pass runs private LXDE sessions with Openbox, lxpanel, and pcmanfm.
Desktop packages and Anthy were extracted into temporary directories; system
packages, desktop configuration, and device permissions were unchanged.

- Native XTest keyboard input reaches the terminal after minimize/restore,
  maximize, fullscreen exit, virtual desktop moves/switches, and palette close.
  Each stage checks executed shell output, with screenshots retained.
- Stock Openbox reserves Ctrl+Alt+arrow for virtual desktop navigation. The
  same key therefore cannot reach cmux pane focus. Changing Focus Left to
  Ctrl+Shift+Left through the existing shortcut settings works with actual
  keyboard events and persists the override.
- Two independently configured Xorg dummy/RandR outputs exercise window moves,
  maximize on the second output, primary-output changes, removal/reconnection,
  vertical arrangement, and mode changes. Terminal input and rendering remain
  live. These output tests use llvmpipe. After removing a lower output, Openbox
  can leave a normal window partly off-screen with its titlebar reachable;
  an independent stock GTK window reproduces the same placement. A maximized
  cmux window relocates and resizes to the remaining work area.
- A non-root disposable container with only the Radeon render node exposed
  runs packaged cmux under a hardware-rendered headless Weston compositor.
  Mesa identifies the AMD Radeon 780M; cmux opens the render node and its
  amdgpu fdinfo records graphics and compute work. Terminal command output,
  split creation, frame presentation, and clean quit pass at output scales
  1 and 2. This verifies hardware rendering, not physical display scanout,
  input-to-photon latency, or transitions between monitors with different DPI.

Actual IBus/Anthy testing found three additional defects: terminal Find and
browser Find consumed Enter before the IME committed converted text, and the
address bar could navigate an old suggestion during composition. All three
capture handlers now yield to GTK while the native editor has preedit text.
Three GTK behavioral tests failed before the fix and pass afterward. Normal
Find and address navigation resume when composition ends. The complete GTK
suite passes **1,095 tests** (762 library, one daemon, 332 socket contracts),
with five existing explicit ignores. Formatting and the GTK build pass.

Desktop evidence is under `../../cmux-daily-audit-2026-09-20/desktop-ime/`,
`desktop-gpu/`, and `desktop-lxde-keyboard/`. Hardware rendering and real IME
coverage reduce the earlier validation gap; the remaining limits above still
apply.

The real-engine rerun uses XTest romaji input, Anthy candidate conversion,
and IBus protocol logs. It verifies `nihongo` becoming `日本語` in the
terminal, palette, terminal Find, browser Find, and address editor, including
an address with an existing suggestion. The initial address is explicitly
cleared before composition because of the GTK limitation below. Terminal Find
reports two matches.
Palette editing survives metadata refresh during composition; its first
Escape cancels composition without closing the palette. Candidate selection
uses the real IBus popup, rather than injected Unicode or synthetic preedit.

Replacing selected text has a separate limitation in the installed GTK/IBus
stack on X11: the first preedit character can be lost. A standalone GTK Entry,
selected after mapping with End then Ctrl+A, reproduces the same `nihongo` →
`意本語` result. Native reset stacks show GTK's PRIMARY clipboard handling
resetting the IME during selection deletion, without a cmux callback. Clearing
the selected text with Backspace before starting composition avoids this path.
The audit records this as an unresolved toolkit limitation, not a fixed cmux
bug. An experimental suggestion-refresh change did not correct it and was
excluded from the final source.
