# Linux Architecture

This document maps the Linux implementation as it exists today. It describes
ownership boundaries, not an idealized rewrite. The [roadmap](ROADMAP.md)
defines which behavior is release-blocking.

## Runtime shape

Stateful daily-driver entry points converge on one in-process application
state:

```text
External CLI → socket client → server ──┐
app --script → UI command parser ───────┼→ AppState → renderer snapshot
GTK actions → GTK shell ────────────────┘       │             │
                                                │             ├→ GTK widgets / WebKitGTK
                                                └→ terminal I/O└→ Ghostty GTK host
```

`AppState` is the authoritative model for windows, workspaces, panes, surfaces,
settings, persistence, and most stateful commands. The display-free server and
the GTK application use the same command and state paths. GTK renders snapshots
of that state and sends mutations back through shared actions instead of
owning a second application model.

Not every CLI path enters `AppState`. Help, version, selected update and tmux
utilities, and other no-socket commands return from `cli.rs` directly. Some
socket handlers also coordinate bounded browser, feedback, or host side effects
around the model mutation. Trace those adapters before assuming a command is a
pure `AppState` call.

The native process currently uses `Arc<Mutex<AppState>>` where state crosses
the socket-server and GTK boundaries. Code holding that lock must not perform
long-running process, network, or renderer work.

## Composition root and entry points

`src/main.rs` currently declares the module graph, installs broken-pipe panic
handling, dispatches the private sidebar interpreter worker, and then hands
normal arguments to `cli::run`.

The Cargo package produces two executables:

- `cmux`: user CLI, display-free daemon, and GTK application
- `cmuxd-remote`: remote daemon used by SSH and remote-host flows

Phase 5 moves the module graph into `src/lib.rs` so `main.rs` contains only
process-level policy and dispatch.

## Module ownership

The source tree is still mostly flat. Use these responsibility groups when
finding code or deciding where new behavior belongs.

| Area | Current modules | Owns |
| --- | --- | --- |
| Application model | `app.rs`, `config.rs`, `project.rs` | Authoritative topology, actions, persistence, configuration, and project metadata. |
| Protocol and entry points | `cli.rs`, `server.rs`, `src/bin/cmuxd-remote.rs` | CLI parsing and output, JSON-lines socket transport, daemon startup, and remote process entry. |
| Renderer contract | `renderer.rs`, `ui.rs` | Display-independent snapshots, diagnostics, render models, and UI-facing actions. |
| Terminal core | `terminal.rs`, `terminal_copy_mode.rs`, `ghostty_vt.rs` | PTY state, key encoding, copy mode, fallback terminal parsing, and the optional Ghostty VT interface. |
| Ghostty GTK integration | `ghostty_embed.rs`, `gtk_ghostty.rs` | Dynamic FFI loading and validation, Ghostty application/surface lifetime, GL hosting, input, clipboard, and callbacks. |
| Native GTK shell | `gtk_ui.rs`, `gtk_ui/`, `gtk_webkit.rs`, `global_shortcuts.rs` | Windows, workspaces, panes, native controls, WebKit surfaces, and desktop shortcuts. |
| Browser model | `browser_runtime.rs`, `browser_settings.rs`, `browser_environment.rs`, `browser_omnibar.rs` | Browser state, automation model, profiles, environment emulation, and suggestions. |
| Agent and remote flows | `agent_session.rs`, `agent_hibernation_settings.rs`, `resume_approval.rs`, `remote_tmux.rs`, `mobile_host.rs` | Provider sessions, recovery policy, remote tmux, and mobile-host protocol. |
| Extension surfaces | `custom_sidebar.rs`, `swift_sidebar.rs`, `sidebar_extension.rs` | Declarative sidebars, bounded Swift interpretation, state, and isolated extension execution. |
| Supporting features | `diff_viewer.rs`, `diff_baseline.rs`, `file_url.rs`, `linux_update.rs`, `shortcut_when.rs` | Focused feature models and utilities. |

