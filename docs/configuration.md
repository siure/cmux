# cmux.json settings

Global app preferences live in `~/.config/cmux/cmux.json`.

## `mobile.artifactFolderAccess`

Controls which files and folders cmux on iOS may browse after a chat references a directory or a directory path appears in a terminal.

```json
{
  "mobile": {
    "artifactFolderAccess": "subtree"
  }
}
```

- `subtree` (default): authorize the referenced directory and its full subtree.
- `oneLevel`: preserve the previous rule, authorizing only immediate children and listing only the referenced directory itself.

Authorization compares canonical paths after resolving symlinks. A symlink inside an authorized folder cannot grant access to a target outside that folder.

## `paneBorderColor` and `activePaneBorderColor`

Customize split-workspace pane boundaries controlled by cmux.

```json
{
  "paneBorderColor": "#6B7280",
  "activePaneBorderColor": "#3B82F6"
}
```

- `paneBorderColor`: overrides the divider color between cmux panes in split workspaces.
- `activePaneBorderColor`: draws a border around the focused cmux pane in split workspaces.

Both settings accept 6-digit hex colors (`#RRGGBB`). Omit a key, or set it to `null`, to keep the built-in appearance. These settings apply to cmux's multi-surface pane layout, not Ghostty's internal splits; Ghostty settings such as `split-divider-color` still only affect splits inside one Ghostty instance.

## Workspace lifecycle

The shared app settings control where interactive workspace creation inserts a
new ungrouped workspace, whether it starts in the active workspace's current
directory, and what the close shortcut does to a workspace's final surface:

```json
{
  "app": {
    "newWorkspacePlacement": "afterCurrent",
    "workspaceInheritWorkingDirectory": true,
    "keepWorkspaceOpenWhenClosingLastSurface": false
  }
}
```

`newWorkspacePlacement` accepts `top`, `afterCurrent`, or `end`. `top` keeps the
pinned workspace prefix intact, `afterCurrent` inserts beside the active
workspace, and `end` appends. Linux applies these settings to the new-workspace
shortcut and GTK toolbar button; group creation continues to use
`workspaceGroups.newWorkspacePlacement`.

`keepWorkspaceOpenWhenClosingLastSurface` defaults to `false`, so the close
shortcut closes the workspace when it targets the final surface. When `true`,
Linux replaces that surface with a new terminal and keeps the workspace open.
The pane tab close button remains an explicit close and still closes the
workspace. Socket `surface.close` rejects the final surface unless the caller
supplies an internal lifecycle source. A Ghostty runtime close keeps the
workspace open with a replacement terminal, while terminal child exit closes
the workspace.

The settings are editable from **Settings > General** and through
`settings.app.status` / `settings.app.set`. Direct `workspace.create` RPC calls
remain append-only unless they explicitly pass `placement`; callers may pass
`inherit_working_directory: true` when they want desktop-style cwd inheritance.

## Terminal interaction

Linux and macOS share the terminal scrollbar and copy-on-selection settings:

```json
{
  "terminal": {
    "showScrollBar": true,
    "copyOnSelect": false,
    "autoResumeAgentSessions": true
  }
}
```

`showScrollBar` displays an overlay scrollbar only when Ghostty reports
scrollback; alternate-screen TUIs with no additional scrollback keep the
rightmost terminal column unobstructed. The user's Ghostty `scrollbar = never`
setting remains authoritative.

`copyOnSelect` maps to Ghostty's `copy-on-select = clipboard` behavior when
enabled and `copy-on-select = false` when disabled. Copying occurs when a
selection gesture is committed rather than on every pointer movement. These
values are editable from **Settings > Terminal** and through
`settings.terminal.status` / `settings.terminal.set`.

`autoResumeAgentSessions` restarts agent-hook and native agent sessions that
were running when the snapshot was saved. When disabled, Linux preserves the
window, pane, scrollback, binding, and native agent state but opens the restored
session idle. Re-enabling the setting before the next relaunch still resumes
that saved running session.

Non-agent terminal commands never repeat merely because they were the
surface's original startup command. Automatic restore requires a signed
`terminal.resumeCommands` prefix approval. `manual` opens an ordinary shell,
`prompt` asks in the GTK app before sending the command, and `auto` starts the
approved command immediately. Approvals bind the command prefix, working
directory, and exact saved environment. Manage them in **Settings > Terminal >
Resume Commands**, with `surface.resume.approve`, or with
`settings.terminal.resume.*`.

## `customSidebars.renderer`

Controls where interpreted Swift-style custom sidebar source is evaluated.

```json
{
  "customSidebars": {
    "renderer": "remote"
  }
}
```

Values:

- `inProcess`: default. Evaluate in the cmux process for the lowest latency.
- `remote`: evaluate through a bounded helper process. A crash, error, or
  timeout keeps the last good sidebar document visible.

