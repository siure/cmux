# Ghostty Fork Changes (manaflow-ai/ghostty)

This repo uses a fork of Ghostty for local patches that aren't upstream yet.
When we change the fork, update this document and the parent submodule SHA.

## Fork update checklist

1) Make changes in `ghostty/`.
2) Commit and push to `manaflow-ai/ghostty`.
3) Update this file with the new change summary + conflict notes.
4) In the parent repo: `git add ghostty` and commit the submodule SHA.

## Current fork changes

Current cmux pinned fork head: `5697db81`, which adds the Darwin-only
`ghostty_surface_set_renderer_realized` C API (a `display_realized` renderer-thread
mailbox message that drives `displayUnrealized()`/`displayRealized()`) on top of
`34cbf180d`. cmux uses it to release an occluded terminal's GPU renderer
resources (Metal swap chain / IOSurface) while keeping its PTY alive, then
rebuild them on re-show. The API returns whether the message was enqueued (a
`.forever` `BlockingQueue.push` can still drop on a spurious wakeup while full) so
the embedder only advances its realize/unrealize mirror state on success. The push is `.instant` (non-blocking) so it never stalls the embedder's main thread waiting on the renderer. See
manaflow-ai/ghostty branch `feat-renderer-realized-offscreen` and
https://github.com/manaflow-ai/cmux/issues/4607. The prebuilt archive is
published at
https://github.com/manaflow-ai/ghostty/releases/tag/xcframework-5697db813b1b0fe14873093e9028f36513ddc187-crashsubdir-cmux-crash-v1
and pinned in `scripts/ghosttykit-checksums.txt`.