Most tests live beside their module. `tests/socket_contract.rs` exercises the
compiled `cmux` binary across its public socket and CLI behavior.

## State and mutation flow

There are three normal mutation paths:

1. A normal external CLI command is converted into a request by `cli.rs` and
   sent over the Unix socket. `server.rs` locks the shared state, applies the
   command, and serializes the response.
2. `cmux app --script` and the display-free application REPL are parsed by
   `ui.rs` and apply the same model and renderer operations in-process.
3. GTK translates input or widget actions into shared model operations, then
   reconciles native widgets from a new renderer snapshot.

When a behavior is available from more than one entry point, its mutation
belongs in the shared application path. The CLI, socket, palette, and GTK
controls should be adapters, not separate implementations.

Persistence follows the XDG directories. Runtime state normally lives under
`$XDG_STATE_HOME/cmux` or `~/.local/state/cmux`; configuration normally lives
under `$XDG_CONFIG_HOME/cmux` or `~/.config/cmux`. Tests should override these
roots rather than reading a developer's real profile.

## Ghostty integration

The parent repository pins `ghostty/` as a Git submodule. Linux embedding
depends on fork changes documented in
[`docs/ghostty-fork.md`](../docs/ghostty-fork.md); an arbitrary upstream
Ghostty checkout is not interchangeable with the pinned commit.

The integration has three layers:

1. `ghostty_embed.rs` declares the C ABI, loads `libghostty-internal.so`, checks
   required and unexpected symbols, validates layout and constant fingerprints,
   locates runtime resources, and wraps FFI lifetimes.
2. `gtk_ghostty.rs` owns the GTK GL host and maps cmux surfaces, input,
   clipboard, renderer wakeups, and Ghostty actions to the application model.
3. `ghostty_vt.rs` exposes a display-free terminal parsing path used by tests
   and fallback renderer diagnostics. It is not the native GTK daily-driver
   renderer.

`linux/scripts/run-dev.sh` is the normal integration entry point. It verifies
the submodule SHA, provisions the exact Zig version required by the submodule,
builds Ghostty with `ReleaseSafe`, builds cmux with GTK, and exports
`CMUX_GHOSTTY_LIBRARY` and `CMUX_GHOSTTY_ROOT` before launch.

Do not update the submodule pointer as part of an unrelated cmux change. A real
Ghostty change must be committed and pushed to the fork before the parent
pointer is committed.

## Build boundaries

The default Cargo feature set is display-free. It builds the model, CLI,
socket server, protocol, and tests without linking GTK. The `gtk` feature adds
GTK 4, WebKitGTK build checks, the native shell, the GL Ghostty host, and portal
support.

This split is intentional:

- model and socket contracts should stay runnable in headless CI
- native GTK behavior must still receive separate feature-enabled and live
  display validation
- renderer-specific code should not leak GTK types into the application model

## Structural debt and direction

The largest files are currently `app.rs`, `tests/socket_contract.rs`,
`gtk_ui.rs`, `cli.rs`, and `gtk_ghostty.rs`. Their size makes navigation and
ownership difficult, but file size alone is not a reason to invent abstractions.

The safe reduction strategy is incremental:

1. establish a library composition root and responsibility-based directories
2. move code mechanically with no behavior change
3. extract one coherent behavior at a time behind existing contract tests
4. introduce a new crate only when an actual dependency boundary needs it

Avoid a simultaneous rewrite of the model, GTK shell, and Ghostty host. Those
layers meet on lifetime, threading, and reconciliation contracts that are much
easier to verify one boundary at a time.

## Where a change belongs

- Put topology and persistent application behavior in the application model.
- Put serialization and transport concerns in the server or CLI adapter.
- Put display-independent view data in the renderer contract.
- Put GTK widget lifetime and event handling in the GTK shell.
- Put Ghostty ABI and ownership rules in the Ghostty integration layer.
- Add behavior coverage at the shared action boundary, then add a native smoke
  when GTK, clipboard, input method, GL, or compositor behavior is involved.