The Linux worker uses a versioned JSON protocol, bounded request and response
sizes, and a two-second timeout. The resulting neutral tree is rendered as
native GTK, including parameterized actions and `Reorderable` drag behavior.
The setting can also be changed in **Settings > Custom Sidebars** and is read
without restarting the app.

## Beta features

Experimental desktop features use the same flat keys as macOS:

```json
{
  "rightSidebar.beta.feed.enabled": false,
  "rightSidebar.beta.dock.enabled": false,
  "extensions.beta.enabled": false,
  "customSidebars.beta.enabled": true,
  "remoteTmux.beta.enabled": false
}
```

Feed and Dock are omitted from the right-sidebar mode switcher while disabled.
If a restored window names a disabled mode, Linux falls back to Files.
Disabling Custom Sidebars hides discovered providers and temporarily uses the
built-in Workspaces sidebar without deleting the saved provider selection.
Remote tmux socket and CLI entry points return `unavailable` while its beta is
off.

Linux runs sidebar extensions as isolated executable providers. Each extension
lives under `~/.config/cmux/extensions/<id>/`, declares the same API version,
read scopes, and action scopes as the macOS extension manifest, and adds a
Linux `entrypoint`:

```json
{
  "id": "dev.example.sidebar",
  "displayName": "Example Sidebar",
  "minimumApiVersion": {"major": 2, "minor": 0},
  "readScopes": ["workspaceMetadata", "notifications"],
  "actionScopes": ["selectWorkspace", "navigateWorkspace"],
  "entrypoint": "extension"
}
```

The executable receives one bounded JSON render request on stdin and returns:

```json
{
  "protocolVersion": 1,
  "document": {
    "version": 1,
    "root": {"type": "text", "text": "Extension ready"}
  }
}
```

The document uses the same native node format as custom sidebars. Buttons may
emit only typed extension actions, for example:

```json
{
  "type": "button",
  "title": "Next workspace",
  "action": {
    "type": "extension",
    "params": {"action": "selectNextWorkspace"}
  }
}
```

Requested access must be approved before any workspace data is sent. Changing
the manifest invalidates the prior grant. Actions are checked against the
current grant again when clicked. On Linux the production host uses Bubblewrap,
mounts only the extension directory and system runtime read-only, disables
network access, clears the inherited environment, provides a private `/tmp`,
and enforces bounded input, output, and execution time. Install the
`bubblewrap` package before enabling extensions.

Socket clients can inspect and manage the host through
`sidebar.extension.list`, `sidebar.extension.select`,
`sidebar.extension.grant`, `sidebar.extension.revoke`, and
`sidebar.extension.reload`.

These settings are editable from **Settings > Beta Features** and through
`settings.beta_features.status` / `settings.beta_features.set`.

## `app.windowTitleTemplate`

Opt-in template for the native window title. On macOS this sets
`NSWindow.title`; on Linux it sets the GTK window title exposed to desktop
environments and tiling window managers. Leave it unset or set it to an empty
string to keep the default behavior, where the title follows the active
workspace title or current directory.

```json
{
  "app": {
    "windowTitleTemplate": "[cmux:{windowToken}] {activeWorkspace}"
  }
}
```

Supported placeholders:

- `{windowId}`: the persisted per-window UUID.
- `{windowToken}`: the first 8 characters of the persisted window UUID.
- `{activeWorkspace}`: the active workspace title, falling back to the default title when the workspace title is blank.
- `{activeDirectory}`: the active workspace's current directory.
- `{defaultTitle}`: the title cmux would have used without a template.
- `{appName}`: `cmux`.

For tiling window managers such as AeroSpace, yabai, Sway, Hyprland, or i3,
match on the stable token in the title. For example, the template above gives
each restored window a title containing `[cmux:abcd1234]`, so a rule can match
`\\[cmux:abcd1234\\]`. The token is stable across relaunches for restored
windows because it comes from the persisted window UUID.

## `app.confirmQuit`

Controls when cmux asks before quitting:

- `always`: show the quit confirmation on Cmd+Q, Ctrl+Q, or app quit.
- `dirty-only`: show it only when a workspace has a terminal or panel that reports close confirmation is needed.
- `never`: quit immediately.

Default: `always` for stable and nightly builds. DEV builds always behave as `never`, regardless of the file setting, so tagged development builds can be replaced without a full-screen quit dialog.

The older boolean `app.warnBeforeQuit` still works as a fallback when `app.confirmQuit` is not set. `true` maps to `always`; `false` maps to `never`.

## Tab close safety

The app-level tab close settings control when cmux warns before closing a
terminal and whether pane tab strips expose their close button:

```json
{
  "app": {
    "warnBeforeClosingTab": true,
    "warnBeforeClosingTabXButton": false,
    "hideTabCloseButton": false
  }
}
```

