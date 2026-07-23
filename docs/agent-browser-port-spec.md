# Agent-Browser Port Spec

Last updated: February 13, 2026  
Source inventory snapshot: `vercel-labs/agent-browser` @ `03a8cb9`

This document tracks implemented behavior and remaining parity gaps for the cmux browser port.

## Goals

1. Provide an LLM-friendly browser automation API in cmux with stable handles.
2. Keep v1 CLI/socket behavior working while v2 reaches full parity.
3. Port `agent-browser` command surface (where meaningful for `WKWebView`).
4. Ensure move/reorder operations preserve `surface_id` identity.
5. Rebuild/port tests so both v1 and v2 suites pass before deprecating v1.

## Validation Status

As of February 12, 2026:
1. `./scripts/run-tests-v1.sh` passes on `cmux-vm`.
2. `./scripts/run-tests-v2.sh` passes on `cmux-vm`.
3. Browser parity suites passing in v2: `test_browser_api_comprehensive.py`, `test_browser_api_p0.py`, `test_browser_api_extended_families.py`, `test_browser_api_unsupported_matrix.py`, and `test_browser_cli_agent_port.py`.
4. Visual suite note: `tests_v2/test_visual_screenshots.py` reports D12 (`Nested: Close Top of T-shape`) as a known non-blocking VM failure when it reproduces (`VIEW_DETACHED`).

## Concepts (Canonical Terms)

1. `window`: native app window.
2. `workspace`: sidebar entry within a window (often called "tab" in UI).
3. `pane`: split region inside a workspace.
4. `surface`: tab within a pane (terminal or browser). This is the primary automation target.
5. `panel`: internal implementation term; CLI/API should prefer `surface`.

Terminology decision:
- Public v2 API and new CLI docs should standardize on `surface` and `pane`.
- Keep `--panel` as compatibility alias in CLI until v1 is retired.

## Self-Identify Requirement

`system.identify` is the canonical "where am I?" call for agents and should remain first-class.

Required response fields for agent workflows:
1. `focused.window_id`
2. `focused.workspace_id`
3. `focused.pane_id`
4. `focused.surface_id`
5. `caller` validation result when caller context is supplied

Recommended extension for browser workflows:
1. `focused.surface_type`
2. `focused.browser.url`
3. `focused.browser.title`
4. `focused.browser.loading`

## Agent-Browser Command Inventory

### Top-Level CLI Verbs (from `cli/src/commands.rs`)

1. `open|goto|navigate`
2. `back`
3. `forward`
4. `reload`
5. `click`
6. `dblclick`
7. `fill`
8. `type`
9. `hover`
10. `focus`
11. `check`
12. `uncheck`
13. `select`
14. `drag`
15. `upload`
16. `download`
17. `press|key`
18. `keydown`
19. `keyup`
20. `scroll`
21. `scrollintoview|scrollinto`
22. `wait`
23. `screenshot`
24. `pdf`
25. `snapshot`
26. `eval`
27. `close|quit|exit`
28. `connect`
29. `get`
30. `is`
31. `find`
32. `mouse`
33. `set`
34. `network`
35. `storage`
36. `cookies`
37. `tab`
38. `window`
39. `frame`
40. `dialog`
41. `trace`
42. `record`
43. `console`
44. `errors`
45. `highlight`
46. `state`
47. `tap`
48. `swipe`
49. `device`

### CLI Subcommands

1. `get`: `text|html|value|attr|url|title|count|box|styles`
2. `is`: `visible|enabled|checked`
3. `find`: `role|text|label|placeholder|alt|title|testid|first|last|nth`
4. `mouse`: `move|down|up|wheel`
5. `content|setcontent|innertext`
6. `setvalue|inserttext|selectall|clear|clipboard`
7. `bringtofront|multiselect|keyboard|pause`
8. `video`: `start|stop`
9. `set`: `viewport|device|geo|geolocation|offline|headers|credentials|auth|media`
10. `network`: `route|unroute|requests`
11. `storage`: `local|session` + `get|set|clear`
12. `cookies`: default get, plus `set|clear`
13. `tab`: default list, plus `new|list|close|<index>`
14. `window`: `new`
15. `frame`: `<selector>|main`
16. `dialog`: `accept|dismiss|respond`
17. `trace`: `start|stop`
18. `record`: `start|stop|restart`
19. `state`: `save|load`
20. `device`: `list`

### Global Flags

