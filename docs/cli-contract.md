# cmux CLI Contract

This document is the compatibility contract for migrating `CLI/cmux.swift` to
Swift ArgumentParser. The migration should preserve command names, aliases,
global flags, exit behavior, socket routing, and no-socket help behavior unless
a PR explicitly calls out an intentional contract change.

The current implementation is a hand-rolled parser. This spec is deliberately
written around user-visible behavior so the implementation can change behind it.

## Migration Rules

- Keep `cmux --help`, `cmux -h`, `cmux --version`, and `cmux -v` working without
  connecting to the cmux socket.
- Keep documented `cmux <command> --help` probes working without a socket where
  they already do.
- Keep `--socket`, `--password`, and `--window` as global options before the
  command. Keep presentation options `--json` and `--id-format` accepted either
  before or after the command.
- Keep UUIDs, refs such as `workspace:2`, and indexes accepted wherever the
  command accepts a window, workspace, pane, surface, or tab handle.
- Keep text output stable for scripting commands unless a command already
  documents JSON as the scripting interface.
- Keep hidden/internal commands available until their callers have migrated.

## Global Invocation

| Form | Contract |
| --- | --- |
| `cmux <path>` | Open a directory or file parent in cmux through the app's file-open path, without requiring control-socket access. Relative paths resolve from the current working directory. |
| `cmux [global-options] <command> [options]` | Run a named command. Presentation options may appear before or after the command. |
| `cmux --help`, `cmux -h` | Print top-level usage without a socket. |
| `cmux help` | Print top-level usage without a socket. |
| `cmux --version`, `cmux -v`, `cmux version` | Print version summary without a socket. |

Global options:

| Option | Contract |
| --- | --- |
| `--socket <path>` | Override the socket path for this invocation. |
| `--password <value>` | Use an explicit socket password. Takes precedence over `CMUX_SOCKET_PASSWORD`. |
| `--json` | Prefer machine-readable JSON output for commands that support it. |
| `--id-format <refs\|uuids\|both>` | Select handle format in JSON and supported text output. |
| `--window <id\|ref\|index>` | Route the command through a specific window when supported. |

Environment:

| Variable | Contract |
| --- | --- |
| `CMUX_SOCKET_PATH` | Canonical socket path override. |
| `CMUX_SOCKET` | Deprecated compatibility alias for `CMUX_SOCKET_PATH`. New scripts should use `CMUX_SOCKET_PATH`; if both variables are set and differ, the CLI fails before socket commands. |
| `CMUX_SOCKET_PASSWORD` | Socket password fallback when `--password` is absent. |
| `CMUX_BROWSER_PROFILES_PATH` | Override the Linux browser profile/history store. Defaults to `$XDG_STATE_HOME/cmux/browser-profiles.json` or `~/.local/state/cmux/browser-profiles.json`. |
| `CMUX_BROWSER_WEBKIT_DATA_DIR` | Override the root for persistent profile-scoped WebKitGTK website data. Defaults to `$XDG_DATA_HOME/cmux/browser-profiles` or `~/.local/share/cmux/browser-profiles`. |
| `CMUX_BROWSER_WEBKIT_CACHE_DIR` | Override the root for profile-scoped WebKitGTK caches. Defaults to `$XDG_CACHE_HOME/cmux/browser-profiles` or `~/.cache/cmux/browser-profiles`. |
| `CMUX_MOBILE_HOST_ATTACH_ROUTE` | Advertise and listen for the Linux mobile host at `<tailscale-host>:<port>`. The host binds all interfaces by default, requires same-account Stack authentication for every data-plane request, and publishes the configured route in attach tickets. |
| `CMUX_MOBILE_HOST_DEBUG_LOOPBACK` | Opt into an iOS Simulator/development route on `127.0.0.1`; `CMUX_MOBILE_HOST_DEBUG_ROUTE_PORT` overrides its port. |
| `CMUX_MOBILE_HOST_BIND_HOST` | Override the mobile listener bind address. Tailscale routes default to `0.0.0.0`; loopback-only development defaults to `127.0.0.1`. |
| `CMUX_MOBILE_HOST_ENABLED` | Set to `0`, `false`, `no`, or `off` to disable the Linux mobile listener even when routes are configured. |
| `CMUX_WORKSPACE_ID` | Default workspace context inside cmux terminals. |
| `CMUX_PANE_ID` | Default pane context inside cmux terminals. |
| `CMUX_SURFACE_ID` | Default surface context inside cmux terminals. |
| `CMUX_TAB_ID` | Default tab context for tab commands. |
| `CMUX_PANEL_ID` | Legacy panel context; maps to the current surface in Linux local and remote terminal shells. |
| `CMUX_LINUX_BUNDLE_VERSION` | Override the installed Linux bundle version used by `cmux version` and update comparison. Relocatable and installed launchers set this from `share/cmux/bundle-version`. |
| `CMUX_LINUX_UPDATE_API_URL` | Override the latest-release JSON endpoint used by Linux update checks. Defaults to the official `manaflow-ai/cmux` GitHub latest-release API. |

## Top-Level Commands