`warnBeforeClosingTab` asks before an interactive shortcut or Ghostty runtime
action closes a terminal that reports close confirmation is needed.
`warnBeforeClosingTabXButton` always asks when the pane tab close button is
used, even when the terminal is clean. `hideTabCloseButton` removes that button
from pane tab strips; keyboard shortcuts and command-palette close actions
remain available.

Linux presents these requests as native GTK dialogs and keeps the requested
surface, window, or application open until the user accepts. The settings are
editable from **Settings > General** and through `settings.app.status` /
`settings.app.set`.

## Sidebar details

Workspace-row content is configured under `sidebar`:

```json
{
  "sidebar": {
    "hideAllDetails": false,
    "wrapWorkspaceTitles": true,
    "showWorkspaceDescription": true,
    "branchLayout": "inline",
    "stackBranchDirectory": false,
    "pathLastSegmentOnly": true,
    "showNotificationMessage": true,
    "showBranchDirectory": true,
    "watchGitStatus": true,
    "showSSH": true,
    "showPorts": true,
    "showLog": true,
    "showProgress": true,
    "showCustomMetadata": true,
    "rightMaxWidth": 420
  },
  "sidebarAppearance": {
    "matchTerminalBackground": true
  }
}
```

Linux applies these values to native GTK workspace rows. Rows can show custom
descriptions, git branch and dirty state, working directory, latest
notification, SSH destination, openable listening ports, sidebar progress,
status entries, metadata blocks, and the latest log entry. `branchLayout`
accepts `vertical` or `inline`; the older boolean `branchVerticalLayout` is
still read for compatibility. `hideAllDetails` leaves only workspace titles
and unread markers. `rightMaxWidth` is bounded to 276-4096 points.

The native **Settings > Sidebar** page edits the same layered configuration.
Socket clients use `settings.sidebar.status` and `settings.sidebar.set`.

## Workspace colors

Workspace color rendering and the reusable named palette are configured under
`workspaceColors`:

```json
{
  "workspaceColors": {
    "indicatorStyle": "leftRail",
    "selectionColor": "#2E5E76",
    "notificationBadgeColor": "#72C7E7",
    "colors": {
      "Blue": "#1565C0",
      "Production": "#AD1457"
    }
  }
}
```

`indicatorStyle` accepts `leftRail` or `solidFill`; legacy macOS aliases are
normalized. `selectionColor` and `notificationBadgeColor` accept six-digit hex
colors or `null`. `colors` is the complete named palette: removing a key
removes it from workspace color pickers, and adding a key makes it available.

Linux applies these settings to native GTK workspace rows, inherited workspace
group colors, selected backgrounds, and unread markers. Workspace context menus
show the effective palette as color swatches. **Settings > Workspace Colors**
can edit the same values, add/remove custom names, and restore the 16 built-in
colors. Socket clients use `settings.workspace_colors.status`,
`settings.workspace_colors.set`, `settings.workspace_colors.color.set`,
`settings.workspace_colors.color.remove`, and
`settings.workspace_colors.palette.reset`.

## `app.forkConversationDefaultDestination`

Controls what the tab right-click `Fork Conversation` item does. The submenu still exposes every destination.

Values: `right`, `left`, `top`, `bottom`, `newTab`, `newWorkspace`.

Default: `right`.

## `ui.newWorkspace.menuSectionOrder`

Controls the section order in the titlebar `+` button menu. The Cloud VM section is built in; the custom section comes from `ui.newWorkspace.contextMenu`.

Values: `customFirst`, `cloudFirst`.

Default: `cloudFirst`.

```json
{
  "ui": {
    "newWorkspace": {
      "menuSectionOrder": "customFirst",
      "contextMenu": [
        "newWorkspace"
      ]
    }
  }
}
```

`sectionOrder` is accepted as an alias. Project-local `.cmux/cmux.json` values override the global setting.

## `terminal.agentHibernation`