1. `--json`
2. `--full|-f`
3. `--headed`
4. `--debug`
5. `--session`
6. `--headers`
7. `--executable-path`
8. `--extension` (repeatable)
9. `--cdp`
10. `--profile`
11. `--state`
12. `--proxy`
13. `--proxy-bypass`
14. `--args`
15. `--user-agent`
16. `-p|--provider`
17. `--ignore-https-errors`
18. `--allow-file-access`
19. `--device`

### Protocol Actions in `src/protocol.ts`

Counts:
1. total actions: 125
2. directly emitted by CLI parser: 107
3. protocol-only (not directly emitted by CLI parser): 18

Protocol-only action names:
1. `addinitscript`
2. `addscript`
3. `addstyle`
4. `dispatch`
5. `evalhandle`
6. `expose`
7. `har_start`
8. `har_stop`
9. `input_keyboard`
10. `input_mouse`
11. `input_touch`
12. `locale`
13. `permissions`
14. `responsebody`
15. `screencast_start`
16. `screencast_stop`
17. `timezone`
18. `useragent`

## cmux Target API (v2)

### Already Present in cmux

1. `system.ping`
2. `system.capabilities`
3. `system.identify`
4. `window.list|current|focus|create|close`
5. `workspace.list|create|select|current|close|move_to_window`
6. `pane.list|focus|surfaces|create`
7. `surface.list|focus|split|create|close|drag_to_split|refresh|health|send_text|send_key|trigger_flash`
8. `browser.open_split|connect|window.new|window.create|navigate|back|forward|reload|url.get|focus_webview|is_webview_focused`
9. notification methods and debug/test methods

### New Browser Parity Method Families (Proposed)

P0 (core parity for daily automation):
1. `browser.snapshot`
2. `browser.eval`
3. `browser.wait`
4. `browser.click`
5. `browser.dblclick`
6. `browser.type`
7. `browser.fill`
8. `browser.press|keydown|keyup`
9. `browser.hover|focus`
10. `browser.check|uncheck`
11. `browser.select`
12. `browser.scroll|scroll_into_view`
13. `browser.drag|upload`
14. `browser.connect`
15. `browser.get.*` (`url|title|text|html|value|attr|count|box|styles`)
16. `browser.is.*` (`visible|enabled|checked`)
17. `browser.screenshot`
18. `browser.focus_webview` and `browser.is_webview_focused` (already present, keep)

P1 (important but not blocking initial parity):
1. `browser.find.*` locators (`role|text|label|placeholder|alt|title|testid|nth|first|last`)
2. `browser.frame.select`
3. `browser.frame.main`
4. `browser.dialog.accept|dismiss|respond`
5. `browser.download.wait`
6. `browser.tab.*` compatibility aliases mapped to cmux surfaces
7. `browser.console.list`
8. `browser.errors.list`
9. `browser.highlight`
10. `browser.state.save|load` (browser state in cmux context)

P2 (advanced parity / optional):
1. network interception/mocking equivalents (`route|unroute|requests|responsebody`; Linux currently stores full response bodies for browser-captured document requests and previews proxy-observed traffic)
2. emulation/settings (`offline|geolocation|useragent|locale|timezone|media|device|permissions` persist into the live WebKit document; mounted WebKit surfaces enforce offline mode with isolated network sessions and apply configured headers and Basic credentials to native document and subresource requests; `viewport` remains in the Linux state subset)
3. trace/screencast/HAR/recording equivalents (`browser.trace.*`, `browser.screencast.*`, HAR-style network capture, and frame-based `browser.video_*` / `browser.record_*` artifacts implemented in Linux; mounted GTK browser surfaces use periodic native WebKit snapshots with generation-safe model fallback)
4. script injection utilities (`addinitscript|addscript|addstyle|dispatch|expose` synchronize the Linux model and live document; `evalhandle` stores the mounted WebKitGTK result with model fallback)
5. raw input device injection (`input_mouse|input_keyboard|input_touch`)
6. content/editing utilities (`content|innertext|setcontent|setvalue|inserttext|selectall|clear|clipboard` implemented in the Linux fake DOM and clipboard subset)

### Object/Handle Semantics

1. stable handles: `window_id`, `workspace_id`, `pane_id`, `surface_id`
2. browser refs (`@e1`) are session-local and ephemeral
3. move/reorder must preserve `surface_id`
4. responses may include `index` for debugging/order, but requests should accept IDs

## CLI Spec (Proposed)

Primary form:
```bash
cmux browser --surface <surface-id> <agent-browser-style-command...>
```

