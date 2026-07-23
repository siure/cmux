# V2 Socket API + Test Migration

This doc tracks the migration from the existing v1 line protocol (space-delimited commands) to a v2 JSON protocol intended for LLM agents.

## Goals

- Add a **v2 JSON socket protocol** (handle-based: `window_id`, `workspace_id`, `pane_id`, `surface_id`).
- Keep **v1 fully working** until v2 reaches feature parity.
- Re-implement the existing automated test suite to use **v2**.
- Run both suites:
  - v1 tests (existing `tests/`)
  - v2 tests (new `tests_v2/`)

## Non-Goals (for initial parity)

- Removing v1.
- Changing existing v1 behaviors/output formats.

## Status

- [x] Implement v2 request/response envelope (JSON, newline-delimited)
- [x] Implement v2 core methods (workspaces/surfaces/panes/input/notifications/browser)
- [x] Implement v2 multi-window methods (windows + cross-window workspace moves)
- [x] Add `surface.trigger_flash` (agent-visible highlight for a surface)
- [x] Implement v2 debug/test methods (simulate typing, render stats, screenshots, etc.)
- [x] Add `tests_v2/` using v2 client
- [x] Add runners for v1 + v2 suites on the VM (`./scripts/run-tests-v1.sh`, `./scripts/run-tests-v2.sh`)
- [x] Verify v1 suite passes (VM)
- [x] Verify v2 suite passes (VM)

Notes:
- A close-top nested split sequence (T-shape) could leave terminal views detached from the window until the user switched workspaces.
  Fix: a debounced post-close reattach pass (see `Sources/Workspace.swift`, `Sources/Panels/TerminalPanel.swift`).

## V2 Protocol Sketch

Each request is one JSON object per line:

```json
{"id":"1","method":"workspace.list","params":{}}
```

Each response is one JSON object per line:

```json
{"id":"1","ok":true,"result":{...}}
```

Errors:

```json
{"id":"1","ok":false,"error":{"code":"not_found","message":"workspace not found"}}
```

Notes:
- `id` is echoed back when present (string or number).
- v2 methods should accept **IDs**; v2 responses may include ephemeral `index` fields for ordering/debugging, but IDs are the stable handles.

## Method Parity Checklist (v1 -> v2)

Windows:
- [x] list_windows -> `window.list`
- [x] current_window -> `window.current`
- [x] focus_window -> `window.focus`
- [x] new_window -> `window.create`
- [x] close_window -> `window.close`
- [x] move_workspace_to_window -> `workspace.move_to_window`

Workspaces:
- [x] list_workspaces -> `workspace.list`
- [x] new_workspace -> `workspace.create`
- [x] select_workspace -> `workspace.select`
- [x] current_workspace -> `workspace.current`
- [x] close_workspace -> `workspace.close`
- [x] reorder-workspace -> `workspace.reorder`
- [x] reorder-workspaces -> `workspace.reorder_many`

Surfaces / Splits:
- [x] list_surfaces -> `surface.list`
- [x] focus_surface / focus_surface_by_panel -> `surface.focus`
- [x] new_split -> `surface.split`
- [x] new_surface -> `surface.create`
- [x] close_surface -> `surface.close`
- [x] drag_surface_to_split -> `surface.drag_to_split`
- [x] refresh_surfaces -> `surface.refresh`
- [x] surface_health -> `surface.health`
- [x] trigger_flash -> `surface.trigger_flash` (new in v2)

Panes:
- [x] list_panes -> `pane.list`
- [x] focus_pane -> `pane.focus`
- [x] list_pane_surfaces -> `pane.surfaces`
- [x] new_pane -> `pane.create`

Input:
- [x] send / send_surface -> `surface.send_text`
- [x] send_key / send_key_surface -> `surface.send_key`