The prior head was refreshed from upstream `main` on May 1, 2026.
Earlier cmux pinned fork head: `34cbf180d`, merging the surface registry
serialization for https://github.com/manaflow-ai/cmux/issues/5458 (`e5c962a72`,
landed on cmux `main`) into the iOS render bounded-acquire line (`f78189ac1`)
combined with the cmd-click link refresh under mouse reporting (`df789cd4b`,
manaflow-ai/ghostty#71 and PRs #74 through #79) for
https://github.com/manaflow-ai/cmux/issues/5128. This keeps the previous head's
manual embedded IO patch in https://github.com/manaflow-ai/ghostty/pull/53,
the Metal renderer row rebuild guard for https://github.com/manaflow-ai/cmux/issues/3369,
the URL/path regex bound for spaced file paths followed by prose, and the iOS
render serial-queue bounded acquire fix from manaflow-ai/ghostty#80. This head
keeps the cmux theme picker hooks, exposes the manual surface IO needed by
libghostty iOS clients, bounds shaped glyph iteration during IME/preedit row
rebuilds, prevents Cmd-hover from highlighting normal sentence text after a file
path, and lets Cmd-click open links even while a mouse-reporting alt-screen TUI
(Claude Code, Codex) has grabbed the mouse.
It also supports Ctrl-N and Ctrl-P in the cmux theme picker.
The corresponding prebuilt archive is published at
https://github.com/manaflow-ai/ghostty/releases/tag/xcframework-34cbf180d8917b802d61d9929cfb493594f2ab52-crashsubdir-cmux-crash-v1
and pinned in `scripts/ghosttykit-checksums.txt`.

### Linux port staging (unpublished local checkout)

- Status: local cmux Linux-port work in `/home/akily/Project/sandbox/cmux-port/ghostty`;
  not yet published as a pinned fork head or prebuilt archive. The cmux
  `Build Linux Desktop App` workflow accepts the eventual published commit as
  its required immutable `ghostty_ref`, records that resolved SHA inside and
  alongside the archive, and is called by stable/nightly desktop workflows.
  Configure the published 40-character commit in the cmux repository variable
  `CMUX_LINUX_GHOSTTY_REF`; manual workflow dispatches can override it with
  `linux_ghostty_ref`.
- Local release validation on July 16, 2026:
  - `zig build -Dapp-runtime=none test` passes the full Ghostty unit suite.
  - The installed-header C/EGL smoke passes against both the checkout's
    `zig-out` tree and a clean `ReleaseSafe` install prefix, including
    surfaceless OpenGL rendering, multiple surfaces, manual I/O, ABI
    self-reporting, and teardown.
  - A clean optimized cmux build plus that `ReleaseSafe` prefix produces a
    28 MB relocatable `cmux-linux-x86_64.tar.gz`; source and relocated
    diagnostics report the Linux embedding and resources available.
  - Rebuilding the archive with the same inputs and `SOURCE_DATE_EPOCH`
    produces a byte-identical archive. The relocated and installed launchers
    both start the GTK/Ghostty app, accept socket control, and exit cleanly.
  - Publication is still required: these changes are not contained in the
    recorded Ghostty `HEAD`, and the current latest cmux GitHub release does
    not yet carry the Linux archive consumed by `cmux update`.
- Files:
  - `build.zig`
  - `include/ghostty.h`
  - `macos/Sources/Ghostty/Ghostty.App.swift`
  - `macos/Sources/Ghostty/Ghostty.Config.swift`
  - `macos/Sources/Ghostty/Ghostty.Inspector.swift`
  - `macos/Sources/Ghostty/Surface View/SurfaceView.swift`
  - `macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - `src/App.zig`
  - `src/Surface.zig`
  - `src/apprt.zig`
  - `src/apprt/embedded.zig`
  - `src/apprt/gtk/class/surface.zig`
  - `src/apprt/structs.zig`
  - `src/apprt/surface.zig`
  - `src/build/GhosttyResources.zig`
  - `src/build/SharedDeps.zig`
  - `src/config/CApi.zig`
  - `src/main_c.zig`
  - `src/os/resourcesdir.zig`
  - `src/renderer/Metal.zig`
  - `src/renderer/OpenGL.zig`
  - `src/renderer/generic.zig`
  - `src/renderer/image.zig`
  - `src/renderer/opengl/shaders.zig`
  - `src/terminal/osc.zig`
  - `src/terminal/osc/parsers.zig`
  - `src/terminal/osc/parsers/kitty_notification.zig`
- Summary:
  - Exposes the current Linux embedding ABI slice with `GHOSTTY_PLATFORM_LINUX`,
    host-owned OpenGL make-current/proc-address/done-current callbacks, and
    display realized/unrealized hooks for embedders that own the host surface.
  - Adds an explicit `ghostty_surface_set_visible` export for Linux embedding
    hosts, while keeping `ghostty_surface_set_occlusion` as a compatibility
    alias with the same visibility boolean semantics.
  - Exposes `ghostty_surface_set_renderer_realized(surface, bool)` on the local
    Linux embedding path as a compatibility alias for the existing cmux fork
    API; `true` maps to display realization and `false` maps to display
    unrealization.
  - Restores `ghostty_surface_select_cursor_cell` in the local Linux embedding
    ABI so cmux selection/copy-mode callers can select the active cursor cell
    through the same C boundary as macOS/iOS embedders.
  - Wires the Linux GTK Ghostty host to use that renderer-realized alias for
    initial surface realization and occlusion transitions, while final surface
    teardown uses the explicit display-unrealized C API so hidden terminal
    surfaces can release/rebuild renderer resources without killing the PTY.
  - Routes Ghostty app-level quit/close-all-windows, close-window, and
    close-tab actions through model-persisted embedded terminal
    close-confirmation state before allowing the GTK host to close windows or
    cmux surfaces, so sibling tabs/surfaces with live confirmation
    requirements are still surfaced from Ghostty shortcuts.
  - Exposes Ghostty start/end-search and match-count actions to the cmux GTK
    host, which uses `ghostty_surface_binding_action` for live search text and
    previous/next navigation while Ghostty owns search execution, highlighting,
    and selection state.
  - Lets the embedded runtime request host-thread redraws and routes Linux
    OpenGL renderer/inspector setup through the local embedder context used by
    the cmux GTK `GLArea` host.
  - Uses those redraw requests as the sole source of GTK `GLArea` paints while
    the host timer services only `ghostty_app_tick`. The app-thread draw
    requirement controls where requested frames run, not a continuous host
    render loop, avoiding unbounded idle DMA-BUF descriptor growth.
  - Treats embedded render actions as handled but skips the host redraw callback
    while a Linux surface display is unrealized, avoiding stale `GLArea` paints
    before `ghostty_surface_display_realized` or after display teardown.
  - Treats exported `ghostty_surface_draw` calls against an unrealized embedded
    display as successful no-ops, so stale GTK render callbacks after
    occlusion/display teardown do not surface as renderer failures.
  - Invalidates cached OpenGL presentation targets across Linux embedded
    display teardown/re-realization and forces the first frame after a rebuilt
    swap chain to redraw instead of replaying stale framebuffer handles.
  - Preserves the thread-local GLAD dispatch table when a later embedded Linux
    surface fails OpenGL context preparation, so one bad `GLArea` realization
    cannot invalidate already-running terminal surfaces on the GTK app thread.
  - Defers a transient Linux OpenGL-context-unavailable result during display
    realization and retries it from the first host draw, covering GTK map timing
    without converting a recoverable pre-map state into permanent teardown.
  - Transfers host-context ownership from the temporary OpenGL API value used
    during generic renderer construction into the renderer-owned value before
    releasing the context. Embedders that implement `done_current` can now
    unbind after surface creation without later realization mistaking the
    released context for a current one.
  - Uses live renderer resource state, not only the display-realized flag, to
    drive Linux display unrealize cleanup so early host teardown still releases
    GL resources allocated during surface construction.
  - Hardens embedder-provided inputs by sanitizing content scale, drawable
    sizes, pointer coordinates, scroll deltas, mouse pressure, and the optional
    surface font-size override, validating required runtime callbacks plus raw
    key, mouse, and split enum values, bounding the surface environment
    variable array advertised in `ghostty.h` and mirrored by the cmux GTK host,
    and rejecting invalid UTF-8 text, binding-action payloads, and surface
    creation option strings before they reach core surface or inspector state.
    Ghostty also rejects empty or `=`-containing environment variable names at
    the C surface config boundary so direct Linux embedders cannot pass invalid
    process environment keys through `ghostty_surface_new`.
  - Zero-initializes the platform union returned by
    `ghostty_surface_config_new`, keeping the C surface config initializer
    deterministic until a Linux embedder installs its host GL callbacks.
  - Completes the C header's `GHOSTTY_MOUSE_BUTTON_*` aliases for buttons
    four through eleven so Linux embedders can bind the full terminal mouse
    tracking range by name instead of relying on raw enum integers.
  - Includes the full input modifier and binding-flag constants plus the
    function/control key constants through F25, PrintScreen, ScrollLock, and
    Pause in the Linux embedding constants fingerprint, covering lock/right-side
    modifiers plus consumed/all/global/performable binding flags so GTK hosts
    can preserve Ghostty keybinding semantics across the C boundary. Synthesized
    keys without native keycodes can mark `ghostty_input_key_s.keycode` with
    `GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG` and pass a `GHOSTTY_KEY_*` value in
    the low bits.
  - Adds C-import ABI assertions for Linux platform callbacks, runtime
    callbacks, surface config, text/selection structs, config accessors, and
    the exported app/surface/inspector function signatures that cmux loads
    dynamically.
  - Restores the `ghostty_config_load_string(config, bytes, len)` C API in the
    Linux embedding checkout so embedders can layer generated config snippets
    without writing a temporary Ghostty config file.
  - Unrealizes embedded displays before freeing inspector objects so Linux
    OpenGL inspector backend cleanup runs while host context callbacks are
    still available.
  - Keeps the existing Swift embedded bridge aligned with the expanded C
    structs by populating the new runtime redraw callback field, copying
    startup-input and wait-after-command surface config fields, and freeing
    inherited Ghostty-owned surface config fields after copying them.
  - Runs `app-runtime=none` Zig tests against the embedded runtime and avoids
    GTK apprt imports unless the GTK runtime is selected, so libghostty C API
    tests exercise the Linux embedding path without requiring GTK modules.
  - Installs the Linux internal library with the runtime resources that the
    embedded app core resolves at startup, including terminfo,
    shell-integration files, optional bundled themes, and i18n locale files
    when gettext support is enabled.
  - Resolves embedded Linux resources from the loaded Ghostty shared object via
    `dladdr` before falling back to the host executable, so cmux can load
    `ghostty-internal.so` from Ghostty's install prefix even when the cmux
    process lives elsewhere, and exposes that resolved runtime path through
    `ghostty_resources_dir` for embedders that need to verify the library's
    actual resource view. The Linux resolver now requires the adjacent
    `share/ghostty` directory to exist before reporting a terminfo-only prefix
    as Ghostty's app resource directory. cmux renderer diagnostics now include
    the selected runtime resource source so staged installs can be
    distinguished from environment and checkout fallbacks, require the staged
    header's `GHOSTTY_EMBEDDING_ABI_VERSION`, `GHOSTTY_PLATFORM_LINUX` enum
    value, `GHOSTTY_SURFACE_MAX_ENV_VARS` bound, and physical-key marker
    constants to match cmux's Rust FFI boundary, require the loaded library's
    direct `ghostty_embedding_info` export and length-checked
    `ghostty_embedding_info_query` self-report to agree with each other and
    match the same ABI version, Linux platform, environment bound,
    app-thread draw contract, high-risk
    C struct layout sizes/alignments plus a field-offset layout fingerprint,
    the OpenGL renderer backend that the cmux GTK `GLArea` host requires, and
    the Ghostty enum/constant values cmux mirrors. Failed cmux dynamic
    loader probes now close partially opened `ghostty-internal` handles when a
    required symbol is missing. The cmux-loaded config API now includes
    `ghostty_config_get` alongside the load/finalize/diagnostic functions, so
    diagnostics, the Rust dynamic loader, and Ghostty's Linux export allowlist
    all enforce the same required symbol set. The local Linux embedding ABI is
    currently v13; the self-report covers runtime/surface config, Linux platform
    callbacks, input keys/triggers, target/action payloads, returned strings,
    diagnostics, surface sizes, text/selection readback, environment variables,
    clipboard content, and manual-I/O surface fields before cmux reports the
    full Ghostty backend available. ABI v11 adds process-free manual surfaces,
    the encoded-input write callback, and `ghostty_surface_process_output` for
    remote terminal transports such as Linux remote tmux. ABI v12 adds bounded
    initial terminal output parsed before child IO starts, allowing cmux to
    restore scrollback ahead of a fresh shell prompt without shell input replay.
    ABI v13 adds byte-bounded full-screen scrollback-tail readback whose memory
    use does not grow with Ghostty's retained history.
  - Makes the C API `ghostty_init` entry point idempotent after a successful
    initialization so repeated embedder probes in one process cannot
    reinitialize or leak Ghostty global state. It also exposes the argv pointer
    list and strings as read-only, normalizes empty embedder argument vectors
    to a synthetic program name, and rejects null non-empty vectors or null
    argv entries before touching global state. The Linux cmux loader now calls
    this idempotent entry point directly for each loaded `ghostty-internal`
    handle with an empty argv, instead of caching initialization success or
    failure process-wide, and cmux diagnostics require the read-only init argv
    declaration before reporting full Linux embedding support.
  - Propagates cmux socket/CLI config reload requests through a renderer
    snapshot generation so GTK Ghostty hosts update the embedded app and
    surface config through the libghostty config-update exports even when the
    reload was not initiated by a Ghostty keybind.
  - Ports the OSC 99 kitty desktop-notification parser into the Linux embedding
    checkout so Ghostty-owned PTYs store `p=title` chunks by notification id and
    emit the existing embedded desktop-notification action for simple
    notifications and `p=body` payloads. This keeps the full `ghostty` renderer
    path aligned with cmux's Rust parser-backed notification behavior.
  - Clears `ghostty_text_s` outputs before failed `ghostty_surface_read_selection`
    and `ghostty_surface_read_text` paths return, so embedders do not observe
    stale Ghostty-owned text pointers after a failed readback.
  - Allocates surface TTY-name, title, and working-directory strings returned
    through the C API with Ghostty's global C string allocator, keeping the
    `ghostty_string_free` ownership boundary independent from any future
    per-app allocator changes. cmux renderer diagnostics now require the
    staged header to declare those three surface metadata APIs as returning
    `ghostty_string_s` before reporting full Linux embedding support.
  - Clears the optional `ghostty_surface_key_is_binding` binding-flags output
    and provided `ghostty_surface_ime_point` output coordinates before early
    failure returns, so Linux embedders do not observe stale binding metadata or
    IME geometry when a lookup is rejected or no binding/point is available.
  - Clears the caller-provided bytes in `ghostty_embedding_info_query` before
    rejecting undersized ABI self-report buffers, so Linux embedders cannot
    accidentally interpret stale layout/constant fingerprints after a failed
    dynamic compatibility probe.
  - Validates both C enum fields in each `ghostty_selection_s` point before
    terminal text readback, so malformed Linux FFI selections fail cleanly
    instead of reaching exhaustive Zig enum switches with invalid values.
  - Consumes pending clipboard request pointers when
    `ghostty_surface_complete_clipboard_request` is called with null or invalid
    UTF-8 text, so rejected embedder clipboard completions do not leak the
    Ghostty-owned request allocation.
  - Honors Ghostty clipboard authorization in the cmux Linux GTK host. Unsafe
    pastes and OSC 52 reads/writes now show a modal confirmation with the exact
    text before completing the request or replacing either GTK clipboard;
    closing or denying the dialog uses an empty confirmed read, and callback
    generation plus surface identity checks reject stale dialog responses.
  - Tracks outstanding embedded clipboard requests per surface, rejects stale or
    foreign completion pointers before dereferencing request memory, and drains
    any still-pending requests during surface teardown.
  - Replaces allocator-address clipboard request identity with monotonic opaque
    tokens. A delayed completion can no longer alias a newer request after the
    allocator reuses an address; confirmation callbacks reissue the same token,
    and every callback issuance still authorizes exactly one completion.
  - Drains the core app surface list before each surface deinit during
    `ghostty_app_free`, so multi-surface Linux embedders cannot skip teardown
    entries while surfaces remove themselves from the same list.
  - Lets runtime surfaces provide an app-destroy hook, and uses it for
    embedded surfaces so `ghostty_app_free` releases any remaining
    Ghostty-owned surface allocations instead of only deinitializing their core
    surface state.
  - Serializes the core app surface registry and focused-surface pointer behind
    a mutex, preserving the prior cmux fork invariant that host-thread
    `ghostty_surface_new`/`ghostty_surface_free` calls cannot concurrently
    mutate the embedded app's surface list. Quit-timer callbacks stay outside
    the registry lock, config reload/global action fanout snapshots the surface
    registry before iterating, and `ghostty_app_free` surface drains no longer
    trigger a quit-timer action for surfaces that were already popped for
    teardown.
  - Hides Darwin-only display-id, quicklook, Metal inspector, and window blur
    declarations from Linux `ghostty.h` consumers while keeping the Linux
    OpenGL inspector declarations available in the same header. The Linux
    `ghostty-internal` export surface now follows the same split: cmux
    diagnostics reject Linux builds that still export those Darwin-only entry
    points.
  - Builds Ghostty's own SIMD C++ helper objects with hidden ELF visibility, so
    Linux `libghostty-internal.so` no longer publishes internal
    `ghostty_simd_*` symbols while keeping the explicit embedding C API visible
    for dynamic loaders. cmux renderer diagnostics now expose
    `embedding_internal_symbols_hidden` and reject stale Linux builds that still
    publish those helper symbols.
  - Adds a Linux ELF version script for `libghostty-internal.so`, reducing the
    dynamic export table to the exact embedded C API symbols cmux loads and
    keeping vendored dependency symbols such as FreeType, Harfbuzz, libxml2,
    shader, and C++ helper entry points out of the host process namespace. cmux
    renderer diagnostics now expose `embedding_unexpected_export_symbols_hidden`
    and reject stale Linux builds that still publish non-embedding symbols.
  - Emits the dedicated `ghostty-internal-static.pc` archive dependencies in
    `Libs`, making a normal pkg-config query a complete static link command;
    the shared module keeps those dependencies private for `--static` queries.
  - Declares zero-argument config and surface-config constructors with `(void)`
    in `ghostty.h`, keeping installed Linux headers valid under strict C
    prototype warnings as well as C++ compilation.
  - Rejects short-enum compiler modes in `ghostty.h` with portable compile-time
    assertions for every C enum in the embedding ABI, preventing silent Linux
    struct-layout and calling-convention mismatches with Zig and cmux.
  - Keeps `ghostty_embedding_info_query` genuinely prefix-compatible: smaller
    caller buffers receive every leading byte that fits while the return value
    still reports that the current full contract requires a larger buffer.
  - Deep-copies non-empty `ghostty_init` argument vectors for process-lifetime
    config reloads instead of retaining pointers into temporary host storage;
    failed initialization frees the copy so a later retry remains clean.
  - Orders the GTK host's Rust fields so `ghostty_app_free` and config cleanup
    run before callback userdata or the dynamically loaded library are dropped,
    preventing teardown callbacks from observing freed host state or code.
  - Shares one Ghostty app/config/library context across all GTK terminal
    surfaces on the application thread. Surface-targeted actions route through
    a synchronized raw-surface registry, app-targeted actions fall back to the
    focused surface, app ticks and snapshot config reloads are deduplicated,
    and the shared context is released after the final terminal host drops.
  - Resolves desktop OpenGL entry points through `libOpenGL`, `libGL`, and
    `libEGL`, including `glXGetProcAddressARB` and `eglGetProcAddress`
    fallbacks. This avoids relying on a nonexistent public
    `epoxy_get_proc_address` symbol when a GTK `GLArea` initializes Ghostty.
  - Leaves the optional Linux `done_current` callback unset for the GTK host,
    allowing `GLArea` to retain ownership of its current context through
    Ghostty's initial display-realization render pass.
  - Runs the GTK application with a sanitized argument vector and updates
    realized Ghostty widgets in place while the snapshot's structural layout
    is unchanged. cmux renderer flags are therefore not reparsed as
    `GApplication` options, and periodic preview/title refreshes no longer
    unrealize and recreate active OpenGL terminal surfaces.
  - Rotates GTK callback registry tokens whenever an embedded surface is
    released or realization fails, so delayed actions and clipboard completions
    from an old GL surface cannot target a replacement that reuses its address.
  - Removes embedded surfaces from the core app registry before the runtime
    `ghostty_surface_free` path deinitializes and destroys them, keeping
    registry ownership and object teardown distinct for GTK host widget
    destruction and app shutdown.
  - Atomically marks an embedded surface as destroying before
    `ghostty_surface_free` enters the app registry. Reentrant or overlapping
    free attempts while the object is still allocated are rejected, and the
    same acquire/release gate keeps late surface and inspector C API calls from
    entering teardown state.
  - Captures the embedded runtime app before host-initiated surface teardown
    and passes that app into the post-deinit quit-timer finish path, so Linux
    embedders no longer rely on reading a deinitialized surface just to notify
    the host that the last terminal closed.
  - Rejects late inspector C API calls when the inspector belongs to an
    embedded surface that has entered teardown, matching the existing surface
    handle guards so stale GTK inspector size/focus/input/OpenGL callbacks
    return false instead of mutating teardown state.
  - Gives inspectors an independent atomic teardown gate because hosts can
    close them while their surface remains live. Ghostty keeps a destroying
    inspector attached until OpenGL backend and ImGui cleanup finish, so host
    context callbacks cannot recursively free or mutate it and cannot create a
    replacement against process-global backend state that is still shutting
    down.
  - Reference-tracks dcimgui OpenGL backends across ImGui contexts. Closing one
    embedded or GTK inspector restores imgl3w while other inspectors remain
    live, and only the final backend shutdown clears the process-wide function
    table; simultaneous inspectors can therefore close in any order without a
    null OpenGL function call. Context-loss abandonment releases the same
    tracking slot without issuing invalid per-context GL cleanup.
  - Defers Ghostty `quit` and `close_all_windows` GTK destruction to the next
    main-loop idle turn. These actions can originate inside an embedded host
    tick; synchronous window destruction would emit `GLArea::unrealize` while
    that host is still mutably borrowed and abort on a nested `RefCell` borrow.
  - Keeps `ghostty_surface_inherited_config_free` available while its surface
    is actively tearing down. Delayed host callbacks can release the
    Ghostty-owned inherited working-directory string instead of losing that
    allocation when the general surface mutation guard closes.
  - Completes the internal C header's IPC action wrapper
    `ghostty_ipc_action_s` and asserts its layout against
    `apprt.ipc.Action.C`, so Linux embedders that parse the whole embedding
    header see the same action tag/payload shape Ghostty's Zig IPC layer uses.
    The IPC target class string is exposed as read-only in the header, and the
    C-import ABI tests now cover the IPC target union, target struct offsets,
    and new-window action payload layout. The IPC new-window argument vector is
    also exposed as a read-only pointer list of read-only strings, matching
    cmux's Rust FFI mirror and preventing embedders from treating Ghostty-owned
    argv storage as mutable. cmux renderer diagnostics now require that
    source-level header contract before reporting full Linux embedding support.
  - Bumps the local Linux embedding ABI to v10 and includes IPC target/action
    sizes, alignments, field-offset layout fingerprint inputs, and IPC enum
    constants in `ghostty_embedding_info_s`, so cmux rejects stale
    `ghostty-internal` libraries that do not cover the whole exposed
    embedding header contract.
  - Includes `GHOSTTY_CLIPBOARD_REQUEST_*` enum values in the v10 constants
    fingerprint so cmux rejects libraries that disagree on the confirm-read
    clipboard callback request tag contract.
  - Marks `ghostty_surface_config_s.env_vars` as a read-only host-owned array
    in the C ABI and mirrors that constness in the Rust FFI. cmux renderer
    diagnostics expose `embedding_header_surface_env_vars_const` and return
    `surface_env_vars_not_const` for stale mutable headers before reporting
    full Linux embedding support.
  - Adds an explicit C-import assertion for the
    `ghostty_runtime_config_s.redraw_surface_cb` field and makes cmux renderer
    diagnostics expose `embedding_header_has_redraw_surface_callback`, returning
    `missing_redraw_surface_callback` for staged headers that cannot schedule
    host-thread Linux GL redraws.
  - Propagates the embedder action callback's handled result through
    `ghostty_app_open_config`, `ghostty_app_reload_config`, and the exported
    split create/focus/resize/equalize/zoom helpers. Hosts can now distinguish
    accepted actions from rejected ones instead of receiving a successful C
    API result for both cases.
  - Hardens the Linux development installer so the resolved full Ghostty or
    `ghostty-vt` artifact is checked through the freshly built
    `cmux app --renderer ... --script 'renderer diagnostics ...'` path before
    `linux-app.env` is written. This catches stale headers, incompatible
    libraries, missing runtime resources, Darwin-only/internal helper or other
    non-embedding dynamic exports from `libghostty-internal.so`, and missing
    `libghostty-vt` exports at install time instead of deferring the failure to
    desktop launch.
  - Marks the embedded runtime app as destroying before `ghostty_app_free`
    drains its surfaces. Reentrant host callbacks can no longer create a new
    surface in the app being torn down, and a second overlapping free request
    is rejected while the runtime object is still alive. All other app-level C
    API calls also reject the destroying handle, preventing reentrant ticks,
    configuration updates, key events, and state queries from entering fields
    while app teardown deinitializes them.
  - Adds a Ghostty-owned Linux C integration probe to the existing
    `build-linux-libghostty` CI job. The job now consumes the staged
    `ReleaseSafe` DSO through its installed pkg-config metadata, compiles the
    public header with strict C warnings, verifies ABI self-report and prefix
    queries, resolves adjacent runtime resources, exercises config parsing, and
    creates, ticks, and tears down an embedded app with the required Linux
    callback table. The probe also creates a real surfaceless Mesa/EGL OpenGL
    context and terminal surface, verifies PTY sizing and host userdata, covers
    deferred context realization, draws a frame, and releases the display and
    surface through only the installed public C API.

### 1) macOS display link restart on display changes

- Commit: `05cf31b38` (macos: restart display link after display ID change)
- Files:
  - `src/renderer/generic.zig`
- Summary:
  - Restarts the CVDisplayLink when `setMacOSDisplayID` updates the current CGDisplay.
  - Prevents a rare state where vsync is "running" but no callbacks arrive, which can look like a frozen surface until focus/occlusion changes.

### 2) macOS resize stale-frame mitigation

The resize commits are grouped by feature because they touch the same stale-frame replay path and
tend to conflict together during rebases.

- Commits:
  - `a3588ac53` (macos: reduce transient blank/scaled frames during resize)
  - `9ba54a68c` (macos: keep top-left gravity for stale-frame replay)
- Files:
  - `pkg/macos/animation.zig`
  - `src/Surface.zig`
  - `src/apprt/embedded.zig`
  - `src/renderer/Metal.zig`
  - `src/renderer/generic.zig`
  - `src/renderer/metal/IOSurfaceLayer.zig`
- Summary:
  - Replays the last rendered frame during resize and keeps its geometry anchored correctly.
  - Reduces transient blank or scaled frames while a macOS window is being resized.

### 3) OSC 99 (kitty) notification parser

- Commits:
  - `2033ffebc` (Add OSC 99 notification parser)
  - `a75615992` (Fix OSC 99 parser for upstream API changes)
- Files:
  - `src/terminal/osc.zig`
  - `src/terminal/osc/parsers.zig`
  - `src/terminal/osc/parsers/kitty_notification.zig`
- Summary:
  - Adds a parser for kitty OSC 99 notifications and wires it into the OSC dispatcher.
  - Adapts the parser to upstream's newer capture API so the cmux OSC 99 hook survives the March 30 upstream sync.

### 4) cmux theme picker helper hooks

- Commits:
  - `66ff6ec4d` (Add cmux theme picker helper hooks)
  - `aa650937d` (Fix cmux theme picker preview writes)
  - `89d3612c9` (Improve cmux theme picker footer contrast)
  - `0dc979889` (Respect system theme in cmux picker)
  - `d9e0ab512` (Skip theme detection in cmux picker)
  - `042cbaaab` (Match Ghostty theme picker startup)
  - `eb34bcdd6` (Harden cmux theme override writes)
  - `04ec69173` (Apply highlighted cmux theme on Enter)
  - `4265d3428` (Apply cmux theme from picker search)
  - `176bd550f` (Add ctrl navigation to cmux theme picker)
- Files:
  - `build.zig`
  - `src/cli/list_themes.zig`
  - `src/main_ghostty.zig`
- Summary:
  - Adds a `zig build cli-helper` step so cmux can bundle Ghostty's CLI helper binary on macOS.
  - Lets `+list-themes` switch into a cmux-managed mode via env vars, writing the cmux theme override file and posting the existing cmux reload notification for live app-wide preview.
  - Keeps the preview UI readable in light mode, matches upstream picker startup behavior, and hardens writes to the cmux-managed theme override file.
  - Restores Enter as the cmux apply action by writing the currently highlighted theme before the picker exits.
  - Applies the highlighted search result when Enter is pressed from search mode in cmux-managed picker sessions.
  - Supports Ctrl-N and Ctrl-P as one-row down/up navigation in cmux-managed picker sessions.

### 5) Color scheme mode 2031 reporting

- Commits:
  - `2be58ee0e` (Fix DECRPM mode 2031 reporting wrong color scheme)
  - `74709c29b` (Send initial color scheme report when mode 2031 is enabled)
- Files:
  - `src/Surface.zig`
  - `src/termio/stream_handler.zig`
- Summary:
  - Keeps Ghostty's mode 2031 color-scheme response aligned with the surface's actual conditional state after config reloads.
  - Sends the initial DSR 997 report as soon as mode 2031 is enabled, which cmux relies on for immediate color-scheme awareness.

### 6) Keyboard copy mode selection C API

- Commit: `0b231db94` (Re-export cmux selection APIs removed from upstream)
- Files:
  - `include/ghostty.h`
  - `src/Surface.zig`
  - `src/apprt/embedded.zig`
- Summary:
  - Restores `ghostty_surface_select_cursor_cell` and `ghostty_surface_clear_selection`.
  - Keeps cmux keyboard copy mode working against the refreshed Ghostty base after upstream removed those exports.

### 7) macos-background-from-layer config flag

- Commits:
  - `ae3cc5d29` (Restore macOS layer background hook)
  - `aa28e1bcb` (Add macos-background-from-layer config flag)
  - `1a01b36d9` (Skip fullscreen bg draw call in layer-background mode)
  - `82e20630b` (Preserve bg images in layer background mode)
  - `465a9a621` (Restore bg-image alpha in layer background mode)
- Files:
  - `src/config/Config.zig`
  - `src/renderer/generic.zig`
- Summary:
  - Adds a `macos-background-from-layer` bool config (default false).
  - When true, sets `bg_color[3] = 0` in the per-frame uniform update so the Metal renderer skips the full-screen background fill.
  - Allows the host app to provide the terminal background via `CALayer.backgroundColor` for instant coverage during view resizes, avoiding alpha double-stacking.
  - Replays the layer-background restore on top of the refreshed Ghostty base so cmux keeps the resize-coverage fix after the upstream sync.

### 8) TerminalStream kitty graphics APC handling

- Commit: `a8e92c9c5` (terminal: add APC handler to stream_terminal)
- Files:
  - `src/terminal/stream_terminal.zig`
- Summary:
  - Wires `.apc_start`, `.apc_put`, and `.apc_end` through the shared APC parser in `TerminalStream`.
  - Restores kitty graphics execution and APC OK/error replies for the non-termio stream path used by cmux/libghostty integrations.

### 9) Config load string C API

- Commit: `f7880c473` (Add config load string C API)
- Files:
  - `include/ghostty.h`
  - `src/config/CApi.zig`
  - `src/config/Config.zig`
- Summary:
  - Adds a C API for loading Ghostty config from an in-memory string.
  - Lets cmux parse generated or override config without materializing a separate config file first.

### 10) Manual embedded IO for libghostty iOS

- Commit: `22fa801f8` (Expose manual embedded IO for iOS)
- PR: https://github.com/manaflow-ai/ghostty/pull/53
- Files:
  - `include/ghostty.h`
  - `src/Surface.zig`
  - `src/apprt/embedded.zig`
  - `src/input.zig`
  - `src/input/text.zig`
  - `src/renderer/Thread.zig`
  - `src/termio.zig`
  - `src/termio/Manual.zig`
  - `src/termio/Termio.zig`
  - `src/termio/backend.zig`
- Summary:
  - Exposes `GHOSTTY_SURFACE_IO_MANUAL`, `io_write_cb`, `ghostty_surface_process_output`,
    `ghostty_surface_text_input`, and `ghostty_surface_render_now` through the embedded C API.
  - Wires the existing manual termio backend into embedded surfaces without taking stale
    xcframework or build-system changes from the old iOS branch.
  - Keeps manual surface writes inline so iOS typing does not wait on the termio thread wakeup path.
  - Comments each fork-only API/runtime hook with its upstream-removal condition.
  - Checked upstream `ghostty-org/ghostty` `4dcb09ada` on May 1, 2026. It does not expose
    equivalent libghostty surface IO selection, write callback, text-input callback,
    render-now C API, or output C API. Upstream already has internal
    `Termio.processOutput`, so prefer an upstream C bridge if one lands.

### 11) Metal renderer preedit row rebuild guard

- Commits:
  - `70b95dada` (Expose unsafe preedit catch-up in renderer rows)
  - `fe972c095` (Bound renderer preedit catch-up to shaped glyphs)
- Files:
  - `src/renderer/generic.zig`
- Summary:
  - Adds a regression test for the row-rebuild path where IME/preedit covers the
    only shaped glyph in a row and the remaining terminal cells are empty.
  - Bounds the shaped glyph cursor before reading from the shaped-cell slice, so
    `GenericRenderer(Metal).rebuildRow` no longer assumes terminal cells and
    shaped glyph cells have one-to-one cardinality.
  - The first commit intentionally preserves the panic so cmux can keep the
    required failing-test-then-fix history for https://github.com/manaflow-ai/cmux/issues/3369.

### 12) URL/path regex bounds for spaced file paths

- Commits:
  - `6e10706a7` (test: cover spaced file path link bounds)
  - `6eed7af92` (fix: bound spaced file path links)
  - `ff6e1260d` (fix: handle dotted spaced path prefixes)
- Files:
  - `src/config/url.zig`
- Summary:
  - Adds coverage for a path with spaces ending in `.mp4` followed by a normal sentence.
  - Routes dotted paths with spaced directory names through the stricter dotted-path branch.
  - Keeps single-space path components such as `Recovered Screen Recordings` while preserving
    the existing double-space stop case.
  - Trims trailing sentence punctuation when more text follows, without breaking dotted paths
    that end at end-of-line.
  - Preserves versioned or dotted path components before the first space, such as
    `/tmp/v1.2 captures/video.mp4`.

### 13) Cmd-click opens links under mouse reporting (alt-screen TUIs)

- Commits (manaflow-ai/ghostty#71, by @doronpr):
  - `1c7613c95` (fix: open terminal links on cmd-click even when mouse reporting is active)
  - `55d154a97` (fix: gate link refresh on effective mouse-reporting state)
- Follow-up commits (manaflow-ai/ghostty#74):
  - `354e3626b` (fix: suppress mouse reporting for the full cmd-clicked link click)
  - `d1dbbec9b` (fix: key cmd-click link suppression on the modifier, not over_link)
- Follow-up commit (manaflow-ai/ghostty#75):
  - `76ead3eae` (fix: also suppress motion reports during a cmd-clicked link drag)
- Follow-up commit (manaflow-ai/ghostty#76):
  - `f24195271` (fix: scope cmd-click link suppression to left button; clear stale hover)
- Follow-up commits (manaflow-ai/ghostty#77):
  - `5998abddd` (fix: latch cmd-click link suppression for the click lifecycle)
  - `59fb750c0` (fix: clear link-click latch unconditionally on left release)
- Follow-up commit (manaflow-ai/ghostty#78):
  - `9f014e98b` (fix: open latched link on release with press-time chord; defer-clear latch)
- Follow-up commit (manaflow-ai/ghostty#79):
  - `df789cd4b` (fix: only open a latched link click that started on a link)
- Files:
  - `src/Surface.zig`
- Summary:
  - Link hover/highlight state was refreshed in `keyCallback`/`cursorPosCallback`
    only when mouse reporting was off, or shift was releasing the mouse from
    capture. Holding the ctrl/super link-activation modifier was not considered,
    so under a mouse-grabbing alt-screen TUI (Claude Code, Codex) `over_link`
    stayed `false`, the link-click branch in `mouseButtonCallback` was skipped,
    and the Cmd-click was reported to the program — which made cmux fall back to
    the OS default browser instead of honoring the configured link-open target.
  - Adds a shared `mouseLinkRefreshAllowed` gate (pure logic in
    `mouseLinkRefreshAllowedState`) that also allows local link handling when the
    ctrl/super modifier is held, using the effective mouse-reporting state
    (`isMouseReporting()`), matching iTerm2 and macOS Terminal. Fixes
    https://github.com/manaflow-ai/cmux/issues/5128.
  - Follow-up (#74): `mouseButtonCallback` ran the link-open path only on
    release, while the mouse-report path ran for both press and release and only
    broke out for the shift-release case — so a Cmd-click over a link still
    reported the *press* to the program and leaked a half-click to mouse-grabbing
    TUIs. The follow-up breaks out of the report path whenever the ctrl/super
    link chord is held (keyed on the modifier, like the shift-release path, so
    cursor jitter can't leak a press or a release), suppressing the whole click.
  - Follow-up (#75): `cursorPosCallback` still emitted `.motion` reports while a
    button was held during the chord, so a drag during link activation leaked
    button-motion. Mirrors the shift "grab override" for the ctrl/super chord in
    the motion path. Net: the link chord suppresses the whole left click+drag —
    press, release, and motion — consistently.
  - Follow-up (#76): scopes that suppression to the left button (ctrl/super
    right/middle clicks still reach the program, since link activation is
    left-only), and clears a stale link highlight/cursor when the chord is
    released through `cursorPosCallback`'s mods (refresh when `over_link` is set,
    mirroring `keyCallback`'s existing reset branch).
  - Follow-up (#77): latches the suppression decision at left-button press
    (`mouse.link_click_active`) and applies it through the release, instead of
    re-checking the live modifier each event — so releasing ctrl/super before the
    mouse button can't leak the release as a half-click. Ties suppression to the
    click lifecycle (press/drag/release), fully closing the half-click class. The
    latch is cleared unconditionally on left release (independent of
    mouse-reporting state) so it can't go stale.
  - Follow-up (#78): unifies the open and suppression decisions. `linkAtPos`
    uses the latched chord while a click is active and the release attempts
    `processLinks` whenever latched, so releasing the modifier before the button
    still opens the link (instead of swallowing the click); the latch is cleared
    via a function-level `defer` so the early-return link-open path resets it.
  - Follow-up (#79): only opens the latched click when it started on a link
    (`link_press_over_link`), so a chord drag that began off a link and released
    over one is swallowed rather than opening a link the press never targeted.
  - Known limitation (noted by review): the bypass matches the default
    `ctrlOrSuper` chord, which is exactly what both link kinds already require to
    activate (OSC 8 `linkAtPos` and the default url `hover_mods = ctrlOrSuper`); a
    user who reconfigures `link.highlight.hover_mods` to a non-default chord would
    not get the under-mouse-reporting bypass. Out of scope for #5128.

### 14) Embedded surface registry serialization

- Commits:
  - `c9b61a8af` (Add surface registry mutation serialization test)
  - `e5c962a72` (Serialize Ghostty surface registry mutations)
- Files:
  - `src/App.zig`
- Summary:
  - Adds a deterministic regression test for concurrent embedded runtime
    surface registry mutation.
  - Protects the native `App.surfaces` list and `focused_surface` pointer with
    one mutex so an off-main `ghostty_surface_free` cannot overlap the main
    actor `ghostty_surface_new` insertion path.
  - Keeps callbacks such as the quit timer outside the registry mutex to avoid
    re-entrancy through the embedder.
- Conflict notes:
  - Any upstream change to `App.addSurface`, `App.deleteSurface`,
    `App.focusedSurface`, or the embedded surface close path should preserve
    serialization of registry/focus mutation across create and free.

The current cmux pin is the merged head `34cbf180d`, which merges the surface
registry serialization (`e5c962a72`, section 14, landed on cmux `main` via
branch `issue-5458-surface-registry-lock`) into the Cmd-click link fix line
(`df789cd4b`, section 13) on top of the iOS render bounded-acquire pin
(`f78189ac1`). It is reachable from `manaflow-ai/ghostty` through branch
`issue-5128-alt-screen-link-open`. Published
`xcframework-34cbf180d8917b802d61d9929cfb493594f2ab52-crashsubdir-cmux-crash-v1`
and pinned its archive checksum in `scripts/ghosttykit-checksums.txt`. The
release and checksum pin must be regenerated whenever this commit changes, even
for comment-only amends, because the release tag is keyed by the Ghostty commit
SHA.

## Upstreamed fork changes

### cursor-click-to-move respects OSC 133 click-to-move

- Was local in the fork as `10a585754`.
- Landed upstream as `bb646926f`, so it is no longer carried as a fork-only patch.

### zsh prompt redraw follow-ups

- Were local in the fork as `8ade43ce5`, `0cf559581`, `312c7b23a`, and `404a3f175`.
- Dropped during the March 30, 2026 rebase because newer Ghostty prompt-marking changes on the refreshed base superseded these fork-only zsh redraw patches, so cmux no longer carries them separately.

### initial focus seeding and DECSET 1004 startup behavior

- Was local in the fork as `c19c82bfd`.
- Dropped from the current pinned fork head when cmux removed the corresponding
  app-side initial focus seed and went back to post-create focus sync.

## Merge conflict notes

These files change frequently upstream; be careful when rebasing the fork:

- April 28, 2026, upstream merge:
  - Merged upstream `659019666` into `465a9a621` without textual conflicts.
  - Verified with `CMUX_GHOSTTYKIT_NO_PREBUILT=1 ./scripts/ensure-ghosttykit.sh`.
  - Verified cmux with `./scripts/reload.sh --tag gtyup`.
  - Published `xcframework-d3117e03ea19665bc83a28f7e0428c63937e6140` and pinned
    its archive checksum in `scripts/ghosttykit-checksums.txt`.
  - Merged `d3117e03e` into fork `main` with https://github.com/manaflow-ai/ghostty/pull/48.
  - Package GhosttyKit archives with `COPYFILE_DISABLE=1`; the archive validator rejects
    macOS AppleDouble entries such as `._GhosttyKit.xcframework`.

- April 28, 2026, theme picker restore:
  - Reapplied the section 4 cmux picker hooks on top of `d3117e03e`.
  - Enter in cmux mode must call the same selection-apply path used by keyboard/mouse navigation
    before setting the picker outcome to apply.
  - Verified with `zig build cli-helper -Dapp-runtime=none -Demit-macos-app=false -Demit-xcframework=false -Doptimize=ReleaseFast`.
  - Verified Enter writes `theme = light:0x96f,dark:0x96f` in a PTY temp-config run.
  - Published `xcframework-04ec69173f8f5ac5a2568afca0faf8e4a74b2dc2` and pinned
    its archive checksum in `scripts/ghosttykit-checksums.txt`.

- April 30, 2026, theme picker search Enter:
  - Search-mode Enter in cmux mode must apply the current filtered selection and exit with
    outcome `apply`.
  - Escape still leaves search mode, and stock Ghostty search Enter still returns to normal mode.
  - Verified with `./scripts/reload.sh --tag thmenter`.
  - Published `xcframework-4265d34282ce2023c27da851c454dabe6cdc76ce` and pinned
    its archive checksum in `scripts/ghosttykit-checksums.txt`.

- May 1, 2026, manual embedded IO for libghostty iOS:
  - Added only the manual embedded IO API/runtime pieces on top of fork `main` `495316732`.
  - Avoided old iOS branch `.gitignore`, package, and xcframework build-system changes.
  - Checked upstream `ghostty-org/ghostty` `4dcb09ada`; no equivalent public libghostty
    surface IO API exists yet.
  - Added comments to the fork-only hunks stating that they should be deleted in favor of
    an upstream implementation when one exists.
  - Verified with `zig build test`.
  - Verified the universal macOS plus iOS xcframework path with
    `CMUX_GHOSTTYKIT_NO_PREBUILT=1 ./scripts/ensure-ghosttykit.sh`.
  - Published `xcframework-22fa801f88f96fa842e54ecce6c34a5d36003d19` and pinned
    its archive checksum in `scripts/ghosttykit-checksums.txt`.
  - Merged https://github.com/manaflow-ai/ghostty/pull/53 so the submodule SHA is
    reachable from fork `main`.

- `src/terminal/osc.zig`
  - OSC dispatch logic moves often. Re-check the integration points for the OSC 99 parser and keep
    the newer `capture`/`captureTrailing()` API usage intact.

- `src/terminal/osc/parsers.zig`
  - Ensure `kitty_notification` stays imported after upstream parser reorganizations.

- `src/cli/list_themes.zig`
  - cmux now relies on the upstream picker UI plus local env-driven hooks for live preview and restore.
    If upstream reorganizes the preview loop or key handling, re-check the cmux mode path and keep the
    stock Ghostty behavior unchanged when the cmux env vars are absent.
  - The April 28, 2026 restore requires Enter in cmux mode to call the same selection-apply path
    used by keyboard/mouse navigation before setting the picker outcome to apply.
  - The April 30, 2026 follow-up requires the same behavior from search mode, while preserving Escape
    as the search cancel path.

- `build.zig`
  - Upstream's new wasm/libghostty work touched the same build graph. Keep the cmux-only `cli-helper`
    step wired in without regressing the upstream `lib-vt` or wasm build paths.

- `src/main_ghostty.zig`
  - The April 28, 2026 restore only conflicted on stdout writer API usage. Keep the current
    `std.fs.File.stdout().writer(&buf)` API plus explicit flush.

- `include/ghostty.h`, `src/Surface.zig`, `src/apprt/embedded.zig`
  - Upstream removed cmux-used selection exports. Preserve the re-exported
    `ghostty_surface_select_cursor_cell` and `ghostty_surface_clear_selection` functions.

- `src/renderer/generic.zig`
  - The `macos-background-from-layer` check sits next to the glass-style check in `updateFrame`.
    If upstream refactors the bg_color uniform update or the glass conditional, re-check that both
    paths still zero out `bg_color[3]` correctly.

- `src/Surface.zig`, `src/apprt/embedded.zig`, `macos/Sources/Ghostty/Surface View/SurfaceView.swift`
  - The initial `focused` plumbing has to stay aligned across the C config, embedded runtime surface,
    and macOS wrapper. If upstream refactors surface creation or post-create focus sync, re-check that
    background panes can start unfocused without synthesizing a focus-loss transition during creation.

- `src/Surface.zig` (modifier tracking)
  - `modsChanged` and the key callback's link-highlight gate must compare binding mods against
    binding mods (stored mouse mods are binding-only). cmux sends sided modifier bits on key
    events for `macos-option-as-alt = left|right`; comparing raw mods re-dirties the screen and
    re-runs the link refresh on every event while a sided or lock modifier is held. If upstream
    refactors modifier tracking, keep the binding-normalized comparison.

- `src/termio/stream_handler.zig`
  - Keep DECSET 1004 enablement side-effect free. xterm-compatible focus reporting should only emit
    `CSI I` / `CSI O` on actual focus transitions, not immediately when the mode is enabled.

- `src/terminal/stream_terminal.zig`
  - Keep the APC handler wired into `.apc_start`, `.apc_put`, `.apc_end`, and preserve the
    `apcEnd()` response path so kitty graphics still reach `Terminal.kittyGraphics()` and reply via
    `write_pty`.

If you resolve a conflict, update this doc with what changed.