Shorthand:
```bash
cmux browser <surface-id> <agent-browser-style-command...>
```

Agent discovery:
```bash
cmux identify
cmux capabilities
cmux browser identify --surface <surface-id>   # wrapper over system.identify + browser fields
```
Linux `system.capabilities` advertises the deterministic browser model with
`browser_model` and `browser_automation`; the legacy `browser_stub` flag is
kept only as a deprecated false value. In the GTK renderer, successful DOM,
input, scroll, script, and style actions are forwarded in sequence to the live
WebKit document and held until an in-progress navigation completes.

Flash:
```bash
cmux trigger-flash [--workspace <id>] [--surface <id>]
```

Compatibility:
1. Keep v1 commands.
2. Add v1->v2 shim for migrated browser/surface commands.
3. Keep `--panel` as alias for `--surface` during migration.

## Move/Reorder Spec (Required)

Required capabilities:
1. reorder surfaces within a pane
2. move surfaces between panes in same workspace
3. move surfaces across workspaces
4. move surfaces across windows
5. reorder workspaces within window

Proposed methods:
1. `surface.move` with `surface_id` + destination (`pane_id` or `workspace_id`/`window_id`) + placement (`before_surface_id|after_surface_id|start|end`)
2. `surface.reorder` with `surface_id` + sibling anchor (`before_surface_id|after_surface_id`)
3. `workspace.reorder` with `workspace_id` + anchor (`before_workspace_id|after_workspace_id`)
4. `workspace.reorder_many` with a `workspace_ids` final leading order inside pinned and unpinned groups. Unmentioned workspaces keep their relative order after the listed workspaces in the same group.

Hard invariant:
1. `surface_id` must remain unchanged after all move/reorder operations.

## Comprehensive TODO

### Phase 0: Contract + Routing

- [x] Lock method names/payload schemas for all new `browser.*` methods.
- [x] Add schema validation for each new method with strict error codes (`invalid_params`, `not_found`, `invalid_state`).
- [x] Add `browser` command group in `CLI/cmux.swift` that accepts agent-browser-style command grammar.
- [x] Add `--surface` mandatory targeting (with fallback from `system.identify` when explicitly desired).
- [x] Add consistent JSON output mode for all browser commands.
- [x] Implement short-ref allocator and resolver for `window/pane/workspace/surface` (`window:N`, `workspace:N`, `pane:N`, `surface:N`).
- [x] Add `--id-format refs|uuids|both` across relevant CLI commands (`--json` default refs, plain-text default refs).
- [x] Ensure browser placement APIs always return decision-rich metadata (resolved target pane, created splits, resulting handles).

### Phase 1: Core Browser Parity (P0)

- [x] Implement `browser.snapshot` (with refs).
  - Mounted WebKitGTK pages are traversed live into accessibility-oriented
    snapshot lines. The returned ephemeral `eN` refs are persisted into the
    surface selector table for subsequent actions; display-free runs retain
    deterministic model snapshots.
- [x] Implement `browser.eval`.
  - Linux socket calls release the app-state lock while GTK evaluates the
    source once in the mounted WebKit document, await returned promises, and
    return the resulting JSON-compatible value. Display-free runs retain the
    deterministic browser-model result.
- [x] Implement `browser.wait` variants: selector, timeout, URL pattern, load state, function, text.
- [x] Implement click family: `click`, `dblclick`, `hover`, `focus`.
- [x] Implement input family: `type`, `fill`, `press`, `keydown`, `keyup`.
- [x] Implement checkbox/select family: `check`, `uncheck`, `select`.
- [x] Implement drag/drop in the live WebKit document and native file-input selection through WebKitGTK's chooser request, with deterministic model fallback.
- [x] Implement `browser.connect` and `browser set <family>` compatibility routing.
- [x] Implement scrolling family: `scroll`, `scroll_into_view`.
- [x] Implement getters: text/html/value/attr/url/title/count/box/styles.
- [x] Implement state checks: visible/enabled/checked.
- [x] Implement screenshot and PDF artifacts.
  - Mounted WebKitGTK views produce native visible-region PNGs, with
    full-document capture through `--full-page`; display-free runs retain the
    deterministic model screenshot. Mounted views also print native PDFs;
    display-free runs retain the model-generated PDF.

### Phase 2: Locator + Session Parity (P1)