Notifications:
- [x] notify -> `notification.create`
- [x] notify_surface -> `notification.create_for_surface`
- [x] notify_target -> `notification.create_for_target`
- [x] list_notifications -> `notification.list`
- [x] clear_notifications -> `notification.clear`
- [x] set_app_focus -> `app.focus_override.set`
- [x] simulate_app_active -> `app.simulate_active`

Browser:
- [x] open_browser -> `browser.open_split`
- [x] navigate -> `browser.navigate`
- [x] browser_back -> `browser.back`
- [x] browser_forward -> `browser.forward`
- [x] browser_reload -> `browser.reload`
- [x] get_url -> `browser.url.get`
- [x] focus_webview -> `browser.focus_webview`
- [x] is_webview_focused -> `browser.is_webview_focused`

Compatibility note: the Linux CLI emits stderr deprecation warnings for these
legacy browser aliases outside JSON mode. Global `--json`, per-command `--json`,
and the v1 line socket protocol keep their existing output shape.

Debug / Test-only:
- [x] set_shortcut -> `debug.shortcut.set`
- [x] simulate_shortcut -> `debug.shortcut.simulate`
- [x] simulate_type -> `debug.type`
- [x] activate_app -> `debug.app.activate`
- [x] is_terminal_focused -> `debug.terminal.is_focused`
- [x] read_terminal_text -> `debug.terminal.read_text`
- [x] render_stats -> `debug.terminal.render_stats`
- [x] layout_debug -> `debug.layout`
- [x] bonsplit_underflow_count/reset -> `debug.bonsplit_underflow.*`
- [x] empty_panel_count/reset -> `debug.empty_panel.*`
- [x] focus_notification -> `debug.notification.focus`
- [x] flash_count/reset -> `debug.flash.*`
- [x] panel_snapshot/panel_snapshot_reset -> `debug.panel_snapshot.*`
- [x] screenshot -> `debug.window.screenshot`

## Browser/topology compatibility mapping

The v1 line protocol and legacy CLI wrappers remain active while v2 is the
canonical agent-facing API. New automation should call the v2 method directly,
but these compatibility mappings must keep working until v1 is explicitly
retired.

Topology and surface mapping:

| v1 / legacy CLI surface | v2 method | Compatibility route |
| --- | --- | --- |
| `list_windows` | `window.list` | v1 socket shim |
| `current_window` | `window.current` | v1 socket shim |
| `focus_window <window>` | `window.focus` | v1 socket shim |
| `new_window` | `window.create` | v1 socket shim |
| `close_window <window>` | `window.close` | v1 socket shim |
| `move_workspace_to_window <workspace> <window>` | `workspace.move_to_window` | v1 socket shim |
| `list_workspaces` | `workspace.list` | v1 socket shim |
| `new_workspace` | `workspace.create` | v1 socket shim and `cmux new-workspace` |
| `select_workspace <workspace>` | `workspace.select` | v1 socket shim |
| `current_workspace` | `workspace.current` | v1 socket shim |
| `close_workspace <workspace>` | `workspace.close` | v1 socket shim |
| `reorder-workspace` | `workspace.reorder` | `cmux reorder-workspace` |
| `reorder-workspaces` | `workspace.reorder_many` | `cmux reorder-workspaces` |
| `list_surfaces` | `surface.list` | v1 socket shim |
| `focus_surface`, `focus_surface_by_panel`, `focus-panel`, `focus-surface` | `surface.focus` | v1 socket shim and CLI wrapper |
| `new_split` | `surface.split` | v1 socket shim and `cmux new-split` |
| `new_surface` | `surface.create` | v1 socket shim and `cmux new-surface` |
| `close_surface` | `surface.close` | v1 socket shim and `cmux close-surface` |
| `drag_surface_to_split` | `surface.drag_to_split` / `surface.split_off` | v1 socket shim and `cmux drag-surface-to-split` |
| `move-surface` | `surface.move` | `cmux move-surface` |
| `reorder-surface` | `surface.reorder` | `cmux reorder-surface` |
| `refresh_surfaces` | `surface.refresh` | v1 socket shim and `cmux refresh-surfaces` |
| `surface_health` | `surface.health` | v1 socket shim and `cmux surface-health` |
| `list_panes` | `pane.list` | v1 socket shim and `cmux list-panes` |
| `focus_pane` | `pane.focus` | v1 socket shim and `cmux focus-pane` |
| `list_pane_surfaces` | `pane.surfaces` | v1 socket shim and `cmux list-pane-surfaces` |
| `new_pane` | `pane.create` / `surface.split` | v1 socket shim and `cmux new-pane` |
| `send`, `send_surface`, `send-panel` | `surface.send_text` | v1 socket shim and CLI wrapper |
| `send_key`, `send_key_surface`, `send-key-panel` | `surface.send_key` | v1 socket shim and CLI wrapper |
| `read_screen`, `capture-pane`, `read-screen` | `surface.read_text` | v1 socket shim and CLI wrapper |