Opt-in Agent Hibernation. cmux kills idle background agent processes to free RAM and CPU, then resumes each one with its saved session when you visit its tab. See [agent-hooks.md](agent-hooks.md#agent-hibernation) for the full behavior, including the confirmation settle window and how resume works.

```json
{
  "terminal": {
    "agentHibernation": {
      "enabled": true,
      "idleSeconds": 5,
      "maxLiveTerminals": 12,
      "confirmationSeconds": 60
    }
  }
}
```

- `enabled`: turn Agent Hibernation on. Default: `false`.
- `idleSeconds`: seconds a background idle agent terminal must be quiet before it can hibernate. Default: `5`. Range: `5`-`604800`.
- `maxLiveTerminals`: how many live restorable agent terminals to keep before cmux hibernates the oldest idle background ones. Nothing hibernates while you are at or under this count. Default: `12`. Range: `1`-`256`.
- `confirmationSeconds`: additional time the terminal output and scoped process set must remain unchanged before cmux terminates the agent. Default: `60`. Range: `1`-`600`.

Enable it from the command palette (`⌘⇧P` on macOS or `Ctrl+Shift+P` on Linux -> Enable Agent Hibernation), from **Settings > Terminal > Agent Hibernation**, or with `cmux agent-hibernation on`.

## `sidebar.showAgentActivity`

Shows a loading spinner on sidebar workspace rows that currently have running coding agents or active manual loaders (`cmux workspace loading on`).

```json
{
  "sidebar": {
    "showAgentActivity": true,
    "loadingSpinnerPosition": "leading",
    "notificationBadgePosition": "leading"
  }
}
```

- `showAgentActivity`: show the spinner at all. Default: `true`. It is a live status signal, so it stays visible even when `sidebar.hideAllDetails` is on. Toggle it from **Settings > Sidebar > Show Loading Spinner**.
- `loadingSpinnerPosition`: `leading` (left, sharing the unread-badge slot) or `trailing` (right, in the close-button corner). Default: `leading`.
- `notificationBadgePosition`: which side the unread notification badge sits on, `leading` or `trailing`. Default: `leading`.

The spinner is compositor-driven (a Core Animation transform run by the render server), so it costs no per-frame CPU and pauses automatically while the window is occluded or Reduce Motion is on. Toggle it manually per workspace with `cmux workspace loading <on|off> [--id <name>]`; each `--id` is a separate loader and the command prints the workspace state as `before=ON;after=OFF`.

## `terminal.showTextBoxOnNewTerminals` and `terminal.focusTextBoxOnNewTerminals`

`terminal.showTextBoxOnNewTerminals` opens the TextBox on newly-created terminal sessions without moving keyboard focus into it.

`terminal.focusTextBoxOnNewTerminals` opens the TextBox and focuses it for foreground terminal sessions created from the app UI, such as new terminal workspaces, tabs, and splits. Terminals created through the cmux CLI/control socket do not auto-focus the TextBox, even when this setting is enabled, so background automation does not steal keyboard focus.

## `terminal.textBoxSubmitActions`

Controls what the TextBox submit button does for new terminal sessions. Active agent sessions such as Claude, Codex, OpenCode, and Pi always use plain Text Entry so prompts go into the running agent instead of launching another command.

Press Shift-Tab in the TextBox to cycle the default action. This shortcut is `shortcuts.bindings.cycleTextBoxSubmitAction`; rebind or disable it from Settings > Keyboard Shortcuts or `cmux.json`. Right-click the submit button to pick any configured action or open this documentation.

```json
{
  "terminal": {
    "textBoxDefaultSubmitAction": "codex",
    "textBoxSubmitActions": [
      {
        "id": "codex",
        "title": "Codex --yolo",
        "kind": "commandTemplate",
        "commandTemplate": "codex --yolo -- {{prompt}}",
        "systemImage": "sparkles",
        "assetName": "AgentIcons/Codex",
        "backgroundColorHex": "#8FDBFF"
      },
      {
        "id": "custom-router",
        "title": "Custom Router",
        "kind": "commandTemplate",
        "commandTemplate": "agent-router --plan {{prompt}}",
        "systemImage": "wand.and.stars",
        "imagePath": "~/Pictures/router.png",
        "backgroundColorHex": "#3DDC97"
      }
    ]
  }
}
```

Built-in action IDs: `claude`, `codex`, `opencode`, `pi`.

Set `textBoxDefaultSubmitAction` to `text-entry` to force plain Text Entry for new terminals.
Built-in provider actions shell-quote `{{prompt}}` before pasting the command. Claude may still show its workspace trust prompt before processing the prompt. Built-ins run `claude --dangerously-skip-permissions -- {{prompt}}`, `codex --yolo -- {{prompt}}`, `opencode --prompt {{prompt}}`, and `pi -- {{prompt}}`.

Action fields:

- `id`: stable action ID.
- `title`: menu label for custom actions.
- `kind`: `textEntry` or `commandTemplate`.
- `commandTemplate`: shell command for `commandTemplate`. Include `{{prompt}}` where the prompt should be shell-quoted into the command line.
- `preservePromptAfterLaunch`: optional boolean for custom launch-only actions. When `true`, cmux submits `commandTemplate` as a provider launch command while keeping the TextBox prompt intact for the active agent session.
- `systemImage`: fallback SF Symbol name shown on the submit button.
- `assetName`: optional app asset catalog image name, for example `AgentIcons/Codex`.
- `imagePath`: optional PNG or image path for the submit button.
- `backgroundColorHex`: action color metadata as RGB or RGBA hex. The submit button fill stays white and only changes opacity between enabled and disabled states.

## `terminal.uploadCommands`

Replace the built-in `scp` for terminal file drops and pastes over SSH with a
command you choose. When you drop or paste a file into a terminal running an SSH
session, cmux normally `scp`s it to `/tmp/cmux-drop-<uuid>` on the host and types
the remote path. `terminal.uploadCommands` is an ordered list of host-scoped
rules; when the ssh destination matches a rule, cmux runs that rule's command
instead and inserts what the command prints.

```json
{
  "terminal": {
    "uploadCommands": [
      {
        "hostPattern": "*.example.com",
        "command": "my-upload \"$CMUX_UPLOAD_LOCAL_PATH\" \"$CMUX_UPLOAD_DESTINATION:$CMUX_UPLOAD_REMOTE_PATH\""
      }
    ]
  }
}
```

- `hostPattern`: an fnmatch glob matched against the ssh destination (`user@` and
  IPv6 brackets stripped, then lowercased) — the same glob style as a single
  `ssh_config` `Host` pattern (`*`, `?`; no pattern lists or `!` negation). Omit
  it, or set it to `null`, for a catch-all.
- `command`: run through `/bin/sh -c`, **once per file**. It receives the file and
  endpoint on its environment: `CMUX_UPLOAD_LOCAL_PATH`, `CMUX_UPLOAD_REMOTE_PATH`
  (the `/tmp/cmux-drop-<uuid>` path cmux picked), `CMUX_UPLOAD_DESTINATION`,
  `CMUX_UPLOAD_PORT`, `CMUX_UPLOAD_IDENTITY_FILE`, and `CMUX_UPLOAD_SSH_OPTIONS`
  (newline-separated; the last three are unset when the session has none). The
  rest of the environment is inherited, so a one-liner resolves tools on `PATH`.
- `enabled`: set to `false` to keep a rule in the list but skip it. Defaults to
  `true`.

**First matching enabled rule wins.** If no rule matches, the built-in `scp` runs
unchanged, so other hosts are untouched.

### How the command's output is used

cmux inserts the command's **stdout** at the cursor:

- Non-empty stdout is inserted **verbatim** (with control characters stripped),
  so a rule can emit a remote path, a URL, or any reference — for example a line
  an agent in the terminal will read.
- If the command prints **nothing**, cmux inserts the shell-escaped remote path it
  chose, so the simplest rule just moves the file and behaves like the built-in.
- For a multi-file drop, each file's output is joined with spaces.

The output is **inserted, not executed** — nothing auto-submits, and you review it
before pressing Enter. Because it is inserted verbatim (rather than shell-escaped
like the built-in path), a rule's stdout should be trusted: it can land at a shell
prompt. A non-zero exit, a timeout, or cancelling from the transfer indicator
inserts nothing — exactly like an `scp` failure.

## `automation.workspaceAutoNaming`

Opt-in AI auto-naming of workspaces and tabs from agent conversation content. When enabled, cmux summarizes supported agent sessions into short sidebar and tab names using each agent's own binary, and refreshes them as the conversation topic shifts. See [workspace-auto-naming.md](workspace-auto-naming.md) for the supported adapter list and full behavior.

```json
{
  "automation": {
    "workspaceAutoNaming": true
  }
}
```

Default: `false`. Manual renames (sidebar, command palette, CLI, or `/rename`) always win: a workspace or tab you renamed yourself is never auto-named again until you clear its custom name. Enable it from **Settings > Automation > Workspace Auto-Naming**.

## `diffViewer.defaultLayout`

Controls the initial layout for newly opened diff viewers.

Values: `unified`, `split`.

Default: `unified`.

```json
{
  "diffViewer": {
    "defaultLayout": "unified"
  }
}
```

The toolbar layout toggle persists the last user choice for future generated diff viewers. Passing `cmux diff --layout split` or `cmux diff --layout unified` overrides both the saved toolbar choice and this default for that invocation.
The native Linux toolbar switches between synchronized side-by-side and unified
rendering while using the same persisted choice.

## `sidebar.beta.workspaceTodos.checklistStyle`

Workspace todos are always available. Status is inferred from live signals (agent needs input / agent running / open PR / merged PRs / dirty tree) and can be pinned manually from the glyph's status popover, the row's context menu (Status submenu, Mark as Done), the command palette, or `cmux workspace status set <lane|auto>`; checklists are managed from the row, the workspace todo pane (`cmux todo open`), `cmux todo ...`, or by agents over the control socket.

`checklistStyle` picks how a row's checklist opens from its summary line: `popover` (default) anchors a checklist popover to the summary line; `inline` expands the items under the row like round one.

```json
{
  "sidebar": {
    "beta": {
      "workspaceTodos": {
        "checklistStyle": "popover"
      }
    }
  }
}
```

Default: `enabled: false`. The setting turns on automatically the first time a status or checklist mutation succeeds from any entrypoint.

Three keyboard shortcuts drive the todo state, all editable in **Settings > Keyboard Shortcuts** or `shortcuts.bindings`:

- `markWorkspaceDone` (default `cmd+;`) pins the selected workspace's status to done.
- `cycleWorkspaceStatus` (default `cmd+shift+;`) advances the status one lane forward (todo → working → needs-attention → review → done → todo).
- `toggleChecklistItemComplete` (default `cmd+return`) toggles the highlighted checklist item in the focused todo pane or checklist popover.

cmux also posts a notification when a workspace's status first reaches done, and when its checklist first becomes fully complete, so you can watch agent progress without keeping the pane open.

## Canvas interaction

Canvas pane movement and resizing can snap to neighboring edges, centers, and
the configured pane gap. On Linux, active snap targets are drawn as dashed
guides and holding Super while dragging temporarily suppresses snapping.

```json
{
  "canvas": {
    "paneGap": 16,
    "snappingEnabled": true
  }
}
```

- `paneGap`: canonical gap used by Canvas snapping, pane placement,
  distribution, and tidy operations. Default: `16`. Range: `0`-`64`.
- `snappingEnabled`: enable Canvas move and resize snapping. Default: `true`.

These values are also available from **Settings > General > Canvas** on Linux.

## `app.systemWideHotkeyEnabled`

Linux and macOS default this setting to `false`. When enabled, cmux registers
`showHideAllWindows` system-wide; its default is `Ctrl+Alt+.` on Linux.
`globalSearch` remains system-wide regardless of this flag and defaults to
`Ctrl+Alt+F`. Both bindings use the stable IDs in `shortcuts.bindings`, so the
Global Hotkey and Keyboard Shortcuts settings pages edit the same values.

The Linux GTK shell prefers the XDG GlobalShortcuts portal and falls back to
passive X11 grabs when the portal is unavailable. The
`global_shortcuts.status` socket method reports the active backend,
registrations, and any availability error.

## `shortcuts.bindings`

Keyboard shortcut overrides use the same stable action IDs on macOS and Linux.
Linux loads the layered configuration at startup and on `cmux reload-config`;
the native **Settings > Keyboard Shortcuts** editor writes the primary
`~/.config/cmux/cmux.json` file.

```json
{
  "shortcuts": {
    "bindings": {
      "newSurface": "super+alt+t",
      "focusLeft": "super+ctrl+h",
      "canvasAlignLeft": null
    }
  }
}
```

Linux built-ins never use Super. Explicit overrides still accept `super`,
`meta`, `cmd`, or `command` and display them as Super. Set a binding to `null`,
an empty string, `none`, `clear`, `unbound`, or `disabled` to unbind it.
Removing the action entry restores its built-in Linux default.

The mechanical Command-to-Ctrl conversion keeps the direct binding for the
common or focus-specific action. Conflicting less-common actions use these
Linux exceptions:

- `closeWindow`: `Ctrl+Alt+Shift+W`
- `equalizeSplits`: `Ctrl+Alt+Shift+=`
- `focusHistoryBack` / `focusHistoryForward`: `Ctrl+Alt+Shift+[` / `]`
- `selectWorkspaceByNumber`: `Ctrl+Alt+Shift+1…9`
- `canvasRevealFocusedPane`, `canvasOverview`, and `canvasTidy`:
  `Ctrl+Alt+Shift+R`, `Ctrl+Alt+Shift+O`, and `Ctrl+Alt+Shift+T`
- `toggleReactGrab`: `Ctrl+Alt+Shift+G`
- `toggleFullScreen`: unbound because `Ctrl+Alt+Shift+F` is Hide Find
- `openDiffViewer`: unbound because `Ctrl+Alt+Shift+D` is Split Browser Down

Context-specific duplicates remain direct only where their focus contexts do
not overlap: command-palette navigation versus opening/creating, browser versus
Markdown zoom, browser navigation versus workspace navigation, browser reload
versus rename, and right-sidebar modes versus numbered surface selection.

The Linux dispatcher includes the shared workspace, split-layout,
configuration, and notification action IDs, including `nextSidebarTab`,
`prevSidebarTab`, `closeWorkspace`, `toggleSplitZoom`, `equalizeSplits`,
`reloadConfiguration`, `showNotifications`, `jumpToUnread`, `toggleUnread`, and
`markOldestUnreadAndJumpNext`. On Linux, `showNotifications` reveals the
notification feed in the right sidebar.

App/window actions use `quit` (`Ctrl+Q`). `toggleFullScreen` is configurable
but unbound by default on Linux; its mechanical `Ctrl+F` mapping conflicts with
Find, and the `Ctrl+Alt+Shift+F` relocation is already used by Hide Find.
Fullscreen state is maintained per model window, persisted with
the Linux session, and synchronized bidirectionally with GTK. Quit checks all
windows for embedded terminals requiring confirmation before GTK exits.
`reopenPreviousSession` (`Ctrl+Shift+O`) opens a new copy of every window from
the snapshot captured at process launch, preserving the windows created or
changed during the current launch. The command is also available as **Restore
Previous App Launch** in the command palette.
`reopenClosedBrowserPanel` (`Ctrl+Shift+T`) restores the most recently closed
browser panel. Linux keeps the latest 100 browser closes in LIFO order,
including URL, title, navigation history, page zoom, owning workspace, pane,
and tab index. A surviving original pane is reused; otherwise cmux recreates
the removed split beside its surviving neighbor and falls back to the focused
pane if that neighbor is gone. Entries for deleted workspaces are discarded.
The same operation is exposed as **Reopen Last Closed** in the command palette
and as `history.reopen_closed` on the socket API.
`globalSearch` (`Ctrl+Alt+F`) opens **Search All Windows**, a dedicated live
search over every open window and panel. Empty-query results browse open
panels; typed queries use prefix-token AND matching over window, workspace,
and panel titles plus browser page text/URLs and Markdown contents/paths.
Results include the matching snippet and location, and selecting one focuses
its window, workspace, pane, and surface. Browser and Markdown content hits
also select the first matching inline-search needle. `Ctrl+1` through
`Ctrl+9` activate the corresponding visible result.
`commandPaletteNext` (`Ctrl+N`) and `commandPalettePrevious` (`Ctrl+P`) are
active only while the palette is visible; unbinding either action lets that
control key reach the focused terminal.

Focused topology and terminal actions also use the shared IDs.
`closeOtherTabsInPane` (`Ctrl+Alt+T`) closes unpinned sibling tabs in the
focused pane, while `toggleFocusedWorkspaceGroupCollapsed`
(`Ctrl+.`) expands or collapses the focused workspace's group and lets
the chord propagate when that workspace is ungrouped.
`groupSelectedWorkspaces` (`Ctrl+Shift+G`) creates a group from two or more
sidebar-selected workspaces. `toggleReactGrab` moved to `Ctrl+Alt+Shift+G` so
the two actions no longer overlap.
`clearScreenKeepScrollback`
(`Ctrl+Shift+K`) sends Ctrl-L to the focused terminal, preserving scrollback.
`sendCtrlFToTerminal` is unbound by default and can be assigned as an escape
hatch for TUIs that consume Ctrl-F. Both terminal actions route through the
live core PTY or the renderer-owned Ghostty input queue.

The beta terminal TextBox uses `focusTextBoxInput` (`Ctrl+Shift+A`) to move
between its multiline composer and the focused terminal, and
`attachTextBoxFile` (`Ctrl+Alt+Shift+A`) to open the multiple-file picker.
Text and ordered attachment paths are submitted through the same Ghostty input
queue as direct terminal paste/key events. Drafts persist per terminal in the
Linux session snapshot. Configure startup visibility/focus and composer height
with `terminal.showTextBoxOnNewTerminals`,
`terminal.focusTextBoxOnNewTerminals`, and `terminal.textBoxMaxLines`.

`toggleTerminalCopyMode` (`Ctrl+Shift+M`) enters the focused Ghostty terminal's
keyboard copy mode. Use `h`/`j`/`k`/`l` or the arrow keys to move, numeric
prefixes to repeat, `v` to start or clear visual selection, `y` to copy a
visual selection, and `yy` or `Shift+Y` to copy full lines. `gg`/`Shift+G`,
page and half-page controls, prompt jumps, and `/` plus `n`/`Shift+N` match the
macOS behavior. `Escape` or `q` exits; Ctrl shortcuts continue to reach the
application while the mode is active.

Find actions use `find` (`Ctrl+F`), `findInDirectory` (`Ctrl+Shift+F`),
`findNext` (`Ctrl+G`), `findPrevious` (`Ctrl+Alt+G`), `hideFind`
(`Ctrl+Alt+Shift+F`), and `useSelectionForFind` (`Ctrl+E`). On a terminal,
Linux opens the GTK search bar and routes navigation, selection, and close
through Ghostty binding actions. A focused browser returns these shortcuts to
WebKit so its native find handling remains authoritative. Directory search
opens and focuses the right sidebar's Find mode.

`toggleSidebar` uses `Ctrl+B` for the left workspace sidebar. Right-sidebar
actions use the same stable IDs as macOS with Linux Ctrl-based defaults:
`focusRightSidebar` (`Ctrl+Shift+E`), `toggleFileExplorer`
(`Ctrl+Alt+B`),
and `switchRightSidebarToFiles`, `switchRightSidebarToFind`,
`switchRightSidebarToSessions`, `switchRightSidebarToFeed`, and
`switchRightSidebarToDock` (`Ctrl+1` through `Ctrl+5`). The mode shortcuts are
active while keyboard focus is in the right sidebar unless their
`shortcuts.when` predicate is overridden.

Browser shortcuts share the macOS action IDs with Linux Ctrl-based defaults.
`openBrowser` uses `Ctrl+Shift+L`; `newBrowserWorkspace` uses `Ctrl+Alt+N`,
and `splitBrowserRight` and `splitBrowserDown` use `Ctrl+Alt+D` and
`Ctrl+Alt+Shift+D`. The workspace action creates a browser-only workspace;
the split actions create a browser in the exact requested direction. All three
focus the new browser's address bar. `focusBrowserAddressBar`, `browserBack`,
`browserForward`, `browserReload`, `browserHardReload`, `browserZoomIn`,
`browserZoomOut`, and `browserZoomReset` use `Ctrl+L`, `Ctrl+[`, `Ctrl+]`,
`Ctrl+R`, `Ctrl+Shift+R`, `Ctrl+=`, `Ctrl+-`, and `Ctrl+0` respectively.
`toggleBrowserDeveloperTools` and `showBrowserJavaScriptConsole` use
`Ctrl+Alt+I` and `Ctrl+Alt+C`. Browser-only actions default to
`browserFocus`, and Linux routes them to the live WebKitGTK view rather than
updating only the browser model.

Markdown viewer zoom uses the shared `markdownZoomIn`, `markdownZoomOut`, and
`markdownZoomReset` IDs with `Ctrl+=`, `Ctrl+-`, and `Ctrl+0`. These actions
default to `markdownFocus`, adjust the native viewer in one-point steps across
the macOS-compatible 8–96 point range, and reset to 15 points.
`saveFilePreview` uses `Ctrl+S` and saves the focused native file or Markdown
text editor from its live GTK buffer; rebinding or unbinding the action removes
the old key path rather than leaving a hardcoded editor shortcut active.
`editWorkspaceDescription` uses `Ctrl+Alt+E` and opens the focused workspace's
persisted Markdown description in the command palette. Enter saves,
`Shift+Enter` inserts a line break, empty input clears the description, and
closing the palette cancels the draft.

Focused diff viewers use the shared `diffViewerScrollDown`,
`diffViewerScrollUp`, `diffViewerScrollToBottom`, `diffViewerScrollToTop`, and
`diffViewerOpenFileSearch` IDs. Their macOS defaults are bare `J`, `K`,
`Shift+G`, the `G G` chord, and `/`. Linux accepts bare first strokes only for
this action family and only routes them while a native diff owns focus, so the
same keys continue to reach terminals and browser pages elsewhere. File search
matches changed paths case-insensitively and Enter advances through matches.

`selectSurfaceByNumber` and `selectWorkspaceByNumber` are shortcut families.
Their stored key is normalized to `1`, but the binding covers all digits from
`1` through `9`; digit `9` selects the last surface or workspace. Defaults are
`Ctrl+1…9` for surfaces in the focused pane and `Ctrl+Alt+Shift+1…9` for workspaces.
Two-stroke bindings are supported as well, with the second-stroke digit
normalized to `1`. While the right sidebar is focused, its priority
`Ctrl+1` through `Ctrl+5` mode actions win; outside that context the same keys
select numbered surfaces.

Two-stroke chords use the same array form on both platforms. The first stroke
must include a modifier; the second may be a bare key. The prefix is consumed
and applies only to the immediately following key event in the same window.

```json
{
  "shortcuts": {
    "bindings": {
      "newSurface": ["ctrl+b", "c"],
      "openFolder": ["ctrl+b", "o"]
    }
  }
}
```

## `shortcuts.when`

Context predicates can gate any supported shortcut action. Predicates use the
same VS Code-style grammar on macOS and Linux: `!`, `&&`, `||`, parentheses,
`==`, `!=`, `=~`, `<`, `<=`, `>`, `>=`, and `in [value, ...]`. Empty clauses
always match, unknown keys are false, and malformed clauses are ignored.

```json
{
  "shortcuts": {
    "bindings": {
      "newSurface": "super+ctrl+y",
      "openBrowser": "super+ctrl+y"
    },
    "when": {
      "newSurface": "terminalFocus && paneCount > 1",
      "openBrowser": "browserFocus && sidebarMode != 'find'"
    }
  }
}
```

Supported context keys are:

- `sidebarFocus`, `browserFocus`, `markdownFocus`, and `terminalFocus`: boolean
  focus dimensions. `terminalFocus` is true when none of the other dimensions
  owns the key event.
- `commandPaletteVisible` and `terminalFindVisible`: boolean overlay state.
- `sidebarMode`: `files`, `find`, `sessions`, `feed`, or `dock`.
- `paneCount` and `workspaceCount`: integer counts for the focused workspace
  and window.

Linux derives focus from the focused GTK widget and its containing surface or
right sidebar. The native shortcut editor displays an action's configured
predicate below its description. Config reloads clear pending chords and apply
both binding and predicate changes together.