- [x] Implement `browser.find.role`.
- [x] Implement `browser.find.text`.
- [x] Implement `browser.find.label`.
- [x] Implement `browser.find.placeholder`.
- [x] Implement `browser.find.alt`.
- [x] Implement `browser.find.title`.
- [x] Implement `browser.find.testid`.
- [x] Implement `browser.find.nth|first|last`.
- [x] Implement frame context switching (`frame.select`, `frame.main`).
- [x] Implement dialog handling (`accept`, `dismiss`, optional prompt text).
- [x] Implement download waiting.
- [x] Implement console/error buffers and retrieval.
- [x] Implement highlight helper.
- [x] Implement browser state save/load format.

### Phase 3: Move/Reorder + Window/Workspace Integration

- [x] Implement `surface.move` with handle-based destination rules.
- [x] Implement `surface.reorder` within pane.
- [x] Implement cross-workspace surface moves.
- [x] Implement cross-window surface moves.
- [x] Implement `browser.window.new|create` as a browser-backed new-window alias.
- [x] Implement `workspace.reorder`.
- [x] Add CLI commands for tab/surface reordering and moving (`move-surface`, `reorder-surface`, `reorder-workspace`, `reorder-workspaces`).
- [x] Add response payloads that confirm final `window_id/workspace_id/pane_id/surface_id`.
- [x] Add explicit invariants tests for `surface_id` stability.

### Phase 4: Advanced/Optional Parity (P2)

- [ ] Evaluate feasibility of request interception/mocking in `WKWebView`; implement supported subset.
  - Linux WebKitGTK implements route/unroute by URL pattern, synthetic response
    bodies, request log inspection, HAR-style entries, and response-body reads.
  - Linux coverage: `browser_network_routes_and_request_log_are_observable`.
- [x] Add exact 1...4096 CSS-pixel viewport emulation for `WKWebView`; aspect-fit the page without changing pane layout, preserve focus, and restore native sizing with `browser viewport reset`.
- [ ] Add trace/recording equivalents where practical.
  - Linux implements trace start/stop JSON artifacts, HAR artifacts, periodic
    native WebKit screencast/video frames with deterministic model fallback,
    and `browser record` start/stop/restart compatibility aliases.
  - Linux coverage: `browser_trace_screencast_and_raw_input_are_observable`
    and `browser_legacy_aliases_cover_extended_agent_browser_surface`, plus the
    native-frame replacement and stale-generation AppState unit test.
- [x] Add Linux WebKitGTK emulation settings.
  - Supported subset: viewport, device presets, geolocation, offline/online,
    user agent, locale, timezone, media/color-scheme/reduced-motion, headers,
    credentials, and permissions.
  - Linux model coverage: `browser_offline_and_geolocation_are_observable`.
  - Linux live coverage: `webkit-runtime-smoke` verifies initial, immediate,
    and post-navigation locale, timezone, media, touch/DPR, online,
    geolocation, and permission overrides in WebKitGTK. It verifies configured
    headers on documents and subresources plus Basic HTTP authentication
    challenges through the embedded web-process extension. It also verifies that
    live `browser.eval` values resolve promises and execute the source once,
    and that DOM-backed reads and snapshots observe elements created only in
    the live document. Native snapshot refs use real CSS selectors for
    subsequent actions. `browser.evalhandle` uses the same native evaluation
    bridge while preserving stable handle identity and deterministic model
    fallback.
- [x] Add script/style injection helpers.
- [x] Document unsupported commands with explicit error `not_supported`.

### Phase 5: Compatibility + Migration

- [x] Add v1-to-v2 shim for migrated command families.
- [x] Keep existing v1 behavior unchanged while shim is active.
- [x] Document v1/v2 mapping table for all browser/topology commands.
  - See `docs/v2-api-migration.md` under "Browser/topology compatibility mapping".
- [x] Add deprecation warnings only after parity + test completion.
  - Linux emits stderr warnings for legacy browser CLI aliases outside JSON
    mode. The v1 line socket protocol remains unchanged while the shim is
    active.

### Phase 6: Docs + Examples

- [x] Update `docs/v2-api-migration.md` with browser parity status.
- [x] Add dedicated browser automation doc in `docs-site`.
  - Implemented at `web/app/[locale]/docs/browser-automation/page.tsx` and
    linked from `web/app/[locale]/components/docs-nav-items.ts`.
- [x] Add examples for LLM workflow: identify -> choose surface -> snapshot -> act -> verify.
  - See `docs/cli-contract.md` under "Browser automation workflow".