Browser mapping:

| v1 / legacy CLI surface | v2 method | Compatibility route |
| --- | --- | --- |
| `open_browser <url>`, `open-browser`, `open_browser` | `browser.open_split` | v1 socket shim and CLI wrapper |
| `navigate <surface> <url>` | `browser.navigate` | v1 socket shim and CLI wrapper |
| `browser_back`, `browser-back` | `browser.back` | v1 socket shim and CLI wrapper |
| `browser_forward`, `browser-forward` | `browser.forward` | v1 socket shim and CLI wrapper |
| `browser_reload`, `browser-reload` | `browser.reload` | v1 socket shim and CLI wrapper |
| `get_url`, `get-url` | `browser.url.get` | v1 socket shim and CLI wrapper |
| `focus_webview`, `focus-webview` | `browser.focus_webview` | v1 socket shim and CLI wrapper |
| `is_webview_focused`, `is-webview-focused` | `browser.is_webview_focused` | v1 socket shim and CLI wrapper |
| `browser window new`, `browser window create` | `browser.window.new` / `browser.window.create` | v2 browser CLI |
| `browser profiles ...`, `browser profile ...` | `browser.profiles.*` | v2 browser CLI |
| `browser set <family> ...` | `browser.<family>.set` | v2 browser CLI compatibility umbrella |
| `diff`, command-palette Open Diff Viewer | `diff.open` / `browser.open_split` | v2 Linux app method for palette/socket clients; CLI still accepts patch/stdin/git-source options |
| `browser network route/unroute/requests/responsebody` | `browser.network.*` | v2 browser CLI |
| `browser trace`, `browser har`, `browser screencast`, `browser video`, `browser record` | `browser.trace.*`, `browser.har.*`, `browser.screencast.*`, `browser.video.*`, `browser.record.*` | v2 browser CLI |
| `browser input`, `browser mouse`, `browser tap`, `browser swipe`, `browser keyboard` | `browser.input_*` / input alias methods | v2 browser CLI |

Linux regression coverage for this table lives in
`legacy_v1_pane_surface_and_browser_commands_match_macos_contract`,
`browser_connect_and_set_cli_aliases_match_agent_browser_contract`,
`browser_profiles_and_cookie_import_match_macos_contract`,
`browser_trace_screencast_and_raw_input_are_observable`, and
`browser_legacy_aliases_cover_extended_agent_browser_surface`.

## Test Migration

v1 suite stays in `tests/`.

v2 suite lives in `tests_v2/` and should:
- use a v2 JSON client (`tests_v2/cmux.py`)
- avoid depending on v1 text output formats

VM runners:
- v1: `ssh cmux-vm 'cd /Users/cmux/cmux && ./scripts/run-tests-v1.sh'`
- v2: `ssh cmux-vm 'cd /Users/cmux/cmux && ./scripts/run-tests-v2.sh'`

## Open Questions

- Should v2 require explicit `workspace_id`/`surface_id` for all operations, or default to the currently-focused ones?
- For move/reorder operations (future): what are the policies for empty workspaces/windows?