| Command | Contract |
| --- | --- |
| `welcome` | Print the welcome screen. |
| `docs` | Print canonical docs URLs, raw GitHub resources, and useful commands for a topic. |
| `settings` | Open Settings, print cmux.json paths, or print settings docs. Linux exposes `settings.app.status` / `settings.app.set` for workspace lifecycle; `settings.browser.status` / `settings.browser.set` for omnibar search-engine configuration; `settings.beta_features.status` / `settings.beta_features.set` for Feed, Dock, Extensions, Custom Sidebars, and Remote tmux runtime gates; `settings.sidebar.status` / `settings.sidebar.set` for workspace-row detail visibility, branch/path layout, link routing, and right-sidebar width; `settings.workspace_colors.*` for sidebar indicator, selection, unread badge, and named-palette management; `settings.terminal.status` / `settings.terminal.set` for scrollbar, copy-on-selection, and agent auto-resume; and `settings.terminal.resume.list|update|delete|clear` for signed resume-command approvals, alongside the shortcut, global-hotkey, TextBox, custom-sidebar, and process-isolated sidebar-extension settings methods. |
| `config` | Validate cmux.json syntax, print config references, or reload config. |
| `update` | Run `cmux update check` or `cmux update status` without requiring a running app. Linux fetches the latest stable release with bounded responses, selects the current architecture's archive, validates the matching published SHA-256 entry, and reports whether the installed bundle version is older. `cmux update install` downloads at most 512 MiB, verifies that SHA-256 while streaming, rejects archive paths, links, roots, entry types, and counts outside the bundle contract, then runs the extracted `install.sh`; non-interactive installs require `--yes`, `--prefix` selects an absolute destination, and `--force` permits a verified reinstall. Socket clients can use `app.update.check` or `system.update.check`; installation remains a local confirmed CLI operation. |
| `shortcuts` | Open Settings to Keyboard Shortcuts. |
| `disable-browser` | Disable cmux browser creation and link interception until re-enabled. |
| `enable-browser` | Re-enable cmux browser creation and link interception. |
| `browser-status` | Print whether cmux browser creation and link interception are enabled. |
| `agent-hibernation` | Enable or disable Agent Hibernation. |
| `restore-session` | Restore the previously saved cmux session. Linux first uses in-memory surface resume bindings when present, then falls back to the versioned app snapshot under `$XDG_STATE_HOME/cmux/session-linux.json` or `~/.local/state/cmux/session-linux.json`. Snapshot autoload runs only enabled agent bindings and valid signed `auto` command approvals; `prompt` approvals remain pending for GTK or `surface.resume.run`, and unsigned original startup commands reopen as ordinary shells. |
| `open` | Open files, directories, or URLs in cmux. |
| `diff` | Open a unified diff/patch in a native Linux diff surface. Linux supports patch files, `-`/stdin, `--unstaged`, `--staged`, `--branch`, and `--last-turn` sources without requiring browser creation, plus app-side `diff.open` for command-palette/socket clients. Compatibility HTML remains available through browser inspection RPCs. Linux hook ingestion records the macOS-compatible agent turn baseline store used by `--last-turn`; `cmux diff --comments <json-file>`, `--comments-json`, and `diff.open` `comments`/`review_comments` payloads render read-only review comments using the macOS comment JSON shape. Linux also exposes persisted review comments through `diff.comments.*`, auto-renders stored comments for repository `diff.open` views, and appends pending comment submission text through `workspace.prompt_submit`. |
| `feedback` | Open feedback UI, submit with `--email`, `--body`, and repeated `--image`, or run `feedback retry [--limit <count>]`. Linux queues each validated report and private attachment copy before posting a bounded multipart request to `/api/feedback`; transient failures and rate limits remain pending for retry. |
| `feed` | Open the keyboard-first Feed TUI or manage persisted Feed workstream history. |
| `mobile` | Show Linux mobile host status, list mobile workspaces, drive mobile terminal replay/input/image-paste flows, drive mobile agent chat sessions, or mint attach tickets with `mobile attach-ticket create` for configured Tailscale/debug routes. A configured route starts the native length-prefixed TCP service used by iOS, with same-account Stack authorization, short-lived attach scope, multiplexed requests, workspace/render-grid/notification/chat events, reconnect support, bounded frames, and an ephemeral-port fallback when the preferred port is occupied. Network workspace mutations require explicit UUID targets, expose iOS pin/unread/focus plus latest-notification preview and stable activity-time list state, restrict `workspace.action` to pin/unpin/rename/read-state changes, protect pinned and last workspaces from close, and emit shared-wire `state_changed`, `descriptor_changed`, and `terminal_blocks` chat frames. |
| `themes` | List, set, clear, or interactively pick Ghostty themes. |
| `claude-teams` | Launch Claude Code with cmux/tmux-style agent team integration. |
| `codex-teams` | Launch Codex with cmux-managed subagent panes. |
| `omo` | Launch OpenCode with oh-my-openagent integration. |
| `omx` | Launch Oh My Codex with cmux pane integration. |
| `omc` | Launch Oh My Claude Code with cmux pane integration. |
| `hooks` | Install, uninstall, and run agent hook integrations under one namespace. |
| `codex` | Compatibility alias for installing or uninstalling Codex hooks. |
| `ping` | Check socket connectivity. |
| `capabilities` | Print server capabilities as JSON. Linux advertises deterministic browser automation with `browser_model` and `browser_automation`; `browser_stub` remains only as a deprecated false compatibility flag. |
| `events` | Stream reconnectable cmux events as newline-delimited JSON. |
| `app` | Run the local Linux app shell backed by the Rust cmux core. Supports `--renderer core`, `--renderer gtk`, `--renderer ghostty`, and `--renderer ghostty-vt`; Linux `Super`/`Meta` shortcut aliases are accepted for the macOS-compatible `cmd` debug/socket spelling; an accepted socket `app.quit.request` is consumed by the GTK owner and terminates the installed app process; full `ghostty` embedding is probed through diagnostics with required-symbol and dynamic-load verification, including split-toggle-zoom and selection-clear support, and `--renderer ghostty` uses the initial GTK GLArea host with keyboard, keyboard-map changes, app/surface keybinds, mouse, scroll modifiers, GTK IME preedit/commit, file/text drops, app and surface focus, visibility, color-scheme, clipboard, close-request/confirmation, stable refresh, cwd, startup-command, wait-after-command, startup-input, environment, inherited-font-size forwarding, renderer-owned initial terminal startup plus queued socket/app text-key input, embedded viewport readback, event-driven selection readback, TTY/process/grid/cell-size metadata sync for debug and memory diagnostics, Ghostty runtime title/cwd/size-limit/initial-size/cell-size/renderer-health/prompt-title/quit-timer/float-window/secure-input/color-change/config-change/notification/bell/open-url/progress/command-finished/child-exited/search/scrollbar/command-palette/config-open/config-reload/readonly/copy-title/cursor actions, Ghostty new-window/close-window/goto-window/new-tab/move-tab/goto-tab/new-split/present-terminal/close-tab/goto-split/resize-split/equalize-splits/toggle-split-zoom layout actions, host GTK handling for the GTK inspector, render requests, quit/close-all-windows, Ghostty fullscreen compatibility requests as native fullscreen on Linux, maximize, visibility/quick-terminal toggles, reset-window-size, window decorations, on-screen keyboard focus requests, and a native update-check dialog backed by the validated Linux release checker, plus an OpenGL Ghostty inspector overlay with focus/mouse/scroll/text/common-key routing and structured key-sequence/key-table terminal state when built with `--features gtk`. |
| `renderer` | Print renderer snapshots or diagnostics for `core`, `gtk`, `ghostty`, and `ghostty-vt`, including GTK runtime/linker development probes, full `ghostty-internal` loadability checks, loaded-library embedding ABI/layout self-report checks, and portable `libghostty-vt` symbol checks, or apply renderer size allocations to pane PTYs. |
| `auth` | Manage auth status, browser login, logout, and the active team through the app. Linux Settings exposes the same account identity, credential-store status, and persisted team selection through `auth.team.select`. |
| `vm`, `cloud` | Manage cloud VMs. `cloud` is an alias for `vm`. |
| `remotes`, `remote` | Manage the Linux local registry of iOS-visible remote hosts. `remote` is an alias for `remotes`. |
| `rpc` | Call a raw v2 socket method with optional JSON params. |
| `identify` | Print server identity and caller context. |
| `list-windows` | List windows. |
| `current-window` | Print the selected window ID. |
| `new-window` | Create a new window. |
| `focus-window` | Focus a window by handle. |
| `close-window` | Close a window by handle. |
| `window displays` | List connected displays (name, index, main flag). |
| `window display <name\|index>` | Move the instance's window(s) onto a display by name (exact, substring) or index, preserving size. Does not steal focus. With `--window`, targets that window; otherwise moves all main windows. `--list` aliases `window displays`. |
| `window default-display [<name>\|--clear]` | Set, show (no arg), or clear (`--clear`) the shared, cross-tag default display that DEBUG dev builds open new windows on, stored in `~/.config/cmux/cmux.json` under `app.devWindowDisplay`. No running app required; applied at window creation. Also settable in Debug > Debug Windows > Dev Window Display. |
| `move-workspace-to-window` | Move a workspace into a target window. |
| `reorder-workspace` | Reorder a workspace inside a window. |
| `reorder-workspaces` | Atomically reorder workspaces inside pinned and unpinned groups. |
| `workspace-action` | Run workspace context-menu actions from the CLI. |
| `workspace` | Namespace for workspace verbs: `list`, `create`, `env`, `close`, `rename`, `select`, `reconnect`, `disconnect`, `group`. `workspace env` prints a workspace's configured environment variables (see [Workspace environment variables](#workspace-environment-variables)); pass `--mask` to redact the values. `workspace reconnect` manually reconnects a remote (SSH) workspace — including one whose automatic reconnect suspended because the host was unreachable — and `workspace disconnect` stops its remote connection. `env`, `reconnect`, and `disconnect` accept a positional workspace handle or `--workspace <id\|ref\|index>`, defaulting to the caller's workspace, then the selected one. |
| `move-tab-to-new-workspace` | Move a tab or surface into a newly created workspace. |
| `list-workspaces` | List workspaces. |
| `new-workspace` | Create a workspace, optionally with cwd, command, description, layout, and per-workspace environment variables (`--env KEY=VALUE` repeatable, `--env-file <path>`). See [Workspace environment variables](#workspace-environment-variables). |
| `ssh` | Open an SSH-backed workspace. Preserves the caller's live `SSH_AUTH_SOCK` for app-launched OpenSSH processes so `ForwardAgent yes` from ssh_config works normally. Supports `-A` / `--forward-agent` to request forwarding and `-a` / `--no-forward-agent` to disable forwarding for a workspace. Agent forwarding remains opt-in because forwarded agents can be used by processes on the remote host while the SSH session is active. |
| `ssh-tmux` | Open a dedicated cmux window that mirrors a remote host's tmux sessions. Supports `--port`, `--identity`, `--no-focus`, `--json`, and Linux `--live`; the Linux CLI honors the macOS-compatible `auth_required`/`ssh_argv` handoff by running vetted `/usr/bin/ssh` in the caller's terminal and retrying once. With `--live` or socket `live: true`, Linux probes `tmux list-sessions` over SSH with BatchMode, creates a default remote session for an empty reachable server, and returns `auth_required` when interactive authentication is needed. It then keeps a persistent `ssh -tt ... tmux -CC` stream per session: tmux windows become primary cmux tabs, the selected window's recursive pane layout is projected into native cmux split panes, initial contents are captured per pane, live output is injected into independent Ghostty manual-I/O surfaces, and focus, terminal input, new-window, right/down split, rename, pane/window close, detach, and client-size operations route back through control mode. Switching a window tab replaces the projected split tree while preserving the primary window tabs. Unexpected SSH/control-process exits use bounded exponential reconnect with topology and surface bindings restored after reattach. |
| `remote-daemon-status` | Print bundled remote daemon version, asset, checksum, and cache status. |
| `ssh-session-list` | List persisted SSH PTY sessions for one remote workspace or all remote workspaces with `--all`. Supports `--json`. |
| `ssh-session-attach` | Create a local terminal surface that reattaches to an existing persisted SSH PTY session. |
| `ssh-session-cleanup` | Close one or all persisted SSH PTY sessions. Supports `--json`. |
| `ssh-session-snapshot` | Export persisted SSH PTY session metadata for app relaunch restore. Supports `--json` and `--workspace`. |
| `ssh-session-restore` | Restore persisted SSH PTY session metadata from `--file`, `--snapshot`, or stdin, preserving only entries with persistent daemon slots. |
| `new-split` | Split from a surface in a direction. |
| `list-panes` | List panes in a workspace. |
| `list-pane-surfaces` | List surfaces in a pane. |
| `tree` | Print a window, workspace, pane, and surface tree. |
| `top` | Print process/resource usage for cmux windows, workspaces, panes, and surfaces. |
| `focus-pane` | Focus a pane. |
| `new-pane` | Create a pane with terminal or browser content. |
| `new-surface` | Create a surface inside a pane. |
| `close-surface` | Close a surface. |
| `move-surface` | Move a surface to another pane, workspace, window, or index. |
| `split-off` | Move a surface into a new split without changing focus by default. |
| `reorder-surface` | Reorder a surface within its pane. |
| `tab-action` | Run horizontal tab context-menu actions. |
| `rename-tab` | Rename a tab. Compatibility wrapper for `tab-action rename`. |
| `drag-surface-to-split` | Move a surface into a split direction. |
| `refresh-surfaces` | Ask the app to refresh terminal surfaces. |
| `reload-config` | Ask cmux to reload configuration. |
| `surface-health` | Print terminal surface health information. |
| `debug-terminals` | Print debug terminal state. |
| `trigger-flash` | Trigger a visual flash on a workspace or surface. |
| `list-panels` | List panels. Compatibility alias over pane/surface data. |
| `focus-panel` | Focus a panel. Compatibility alias over surface focus. |
| `close-workspace` | Close a workspace. |
| `select-workspace` | Select a workspace. |
| `rename-workspace`, `rename-window` | Rename a workspace. `rename-window` is a compatibility alias. |
| `current-workspace` | Print current workspace information. |
| `read-screen` | Read terminal text from a surface. |
| `send` | Send text to a terminal surface. |
| `send-key` | Send one key to a terminal surface. |
| `send-panel` | Send text to a panel/surface. |
| `send-key-panel` | Send one key to a panel/surface. |
| `notify` | Send a notification to a workspace/surface. |
| `list-notifications` | List queued notifications, including `created_at` and `tab_title`. |
| `dismiss-notification` | Remove one notification, or remove already-read notifications with `--all-read`. |
| `mark-notification-read` | Mark one notification, a workspace/surface scope, or all notifications read. |
| `open-notification` | Focus the notification's workspace/surface and mark it read. |
| `jump-to-unread` | Focus the latest unread notification. |
| `clear-notifications` | Clear queued notifications. |
| `right-sidebar` | Control right sidebar visibility, mode, focus, and state reads. |
| `sidebar validate [name]` | Validate all custom left sidebars or one named sidebar. Linux renders declarative JSON and a bounded interpreted SwiftUI-style subset with live workspace data, parameterized actions, persisted top-level `@State`, native `Toggle`/`TextField`/`Slider`/`Picker`/`Stepper` bindings, post-write `.onChange`, inherited `.onSubmit`, and configurable in-process or isolated-worker evaluation. |
| `sidebar reload [name]` | Revalidate custom sidebars and advance the live reload generation. |
| `sidebar select <name\|workspaces>` | Persist and activate a custom left sidebar, or restore the built-in workspace list. |
| `sidebar clear-state [name]` | Clear the selected or named custom sidebar's private persisted state. The next render reseeds values from its `@State` initializers. |
| `set-status` | Set a sidebar status pill. |
| `clear-status` | Remove a sidebar status pill. |
| `list-status` | List sidebar status pills. |
| `set-progress` | Set sidebar progress. |
| `clear-progress` | Clear sidebar progress. |
| `log` | Append a sidebar log entry. |
| `clear-log` | Clear sidebar log entries. |
| `list-log` | List sidebar log entries. |
| `sidebar-state` | Dump sidebar metadata state. |
| `claude-hook` | Compatibility alias for Claude Code hook events from stdin JSON. |
| `set-app-focus` | Override app focus state for tests. |
| `simulate-app-active` | Trigger app-active handling for tests. |
| `browser` | Run browser automation commands. |
| `open-browser` | Deprecated legacy alias for `browser open`; non-JSON CLI calls warn on stderr. |
| `navigate` | Deprecated legacy alias for `browser navigate`; non-JSON CLI calls warn on stderr. |
| `browser-back` | Deprecated legacy alias for `browser back`; non-JSON CLI calls warn on stderr. |
| `browser-forward` | Deprecated legacy alias for `browser forward`; non-JSON CLI calls warn on stderr. |
| `browser-reload` | Deprecated legacy alias for `browser reload`; non-JSON CLI calls warn on stderr. |
| `get-url` | Deprecated legacy alias for `browser get-url`; non-JSON CLI calls warn on stderr. |
| `focus-webview` | Deprecated legacy alias for `browser focus-webview`; non-JSON CLI calls warn on stderr. |
| `is-webview-focused` | Deprecated legacy alias for `browser is-webview-focused`; non-JSON CLI calls warn on stderr. |
| `markdown` | Open a markdown file in a formatted viewer panel with live reload. |
| `vm-pty-attach` | Internal VM PTY attach command. |
| `vm-ssh-attach` | Hidden compatibility alias for older VM workspaces. |
| `vm-pty-connect` | Internal helper that connects to a VM PTY from a config file. |
| `ssh-pty-attach` | Internal helper used by SSH terminal startup scripts to bridge a local terminal surface to a remote PTY session. |
| `ssh-session-end` | Internal helper that clears remote SSH session state. |
| `__tmux-compat` | Internal tmux compatibility dispatcher. |

Linux app renderer default:

`cmux app` reads `CMUX_LINUX_RENDERER` when `--renderer` is omitted and falls
back to `core` when the variable is unset. `--renderer` remains an explicit
override and accepts the same renderer names and aliases as the environment
default path.

## Command Families

Auth subcommands:

| Command | Contract |
| --- | --- |
| `auth status` | Print signed-in state. Supports `--json`. |
| `auth login` | Begin sign-in through the app and wait for completion. |
| `auth logout` | Clear the current session. |
| `auth team <team-id\|none>` | Select or clear the active team used by team-scoped cloud requests. The selection is persisted with the private Linux auth state. |

VM subcommands:

| Command | Contract |
| --- | --- |
| `vm ls`, `vm list` | List VMs. |
| `vm new`, `vm create` | Create a VM. Supports `--image`, `--provider`, `--detach`, and `-d`. |
| `vm shell`, `vm attach` | Open an interactive shell for an existing VM. |
| `vm rm`, `vm destroy`, `vm delete` | Destroy a VM. |
| `vm ssh` | Open a cmux-managed SSH workspace for an existing VM. |
| `vm ssh-info` | Print SSH connection info. |
| `vm ssh-attach` | Internal attach helper. |
| `vm exec` | Run a shell command inside a VM. |

Remotes subcommands:

| Command | Contract |
| --- | --- |
| `remotes list`, `remotes ls` | List the team's registered remotes (name, deviceId, routes, tag, last seen). Supports `--json`. |
| `remotes add <name>` | Register or update a remote with one or more `--route <host:port>`. Supports `--tag` and `--json`. Idempotent on `<name>` (re-adding updates routes). The host must be a Tailscale address the phone can authenticate to (CGNAT `100.64.x.x`-`100.127.x.x` or `*.ts.net`); loopback, plain LAN IPs, and bare hostnames are rejected. |
| `remotes remove <name-or-deviceId>` | Remove a remote you registered. Aliases `rm`, `delete`. Supports `--json`. |

Theme subcommands:

| Command | Contract |
| --- | --- |
| `themes` | In a TTY, open the interactive picker. Outside a TTY, list themes. |
| `themes list` | List available themes and current light/dark defaults. |
| `themes set <theme>` | Set the same theme for light and dark appearance. |
| `themes set --light <theme>` | Set the light appearance theme. |
| `themes set --dark <theme>` | Set the dark appearance theme. |
| `themes clear` | Remove the cmux theme override. |

Workspace and tab action names:

| Command | Actions |
| --- | --- |
| `workspace-action` | `pin`, `unpin`, `rename`, `clear-name`, `set-description`, `clear-description`, `move-up`, `move-down`, `move-top`, `close-others`, `close-above`, `close-below`, `mark-read`, `mark-unread`, `set-color`, `clear-color` |
| `tab-action` | `rename`, `clear-name`, `close-left`, `close-right`, `close-others`, `new-terminal-right`, `new-browser-right`, `reload`, `duplicate`, `move-to-new-workspace`, `pin`, `unpin`, `mark-read`, `mark-unread` |

### Window, workspace, pane, and surface handles

cmux uses four stable topology levels. CLI commands should preserve these names
in new user-facing output and accept UUIDs, short refs, or indexes wherever a
handle is documented.

| Term | Meaning | Short ref | Common commands |
| --- | --- | --- | --- |
| `window` | Top-level native app window. A window owns an ordered workspace list. | `window:N` | `window list`, `window focus`, `move-workspace-to-window`, `new-window` |
| `workspace` | A workspace tab inside a window. It owns a split-tree of panes. | `workspace:N` | `workspace list`, `workspace select`, `new-workspace`, `workspace-action` |
| `pane` | A leaf split region inside one workspace. It owns one or more surfaces/tabs. | `pane:N` | `list-panes`, `focus-pane`, `new-split`, `pane.surfaces` |
| `surface` | A terminal, browser, markdown, or settings surface inside a pane. Legacy CLI text may call this a tab or panel. | `surface:N` | `surface focus`, `browser <surface> ...`, `send`, `read-screen`, `tab-action` |

Targeting rules:

- `--window`, `--workspace`, `--pane`, `--surface`, and `--tab` accept UUIDs,
  short refs, and numeric indexes where the command supports that level.
- `tab` is a compatibility synonym for a surface in horizontal tab-strip
  commands; new socket APIs should prefer `surface_id`.
- `panel` is a compatibility synonym for older pane/surface UI language.
- Browser commands target a browser `surface`. `cmux browser surface:2 url` and
  `cmux browser --surface surface:2 url` are equivalent.
- Commands run from a cmux terminal may use `CMUX_WORKSPACE_ID`,
  `CMUX_PANE_ID`, `CMUX_SURFACE_ID`, `CMUX_TAB_ID`, and `CMUX_PANEL_ID` as caller
  context when no explicit target is provided.

### Workspace environment variables

A workspace can carry a set of user-defined environment variables that every
shell spawned in it inherits.

Setting them:

- CLI: `cmux new-workspace --env KEY=VALUE [--env ...] [--env-file <path>]`
  (and the same flags on `cmux workspace create`). `--env` is repeatable;
  `--env-file` reads `KEY=VALUE` lines (blank lines and `#` comments ignored, an
  optional leading `export ` stripped). When both are given, `--env` overrides a
  value from a file.
- Project config (`cmux.json`): an `env` object on a workspace definition, e.g.
  `{ "name": "Build", "cwd": ".", "env": { "AWS_PROFILE": "prod" } }`.
- Socket: the `workspace_env` param on `workspace.create`.

Inspecting them: `cmux workspace env [<handle>] [--mask] [--json]` prints the
configured set. `--mask` redacts the values so secrets are not echoed in full.
The env set is intentionally omitted from `workspace list` output so a plain
listing never leaks secrets.

Semantics:

- **Inheritance.** The variables apply to the workspace's initial shell and to
  every pane, surface, and split created later in that workspace — no per-pane
  re-export. They are also re-applied to every shell recreated on session
  restore.
- **Persistence.** They are stored on the workspace in the session manifest, so
  they survive app restart, daemon restart, and session restore.
- **Precedence.** Workspace env overlays the inherited process environment. It is
  applied as the shell's startup environment, so it is visible to login-shell
  init files (`~/.zprofile`, `~/.zshrc`) as they run, but any `export` those
  files perform for the same key wins for the interactive session (they run after
  the variable is seeded). An explicit per-surface environment (a layout
  `surfaces[].env`, `surfaces[].terminal_env`, `surface_env`, `initial_env`, or
  SSH startup env) overrides the workspace value for that surface.
- **Protected `CMUX_*` variables.** Workspace env can never override the managed
  variables cmux injects (e.g. `CMUX_WORKSPACE_ID`, `CMUX_PANE_ID`,
  `CMUX_SURFACE_ID`, `CMUX_TAB_ID`, `CMUX_PANEL_ID`, `CMUX_SOCKET_PATH`,
  `CMUX_SOCKET_PASSWORD`) or the terminal identity variables (`TERM`,
  `COLORTERM`, `TERM_PROGRAM`, `TERM_PROGRAM_VERSION`); those keys are protected
  at spawn time and silently win.
- **Secrets.** Values may be secrets. They are never logged, are masked by
  `--mask`, and are kept out of `workspace list`. Prefer `--env-file` so secrets
  do not land in shell history. Note that values stored in the session manifest
  live on disk in plaintext.

tmux compatibility commands:

| Command | Contract |
| --- | --- |
| `capture-pane` | Read pane text. |
| `resize-pane` | Resize a pane with direction flags. |
| `pipe-pane` | Pipe pane text to a shell command. |
| `wait-for` | Signal or wait on a named synchronization point. |
| `swap-pane` | Swap two panes. |
| `break-pane` | Move a pane into a new workspace. |
| `join-pane` | Join a pane into another pane. |
| `next-window`, `previous-window`, `last-window` | Move workspace selection. |
| `last-pane` | Focus the last pane. |
| `find-window` | Find a workspace by title or content. |
| `clear-history` | Clear terminal scrollback. |
| `set-hook` | Manage tmux-compat hook definitions. |
| `popup` | Create a popup-like terminal surface for a command, or display a popup notification when no command is provided. |
| `bind-key`, `unbind-key` | Store, list, and remove tmux-compat key binding definitions. |
| `copy-mode` | Toggle tmux-compat copy-mode state. |
| `set-buffer` | Set a tmux-compat buffer. |
| `paste-buffer` | Paste a tmux-compat buffer. |
| `list-buffers` | List tmux-compat buffers. |
| `respawn-pane` | Send a restart command to a surface. |
| `display-message` | Print or display a message. |

Browser subcommands:

| Command | Contract |
| --- | --- |
| `browser open`, `browser open-split`, `browser new` | Create or open a browser surface. |
| `browser connect` | Connect to an existing cmux browser surface, or create a deterministic Linux browser surface when none exists. Supports `--surface`, `--url`, `--workspace`, `--create`, `--no-create`, and `--focus`. |
| `browser window new`, `browser window create` | Create a new cmux window with an initial browser surface. Supports `--title`, `--window-title`, `--workspace-title`, and `--no-focus`. |
| `browser close`, `browser quit`, `browser exit` | Close the targeted browser surface. |
| `browser goto`, `browser navigate` | Navigate to a URL. |
| `browser back`, `browser forward`, `browser reload` | Navigate browser history or reload. |
| `browser url`, `browser get-url` | Print current URL. |
| `browser focus-webview`, `browser is-webview-focused` | Focus or query webview focus. |
| `browser snapshot` | Print an accessibility-oriented DOM snapshot with ephemeral `eN` selector refs. Mounted WebKitGTK pages are traversed live; display-free runs use the deterministic model. |
| `browser eval` | Evaluate JavaScript in the targeted live WebKit document and return its JSON-compatible value, awaiting returned promises. Display-free runs and systems without a mounted WebKit view use the deterministic Linux browser-model subset. |
| `browser wait` | Wait for selector, text, URL, load state, or JS predicate. Supports positional selector plus `--selector`, `--text`, `--url-contains`, `--function`, and `--load-state`. |
| `browser click`, `browser dblclick`, `browser hover`, `browser focus`, `browser check`, `browser uncheck`, `browser scroll-into-view` | Run element interaction. |
| `browser drag`, `browser upload` | Drag/drop is dispatched into the mounted WebKitGTK document with a shared `DataTransfer` object. Uploads select real local files through WebKitGTK's file-chooser request before activating the target input. Both retain deterministic model fallback; `upload` supports `--file`/`--path` or a positional path. |
| `browser bringtofront` | Focus and foreground the browser surface. |
| `browser type`, `browser fill` | Type into or set an input. |
| `browser press`, `browser key`, `browser keydown`, `browser keyup`, `browser keyboard` | Send keyboard input. |
| `browser select`, `browser multiselect` | Select one or more options. |
| `browser scroll` | Scroll page or element. |
| `browser screenshot`, `browser pdf` | Save browser artifacts. Both support `--out`/`--path` or a positional output path. Screenshots use the mounted WebKitGTK view when available and accept `--full-page`; PDFs print the mounted live document through WebKitGTK. Display-free runs use deterministic model artifacts. |
| `browser get` | Read URL, title, text, HTML, value, attr, count, box, or styles. DOM-backed reads return the mounted WebKitGTK document value and fall back to the deterministic model when no native view is available. |
| `browser content`, `browser innertext`, `browser setcontent`, `browser setvalue`, `browser inserttext`, `browser selectall`, `browser clear`, `browser clipboard` | Read mounted WebKitGTK document content/text with deterministic model fallback; replace content, edit field text, track selection, and read/write the Linux browser clipboard subset. |
| `browser is` | Check visible, enabled, or checked state in the mounted WebKitGTK document, with deterministic model fallback. |
| `browser find` | Find by role, text, label, placeholder, alt, title, testid, first, last, or nth. |
| `browser frame` | Select frame context. |
| `browser dialog` | Accept, dismiss, or respond to dialogs. |
| `browser download` | Wait for or save downloads. |
| `browser profiles` | List, add, select, rename, clear, or delete cmux browser profiles. `select <profile> [--surface <surface>]` changes the default for new tabs and optionally rebinds a live browser tab while retaining its current URL, back/forward history, zoom, and developer-tools state. Selecting or explicitly opening a profile records it as that workspace's preferred profile, so later browser tabs inherit it independently in each workspace; this preference survives session restoration. The GTK browser toolbar exposes the same per-tab selection and profile creation flow. Browser surfaces carry a stable profile ID, and GTK tabs in the same profile share one persistent WebKitGTK network/data session while different profiles use isolated mode-`0700` XDG data and cache directories. Profile metadata, native-data generation, bounded history and bookmarks, plus portable imported homepage/search preferences persist in a private atomic XDG state file. Clearing a profile advances its native-data generation and clears its imported history, bookmarks, and settings; startup removes stale generations and deleted-profile directories. `clear` requires `--force`. |
| `browser import` | Import browser data non-interactively or open the browser-backed import surface with `--interactive`. Linux detects Firefox, Chrome, Chromium, Brave, and Edge profiles and imports available cookies, normalized history, bookmark folder trees, homepage, bookmark-bar visibility, and known or custom search-provider templates. The native Browser settings page uses `browser.import.sources` for source/profile selection. Supported scopes include `cookies`, `history`, `bookmarks`, `settings`, `cookies-and-history`, `additional`, and `all`; explicit files use `--cookies-file`, `--bookmarks-file`, and `--settings-file`. `--from`, `--profile`, `--all-profiles`, `--to-profile`, `--create-profile`, `--workspace`, and repeatable `--domain` filtering remain available. Encrypted Chromium cookie values that require keyring decryption are reported and skipped. Chromium/Firefox extensions are not binary-compatible with WebKitGTK and are explicitly reported as not imported. |
| `browser cookies` | Get, set, or clear cookies. |
| `browser storage` | Get, set, or clear local/session storage. Explicit mutations synchronize the model and live WebKit document. |
| `browser tab` | Create, list, switch, or close browser tabs. |
| `browser console`, `browser errors` | List, show, or clear console messages and list errors. |
| `browser devtools`, `browser react-grab`, `browser focus-mode`, `browser zoom` | Toggle browser panel controls, React grab mode, focus mode, or page zoom. |
| `browser history list`, `browser history search`, `browser history clear` | List or search the targeted surface's profile, or an explicitly selected/current profile, and clear it with explicit `--force`. History rows include stable IDs, URL/title, last-visited milliseconds, and visit count. |
| `browser bookmarks list`, `browser bookmarks search`, `browser bookmarks clear` | List or search profile-scoped imported bookmarks, preserving stable IDs, URL/title, folder path, and added time. Clear requires explicit `--force`. |
| `browser.omnibar.resolve`, `browser.omnibar.suggestions` | Resolve native omnibar input as an HTTP(S)/file URL or configured search query, and return bounded suggestions from the targeted surface's browser profile, matching open browser tabs, imported bookmarks, history, and optional supported search providers. Imported per-profile search settings override the global engine only for that profile. Linux matches the macOS bare-host, host-with-port, localhost, search-engine template, one-character suppression, and URL-intent rules. The GTK address field fetches provider predictions on a debounced background worker, discards stale results, labels open-tab/bookmark rows, switches to existing surfaces, and supports Up, Down, Enter, and Escape. Provider requests are controlled by `browser.showSearchSuggestions`; Google, DuckDuckGo, Bing, Kagi, and Startpage are supported. |
| `browser highlight` | Highlight an element. |
| `browser state` | Save or load URL, cookies, local storage, session storage, and frame-selector state. Linux also loads the legacy flat local-storage map. |
| `browser addinitscript`, `browser addscript`, `browser addstyle`, `browser dispatch`, `browser expose`, `browser evalhandle` | Inject scripts/CSS, dispatch DOM events, expose page functions, or create eval handles. `evalhandle` stores the mounted WebKitGTK result when available and otherwise uses the deterministic model. |
| `browser viewport` | Set viewport size. |
| `browser device` | Set device preset, viewport scale, mobile, and touch emulation. Device scale and touch capability are reflected in the live WebKit document; `browser device list` lists deterministic Linux preset metadata. |
| `browser set` | Agent-browser compatibility umbrella for emulation/settings families such as `viewport`, `device`, `geo`, `offline`, `headers`, `credentials`, `auth`, and `media`. |
| `browser geolocation`, `browser geo` | Set page-observable geolocation in the live WebKit document. |
| `browser useragent`, `browser locale`, `browser timezone`, `browser media` | Set persistent live WebKit user-agent, language/timezone, and media/color-scheme/reduced-motion emulation. |
| `browser headers`, `browser credentials`, `browser permissions` | Set headers, Basic credentials, and page-observable permission states for the active browser surface. Mounted WebKitGTK pages apply headers and credentials to native document and subresource requests and answer HTTP authentication challenges; display-free runs retain request-model behavior. |
| `browser offline` | Toggle page-observable online state and events. Mounted Linux WebKit surfaces also block native network requests until online mode is restored. |
| `browser trace` | Start or stop trace capture; `start` accepts `--path` or a positional output path. |
| `browser har` | Start or stop HAR-style network recording; `start` accepts `--path` or a positional output path. |
| `browser video`, `browser record` | Start, stop, or restart frame-based Linux recording artifacts. Mounted GTK browser surfaces capture periodic native WebKit frames; headless and unavailable-runtime paths retain deterministic model frames. Each frame reports `source: webkit|model`. |
| `browser network` | Route, unroute, list requests, or read a browser request response body. |
| `browser screencast` | Start or stop screencast with the same native WebKit capture and deterministic model fallback behavior as video recording. |
| `browser input`, `browser input_mouse`, `browser input_keyboard`, `browser input_touch`, `browser mouse`, `browser tap`, `browser swipe` | Send low-level keyboard, mouse, and touch input. |
| `browser identify` | Identify browser surface context. |

Browser focus mode is also available from the GTK toolbar and the stable `toggleBrowserFocusMode` shortcut (`Super+Alt+Enter`). While active, keyboard input is routed to the focused page before cmux shortcuts. Press and release Escape twice within 1.6 seconds to exit.

React Grab uses the stable `toggleReactGrab` shortcut (`Super+Shift+G`). It targets the focused browser, or the sole browser in the workspace when a terminal is focused; ambiguous multi-browser terminal routes are left untouched.

### Browser automation workflow

LLM-facing scripts should make browser state explicit and verify after every
state-changing action:

1. Identify context and available browser surfaces.

   ```bash
   cmux browser identify --json
   cmux identify --json
   ```

2. Choose a target surface from the focused browser context or open one.

   ```bash
   cmux browser open https://example.com --json
   cmux browser surface:2 identify --json
   ```

3. Snapshot before acting so selectors and element refs are fresh.

   ```bash
   cmux browser surface:2 snapshot --interactive --compact --json
   ```

4. Act with explicit targeting and request a post-action snapshot for mutating
   actions when possible.

   ```bash
   cmux browser surface:2 click "button[type='submit']" --snapshot-after --json
   cmux browser surface:2 fill "#email" --text "ops@example.com" --snapshot-after --json
   ```

5. Verify through a second read of user-visible state.

   ```bash
   cmux browser surface:2 wait --text "Saved" --timeout-ms 10000 --json
   cmux browser surface:2 get text "main" --json
   cmux browser surface:2 url --json
   ```

Hook subcommands:

| Command | Contract |
| --- | --- |
| `hooks setup` | Install hooks for all supported agents whose binaries are on `PATH`. Supports `--agent <name>`, positional agent filters such as `cmux hooks setup rovo`, and `--yes`. |
| `hooks uninstall` | Remove hooks for all supported agents. Supports `--agent <name>`, positional agent filters such as `cmux hooks uninstall rovo`, and `--yes`. |
| `hooks <agent> install` | Install hooks for one supported agent. `opencode` also supports `--project` for the project-local Feed plugin. |
| `hooks <agent> uninstall` | Remove hooks for one supported agent. |
| `hooks claude <event>` | Handle Claude Code hook events. `claude-hook <event>` remains as the main-compatibility alias. |
| `hooks codex <event>` | Handle Codex hook events. `codex install-hooks` remains as the main-compatibility installer alias. |
| `hooks feed --source <agent>` | Convert agent hook events into Feed context. |
| `hooks <agent> <event>` | Generic hook surface for `grok`, `opencode`, `pi`, `amp`, `cursor`, `gemini`, `rovodev`, `copilot`, `codebuddy`, `factory`, and `qoder`. `UserPromptSubmit` records a last-turn diff baseline when workspace/surface/cwd context is available; `PreToolUse` records one only if a baseline for the current turn is missing. |

Right sidebar commands:

| Command | Contract |
| --- | --- |
| `right-sidebar toggle`, `right-sidebar show`, `right-sidebar hide` | Change right-sidebar visibility without printing on success. |
| `right-sidebar focus` | Focus the current right-sidebar mode. |
| `right-sidebar set <files\|find\|vault\|sessions\|feed\|dock>` | Show the right sidebar, switch mode, and focus it unless `--no-focus` is passed. |
| `right-sidebar files`, `right-sidebar find`, `right-sidebar vault`, `right-sidebar sessions`, `right-sidebar feed`, `right-sidebar dock` | Short aliases for `right-sidebar set <mode>` with focus. |
| `right-sidebar mode` | Print JSON with `visible` and `mode`. |
| `--workspace <id\|ref\|index>` | Target the window containing a workspace. Refs and indexes resolve before the V1 socket command is sent. |
| `--window <id\|ref\|index>` | Target a window. Refs and indexes resolve before the V1 socket command is sent. |
| `--no-focus` | Only valid with `set`; switches mode without moving focus. |

Docs topics:

| Command | Contract |
| --- | --- |
| `docs` | List docs topics without a socket. |
| `docs settings` | Print the configuration docs URL, raw schema URL, cmux.json paths, backup reminder, and reload command. |
| `docs shortcuts` | Print shortcut docs and raw shortcut data resources. |
| `docs api` | Print API docs and raw CLI contract resources. |
| `docs browser` | Print browser automation docs and raw browser skill resources. |
| `docs agents` | Print agent integration docs and raw integration resources. |

Settings subcommands:

| Command | Contract |
| --- | --- |
| `settings` | Open the Settings window, launching cmux if needed. |
| `settings open [target]` | Open Settings to an optional target section. |
| `settings path` | Print cmux.json paths, docs URL, schema URL, backup reminder, and reload command without a socket. |
| `settings docs` | Print the same output as `docs settings` without a socket. |
| `settings <target>` | Open Settings to a target section. Supported aliases include `shortcuts`, `json`, `cmux-json`, `browser`, and `automation`. Linux renders functional native Account, Mobile, Automation, and Workspace Colors sections. Workspace Colors edits the indicator style, selection/unread colors, and complete named palette also used by workspace context-menu swatches. |

Config subcommands:

| Command | Contract |
| --- | --- |
| `config doctor [--path <file>]`, `config check`, `config validate` | Validate JSONC syntax for config files. When `--path` is absent, default discovery checks the primary config, project-level `.cmux/cmux.json` or `cmux.json`, and legacy config files. `--path <file>` may be repeated to validate multiple explicit files. Exits 0 on success and 1 on any error. Supports `--json`. Works without a socket. |
| `config path`, `config paths` | Print cmux.json paths, docs URL, schema URL, backup reminder, and reload command without a socket. |
| `config docs`, `config documentation` | Print the same output as `docs settings` without a socket. |
| `config reload` | Ask the running cmux app to reload configuration. Requires a socket. |
| `config get sidebar-font-size` | Print the effective sidebar text size. |
| `config set sidebar-font-size <points>` | Write the sidebar text size to cmux's editable Ghostty config and reload the running app when available. |
| `config sidebar-font-size [points]` | Get the sidebar text size, or set it when a point size is provided. |
| `config get surface-tab-bar-font-size` | Print the effective workspace tab bar text size. |
| `config set surface-tab-bar-font-size <points>` | Write the workspace tab bar text size to cmux's editable Ghostty config and reload the running app when available. |
| `config surface-tab-bar-font-size [points]` | Get the workspace tab bar text size, or set it when a point size is provided. |
| `config get <key>`, `config set <key> <points>` | Generic get/set for `sidebar-font-size` and `surface-tab-bar-font-size`. |

`config doctor --json` outputs an object with `ok`, `error_count`,
`findings`, `reload_command`, `docs_url`, and `schema_url`. Each finding includes
`label`, `display_path`, `path`, `status`, `ok`, `keys`, and, when available,
`message` and `bytes`.

Events command:

| Option | Contract |
| --- | --- |
| `--after <seq>`, `--after-seq <seq>` | Subscribe to retained events after a sequence number. |
| `--cursor-file <path>` | Read the starting sequence from a file and update it after every event. |
| `--name <event>` | Filter by event name. Repeatable. |
| `--category <name>` | Filter by category. Repeatable. |
| `--reconnect` | Reconnect and resume from the last received sequence until interrupted. |
| `--limit <n>` | Exit after printing `n` event frames. |
| `--no-ack` | Suppress the initial ack frame in stdout. |
| `--no-heartbeat`, `--no-heartbeats` | Suppress heartbeat frames in stdout. |

`events.stream` is a v2 socket method advertised by `capabilities`. The first
response frame is an `ack`; sequence resume metadata lives under `ack.resume` as
`after_seq`, `oldest_seq`, `latest_seq`, `next_seq`, and `gap`. Event frames
carry a process-local monotonic `seq` and a stable `id` for dedupe. Clients
should persist `seq` after processing each event and reconnect with that value.
See [events.md](events.md) for the full protocol and event catalog. Every emitted event is also appended to
`~/.cmuxterm/events.jsonl`, including model lifecycle events for window
creation, close, focus, key-window state, workspace selection, pane focus, and
surface selection, focus, creation, closure, and terminal create/input/paste,
viewport, and mouse RPCs. Terminal text and image payloads are redacted before
event recording. The stream is bounded: cmux keeps
4,096 replay events in memory, caps each encoded event frame at 16 KiB, closes
slow subscribers after 1,024 pending events, and rotates `events.jsonl` with one
16 MiB archive at `events.jsonl.1`.

## No-Socket Help Probes

The following probes are executable contract checks. They must exit 0 and print
the expected text without connecting to a cmux socket.

<!-- cli-contract-help-probes:start -->
- `cmux --help` -> `cmux - control cmux Linux via Unix socket`
- `cmux --help` -> `open <path-or-url>...`
- `cmux help` -> `cmux - control cmux Linux via Unix socket`
- `cmux ping --help` -> `Usage: cmux ping`
- `cmux capabilities --help` -> `Usage: cmux capabilities`
- `cmux events --help` -> `Usage: cmux events [options]`
- `cmux auth --help` -> `Usage: cmux auth <status|login|logout>`
- `cmux vm --help` -> `Usage: cmux vm <new|ls|rm|exec|shell|attach|ssh|ssh-info> [args...]`
- `cmux cloud --help` -> `Usage: cmux cloud <new|ls|rm|exec|shell|attach|ssh|ssh-info> [args...]`
- `cmux remotes --help` -> `Usage: cmux remotes <list|add|remove> [options]`
- `cmux remote --help` -> `Usage: cmux remotes <list|add|remove> [options]`
- `cmux rpc --help` -> `Usage: cmux rpc <method> [json-params]`
- `cmux help --help` -> `Usage: cmux help`
- `cmux docs --help` -> `Usage: cmux docs [settings|shortcuts|api|browser|agents|dock|sidebars]`
- `cmux docs` -> `Topics:`
- `cmux docs settings` -> `Config files:`
- `cmux docs dock` -> `dock: Custom right-sidebar terminal controls`
- `cmux settings --help` -> `Usage: cmux settings [open [target]|path|docs|<target>]`
- `cmux settings path` -> `Config files:`
- `cmux settings docs` -> `Config files:`
- `cmux config --help` -> `Usage: cmux config <doctor|check|validate|path|paths|docs|documentation|reload|get|set|sidebar-font-size|surface-tab-bar-font-size>`
- `cmux config path` -> `Config files:`
- `cmux config docs` -> `Config files:`
- `cmux welcome --help` -> `Usage: cmux welcome`
- `cmux welcome` -> `Super+Shift+P          Command palette`
- `cmux welcome` -> `Toggle Left Sidebar`
- `cmux welcome` -> `Toggle Right Sidebar`
- `cmux shortcuts --help` -> `Usage: cmux shortcuts`
- `cmux help shortcuts` -> `Usage: cmux shortcuts`
- `cmux help keyboard-shortcuts` -> `Usage: cmux shortcuts`
- `cmux help sidebar` -> `Usage: cmux sidebar <validate|reload|select|clear-state>`
- `cmux help dock` -> `Usage: cmux right-sidebar <command> [flags]`
- `cmux help sidebars` -> `Usage: cmux sidebar <validate|reload|select|clear-state>`
- `cmux help agents` -> `Usage: cmux hooks setup [agent]`
- `cmux disable-browser --help` -> `Usage: cmux disable-browser [--json]`
- `cmux enable-browser --help` -> `Usage: cmux enable-browser [--json]`
- `cmux browser-status --help` -> `Usage: cmux browser-status [--json]`
- `cmux agent-hibernation --help` -> `Usage: cmux agent-hibernation <on|off> [--json]`
- `cmux restore-session --help` -> `Usage: cmux restore-session`
- `cmux open --help` -> `Usage: cmux open <path-or-url>...`
- `cmux diff --help` -> `Usage: cmux diff [patch-file|-]`
- `cmux feedback --help` -> `Usage: cmux feedback`
- `cmux feed --help` -> `Usage: cmux feed list [--pending-only]`
- `cmux hooks --help` -> `Usage: cmux hooks setup [agent] [--agent <name>] [--yes|-y]`
- `cmux codex --help` -> `Usage: cmux codex <install-hooks|uninstall-hooks>`
- `cmux themes --help` -> `Usage: cmux themes`
- `cmux omo --help` -> `Usage: cmux omo [opencode-args...]`
- `cmux omx --help` -> `Usage: cmux omx [omx-args...]`
- `cmux omc --help` -> `Usage: cmux omc [omc-args...]`
- `cmux identify --help` -> `Usage: cmux identify`
- `cmux list-windows --help` -> `Usage: cmux list-windows`
- `cmux current-window --help` -> `Usage: cmux current-window`
- `cmux new-window --help` -> `Usage: cmux new-window`
- `cmux focus-window --help` -> `Usage: cmux focus-window --window <id|ref|index>`
- `cmux close-window --help` -> `Usage: cmux close-window --window <id|ref|index>`
- `cmux move-workspace-to-window --help` -> `Usage: cmux move-workspace-to-window`
- `cmux move-surface --help` -> `Usage: cmux move-surface`
- `cmux split-off --help` -> `Usage: cmux split-off`
- `cmux reorder-surface --help` -> `Usage: cmux reorder-surface`
- `cmux reorder-workspace --help` -> `Usage: cmux reorder-workspace`
- `cmux reorder-workspaces --help` -> `Usage: cmux reorder-workspaces`
- `cmux workspace-action --help` -> `Usage: cmux workspace-action --action <name>`
- `cmux move-tab-to-new-workspace --help` -> `Usage: cmux move-tab-to-new-workspace`
- `cmux tab-action --help` -> `Usage: cmux tab-action --action <name>`
- `cmux rename-tab --help` -> `Usage: cmux rename-tab`
- `cmux new-workspace --help` -> `Usage: cmux new-workspace`
- `cmux list-workspaces --help` -> `Usage: cmux list-workspaces`
- `cmux ssh --help` -> `Usage: cmux ssh <destination>`
- `cmux ssh --help` -> `--forward-agent`
- `cmux ssh-tmux --help` -> `Usage: cmux ssh-tmux <destination>`
- `cmux ssh-session-list --help` -> `Usage: cmux ssh-session-list [--workspace <id|ref|index>|--all]`
- `cmux ssh-session-attach --help` -> `Usage: cmux ssh-session-attach --session-id <id>`
- `cmux ssh-session-cleanup --help` -> `Usage: cmux ssh-session-cleanup`
- `cmux ssh-session-snapshot --help` -> `Usage: cmux ssh-session-snapshot`
- `cmux ssh-session-restore --help` -> `Usage: cmux ssh-session-restore`
- `cmux new-split --help` -> `Usage: cmux new-split`
- `cmux list-panes --help` -> `Usage: cmux list-panes`
- `cmux list-pane-surfaces --help` -> `Usage: cmux list-pane-surfaces`
- `cmux tree --help` -> `Usage: cmux tree`
- `cmux top --help` -> `Usage: cmux top`
- `cmux focus-pane --help` -> `Usage: cmux focus-pane`
- `cmux new-pane --help` -> `Usage: cmux new-pane`
- `cmux new-surface --help` -> `Usage: cmux new-surface`
- `cmux close-surface --help` -> `Usage: cmux close-surface`
- `cmux drag-surface-to-split --help` -> `Usage: cmux drag-surface-to-split`
- `cmux refresh-surfaces --help` -> `Usage: cmux refresh-surfaces`
- `cmux reload-config --help` -> `Usage: cmux reload-config`
- `cmux surface-health --help` -> `Usage: cmux surface-health`
- `cmux debug-terminals --help` -> `Usage: cmux debug-terminals`
- `cmux trigger-flash --help` -> `Usage: cmux trigger-flash`
- `cmux list-panels --help` -> `Usage: cmux list-panels`
- `cmux focus-panel --help` -> `Usage: cmux focus-panel`
- `cmux close-workspace --help` -> `Usage: cmux close-workspace`
- `cmux select-workspace --help` -> `Usage: cmux select-workspace`
- `cmux rename-workspace --help` -> `Usage: cmux rename-workspace`
- `cmux rename-window --help` -> `Usage: cmux rename-workspace`
- `cmux current-workspace --help` -> `Usage: cmux current-workspace`
- `cmux capture-pane --help` -> `Usage: cmux capture-pane`
- `cmux resize-pane --help` -> `Usage: cmux resize-pane`
- `cmux pipe-pane --help` -> `Usage: cmux pipe-pane`
- `cmux wait-for --help` -> `Usage: cmux wait-for`
- `cmux swap-pane --help` -> `Usage: cmux swap-pane`
- `cmux break-pane --help` -> `Usage: cmux break-pane`
- `cmux join-pane --help` -> `Usage: cmux join-pane`
- `cmux next-window --help` -> `Usage: cmux next-window`
- `cmux previous-window --help` -> `Usage: cmux previous-window`
- `cmux last-window --help` -> `Usage: cmux last-window`
- `cmux last-pane --help` -> `Usage: cmux last-pane`
- `cmux find-window --help` -> `Usage: cmux find-window`
- `cmux clear-history --help` -> `Usage: cmux clear-history`
- `cmux set-hook --help` -> `Usage: cmux set-hook`
- `cmux popup --help` -> `Usage: cmux popup`
- `cmux bind-key --help` -> `Usage: cmux bind-key`
- `cmux unbind-key --help` -> `Usage: cmux unbind-key`
- `cmux copy-mode --help` -> `Usage: cmux copy-mode`
- `cmux set-buffer --help` -> `Usage: cmux set-buffer`
- `cmux paste-buffer --help` -> `Usage: cmux paste-buffer`
- `cmux list-buffers --help` -> `Usage: cmux list-buffers`
- `cmux respawn-pane --help` -> `Usage: cmux respawn-pane`
- `cmux display-message --help` -> `Usage: cmux display-message`
- `cmux read-screen --help` -> `Usage: cmux read-screen`
- `cmux send --help` -> `Usage: cmux send`
- `cmux send-key --help` -> `Usage: cmux send-key`
- `cmux send-panel --help` -> `Usage: cmux send-panel`
- `cmux send-key-panel --help` -> `Usage: cmux send-key-panel`
- `cmux notify --help` -> `Usage: cmux notify`
- `cmux list-notifications --help` -> `Usage: cmux list-notifications`
- `cmux dismiss-notification --help` -> `Usage: cmux dismiss-notification`
- `cmux mark-notification-read --help` -> `Usage: cmux mark-notification-read`
- `cmux open-notification --help` -> `Usage: cmux open-notification`
- `cmux jump-to-unread --help` -> `Usage: cmux jump-to-unread`
- `cmux clear-notifications --help` -> `Usage: cmux clear-notifications`
- `cmux right-sidebar --help` -> `Usage: cmux right-sidebar <command> [flags]`
- `cmux set-status --help` -> `Usage: cmux set-status`
- `cmux clear-status --help` -> `Usage: cmux clear-status`
- `cmux list-status --help` -> `Usage: cmux list-status`
- `cmux set-progress --help` -> `Usage: cmux set-progress`
- `cmux clear-progress --help` -> `Usage: cmux clear-progress`
- `cmux log --help` -> `Usage: cmux log`
- `cmux clear-log --help` -> `Usage: cmux clear-log`
- `cmux list-log --help` -> `Usage: cmux list-log`
- `cmux sidebar-state --help` -> `Usage: cmux sidebar-state`
- `cmux set-app-focus --help` -> `Usage: cmux set-app-focus`
- `cmux simulate-app-active --help` -> `Usage: cmux simulate-app-active`
- `cmux claude-hook --help` -> `Usage: cmux claude-hook`
- `cmux browser --help` -> `Usage: cmux browser`
- `cmux open-browser --help` -> `Legacy alias for 'cmux browser open'`
- `cmux navigate --help` -> `Legacy alias for 'cmux browser navigate'`
- `cmux browser-back --help` -> `Legacy alias for 'cmux browser back'`
- `cmux browser-forward --help` -> `Legacy alias for 'cmux browser forward'`
- `cmux browser-reload --help` -> `Legacy alias for 'cmux browser reload'`
- `cmux get-url --help` -> `Legacy alias for 'cmux browser get-url'`
- `cmux focus-webview --help` -> `Legacy alias for 'cmux browser focus-webview'`
- `cmux is-webview-focused --help` -> `Legacy alias for 'cmux browser is-webview-focused'`
- `cmux markdown --help` -> `Usage: cmux markdown open <path>`
<!-- cli-contract-help-probes:end -->

## No-Socket Negative Help Probes

The following probes must not print help. They protect argument forwarding after
`--`, where a forwarded `--help` token belongs to the command payload.

<!-- cli-contract-negative-help-probes:start -->
- `cmux vm exec demo -- --help` !> `Usage: cmux vm`
<!-- cli-contract-negative-help-probes:end -->

## Current Help Caveats

These are current contracts to preserve until a follow-up PR intentionally
changes them:

- `cmux version --help` currently prints the version summary because `version`
  is handled before subcommand help dispatch.
- `cmux claude-teams --help` is handled by the command launcher, not by the
  pre-socket help dispatcher.
- `cmux codex-teams --help` is handled by the command launcher, not by the
  pre-socket help dispatcher.
- `cmux remote-daemon-status --help` currently prints status because the command
  runs before subcommand help dispatch.

## ArgumentParser Migration Sequence

1. Keep this contract file and `tests/test_cli_contract_help.py` green.
2. Add Swift ArgumentParser as a dependency without changing behavior.
3. Introduce a parse-only facade that maps ArgumentParser command structs onto
   existing `CMUXCLI` runner methods.
4. Move one command family at a time into small files, starting with no-socket
   commands (`version`, `themes`, hook installers), then socket commands, then
   browser and tmux compatibility.
5. After each family moves, run the contract probes plus targeted socket tests in
   GitHub Actions.
6. When all command families are migrated, remove the manual global parser and
   legacy helper code that no longer owns behavior.