- [x] Add explicit "surface vs pane vs workspace vs window" section to CLI docs.
  - See `docs/cli-contract.md` under "Window, workspace, pane, and surface handles".

## Test Port Plan (Comprehensive)

### Port Targets from `agent-browser`

1. `src/browser.test.ts` -> ported/adapted into:
   - `tests_v2/test_browser_api_p0.py`
   - `tests_v2/test_browser_api_comprehensive.py`
   - `tests_v2/test_browser_api_unsupported_matrix.py`
2. `src/actions.test.ts` -> adapted negative coverage in `tests_v2/test_browser_api_comprehensive.py` (`invalid_params`, `not_found`, `timeout`).
3. `src/protocol.test.ts` -> adapted browser command/shape validation in `tests_v2/test_browser_api_unsupported_matrix.py` and existing `CLI/cmux.swift` command grammar checks.
4. `test/file-access.test.ts` and `test/launch-options.test.ts` -> partially applicable to `WKWebView`; currently tracked as follow-up parity work (not blocking current browser method coverage).
5. `src/daemon.test.ts`, `src/stream-server.test.ts`, `test/serverless.test.ts`, `src/ios-manager.test.ts` -> out-of-scope for cmux browser parity (different transport/runtime).

### Implemented cmux Browser Suites

1. `tests_v2/test_browser_api_p0.py`
2. `tests_v2/test_browser_api_comprehensive.py`
3. `tests_v2/test_browser_api_unsupported_matrix.py`
4. `tests_v2/test_browser_goto_split.py`
5. `tests_v2/test_browser_panel_stability.py`
6. `tests_v2/test_browser_custom_keybinds.py`

### Test Design Rules

1. Prefer deterministic local fixtures (embedded HTML or local HTTP server), not public websites.
2. Every command gets at least one positive and one negative test.
3. Every handle-accepting API gets tests for UUID target and index-compat shim target.
4. Every move/reorder test asserts `surface_id` stability pre/post operation.
5. Browser tests must verify behavior from both focused and unfocused webview states.
6. Self-identify tests must validate `focused` and `caller` fields.

### Migration Gate Criteria

1. New browser parity tests in `tests_v2/` pass.
2. Existing v2 regression suites still pass.
3. v1 suites still pass with shim active.
4. No regressions in existing window/workspace/surface workflows.

Planned verification commands at implementation completion:
1. `ssh cmux-vm 'cd /Users/cmux/cmux && ./scripts/run-tests-v2.sh'`
2. `ssh cmux-vm 'cd /Users/cmux/cmux && ./scripts/run-tests-v1.sh'`

## Decision Log (Locked - February 12, 2026)

1. `cmux browser tab ...` maps to browser `surface` tabs only (no separate workspace-level tab meaning inside `browser` namespace).
2. Default browser placement without explicit target is caller-relative: reuse the nearest right sibling pane; if none exists, split right from the caller pane.
3. Deeply nested layouts use local split ancestry: choose the nearest right sibling leaf in the caller's subtree path and avoid reshuffling unrelated panes.
4. Network parity target is full parity (not block-only phase).
5. Output shape is cmux-native overall, but `browser.snapshot` and selector `not_found` diagnostics intentionally mirror agent-browser semantics for agent usability.
6. ID model accepts UUIDs and short refs.
7. Short ref format uses full words and colon: `surface:N`, `pane:N`, `workspace:N`, `window:N`.
8. Short refs are global per daemon, monotonic, and never reused until daemon restart.
9. Plain-text CLI output defaults to short refs.
10. JSON output defaults to short refs (UUIDs available via `--id-format uuids|both`).
11. CLI supports `--id-format refs|uuids|both` for output shaping.
12. Browser create/move commands should expose enough placement/result metadata for agents to make deterministic follow-up decisions.
13. Reuse behavior is implicit by default (caller-relative right-pane reuse); explicit handles can still force deterministic targeting.
14. `browser fill` accepts empty text and treats it as a clear operation.
15. Mutating browser actions can opt into post-action verification snapshots via `snapshot_after` (`--snapshot-after` in CLI), returning `post_action_snapshot` (+ refs/title/url).
16. Legacy `new-pane`/`new-surface` plain output prefers short `surface:N` refs under default CLI ID formatting.

## Remaining Open Decisions

1. Unsupported command policy: strict `not_supported` errors vs best-effort fallback for commands that cannot be implemented on `WKWebView` with correct semantics.
2. Whether to expose protocol-only agent-browser actions in first public release of `cmux browser` or gate them behind a second rollout phase.
