<h1 align="center">cmux</h1>
<p align="center">A Ghostty-based terminal with vertical tabs and notifications for AI coding agents</p>

<p align="center">
  <a href="https://github.com/manaflow-ai/cmux/releases/latest/download/cmux-macos.dmg">
    <img src="./docs/assets/macos-badge.png" alt="Download cmux for macOS" width="180" />
  </a>
</p>

<p align="center">
  English | <a href="README.ja.md">日本語</a> | <a href="README.vi.md">Tiếng Việt</a> | <a href="README.zh-CN.md">简体中文</a> | <a href="README.zh-TW.md">繁體中文</a> | <a href="README.ko.md">한국어</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.fr.md">Français</a> | <a href="README.it.md">Italiano</a> | <a href="README.da.md">Dansk</a> | <a href="README.pl.md">Polski</a> | <a href="README.ru.md">Русский</a> | <a href="README.bs.md">Bosanski</a> | <a href="README.ar.md">العربية</a> | <a href="README.no.md">Norsk</a> | <a href="README.pt-BR.md">Português (Brasil)</a> | <a href="README.th.md">ไทย</a> | <a href="README.tr.md">Türkçe</a> | <a href="README.km.md">ភាសាខ្មែរ</a> | <a href="README.uk.md">Українська</a>
</p>

<p align="center">
  <a href="https://x.com/manaflowai"><img src="https://img.shields.io/badge/@manaflow-555?logo=x" alt="X / Twitter" /></a>
  <a href="https://discord.gg/xsgFEVrWCZ"><img src="https://img.shields.io/badge/Discord-555?logo=discord" alt="Discord" /></a>
  <a href="https://github.com/manaflow-ai/cmux"><img src="https://img.shields.io/github/stars/manaflow-ai/cmux?style=flat&logo=github&label=stars&color=4c71f2" alt="GitHub stars" /></a>
</p>

<p align="center">
  <img src="./docs/assets/main-first-image.png" alt="cmux screenshot" width="900" />
</p>

<p align="center">
  <a href="https://www.youtube.com/watch?v=i-WxO5YUTOs">▶ Demo video</a> · <a href="https://cmux.com/blog/zen-of-cmux">The Zen of cmux</a>
</p>

## Features

<table>
<tr>
<td width="40%" valign="middle">
<h3>Notification rings</h3>
Panes get a blue ring and tabs light up when coding agents need your attention
</td>
<td width="60%">
<img src="./docs/assets/notification-rings.png" alt="Notification rings" width="100%" />
</td>
</tr>
<tr>
<td width="40%" valign="middle">
<h3>Notification panel</h3>
See all pending notifications in one place, jump to the most recent unread
</td>
<td width="60%">
<img src="./docs/assets/sidebar-notification-badge.png" alt="Sidebar notification badge" width="100%" />
</td>
</tr>
<tr>
<td width="40%" valign="middle">
<h3>In-app browser</h3>
Split a browser alongside your terminal with a scriptable API ported from <a href="https://github.com/vercel-labs/agent-browser">agent-browser</a>
</td>
<td width="60%">
<img src="./docs/assets/built-in-browser.png" alt="Built-in browser" width="100%" />
</td>
</tr>
<tr>
<td width="40%" valign="middle">
<h3>Vertical + horizontal tabs</h3>
Sidebar shows git branch, linked PR status/number, working directory, listening ports, and latest notification text. Split horizontally and vertically.
</td>
<td width="60%">
<img src="./docs/assets/vertical-horizontal-tabs-and-splits.png" alt="Vertical tabs and split panes" width="100%" />
</td>
</tr>
<tr>
<td width="40%" valign="middle">
<h3>SSH</h3>
<code>cmux ssh user@remote</code> creates a workspace for a remote machine. Browser panes route through the remote network so localhost just works. Drag an image into a remote session to upload via scp.
</td>
<td width="60%">
<img src="./docs/assets/ssh.png" alt="cmux SSH" width="100%" />
</td>
</tr>
<tr>
<td width="40%" valign="middle">
<h3>Claude Code Teams</h3>
<code>cmux claude-teams</code> runs Claude Code's teammate mode with one command. Teammates spawn as native splits with sidebar metadata and notifications. No tmux required.
</td>
<td width="60%">
<img src="./docs/assets/claude-code-teams.png" alt="Claude Code Teams" width="100%" />
</td>
</tr>
</table>

- **Browser import** — Import cookies, history, and sessions from Chrome, Firefox, Arc, and 20+ browsers so browser panes start authenticated
- **Custom commands** — Define project-specific actions in [`cmux.json`](https://cmux.com/docs/custom-commands) that launch from the command palette
- **Scriptable** — CLI and socket API to create workspaces, split panes, send keystrokes, and automate the browser
- **Native app** — Shipping as a Swift/AppKit macOS app today, with an in-tree Rust/GTK Linux source port that shares the terminal, browser, notification, and socket model.
- **Ghostty compatible** — Reads your existing `~/.config/ghostty/config` for themes, fonts, and colors
- **GPU-accelerated** — Powered by libghostty for smooth rendering

## Install

### DMG (recommended)

<a href="https://github.com/manaflow-ai/cmux/releases/latest/download/cmux-macos.dmg">
  <img src="./docs/assets/macos-badge.png" alt="Download cmux for macOS" width="180" />
</a>

Open the `.dmg` and drag cmux to your Applications folder. cmux auto-updates via Sparkle, so you only need to download once.

### Homebrew

```bash
brew tap manaflow-ai/cmux
brew install --cask cmux
```

To update later:

```bash
brew upgrade --cask cmux
```

On first launch, macOS may ask you to confirm opening an app from an identified developer. Click **Open** to proceed.

### Linux bundle and source port

The Linux port lives in [`linux/`](linux/README.md). It currently provides the
Rust app core, CLI/socket contract, a display-free app shell, optional GTK4 UI,
Ghostty VT integration, and a local Ghostty full-embedding path backed by the
sibling `ghostty/` checkout.

Build a relocatable archive containing cmux, `cmuxd-remote`, the Linux
libghostty embedding, Ghostty runtime resources, and desktop integration files:

```bash
./linux/scripts/build-bundle.sh
mkdir -p /tmp/cmux-linux
tar -xzf dist/cmux-linux-$(uname -m).tar.gz -C /tmp/cmux-linux
/tmp/cmux-linux/cmux-linux-$(uname -m)/bin/cmux-linux-app
```

Run `install.sh` from the extracted directory to install under `~/.local`, or
set `PREFIX` to choose another location. The archive requires the GTK 4 runtime;
WebKitGTK 6 remains optional for native browser surfaces. The builder itself
requires the development dependencies described below plus Zig and Rust.

For development directly from the source tree:

```bash
cargo run --manifest-path linux/Cargo.toml -- app
cargo run --manifest-path linux/Cargo.toml -- app --script $'status\nsplit right\npanes\nquit'
cargo test --manifest-path linux/Cargo.toml
```

The display-free core and tests do not require GTK. The GTK and full Ghostty
renderer paths require GTK 4 and WebKitGTK 6 development files so Cargo can
compile the embedded browser request extension (`libgtk-4-dev` and
`libwebkitgtk-6.0-dev` on Debian/Ubuntu). Native browser surfaces dynamically
load the WebKitGTK 6 runtime when available and retain the model preview as a
fallback when it is not installed. Browser DOM, input, script, and style
automation side effects are mirrored into the live WebKit document in command
order, including commands queued while a navigation is loading. Socket
`browser.eval` and `browser.evalhandle` calls return values produced by the live
document (including resolved promises) when that native view is mounted.
DOM-backed `browser.get` reads and visibility, enabled, and checked predicates
likewise report the native document instead of stale model state. Browser
headers and Basic credentials are applied to every native document and
subresource request, including HTTP authentication challenges. Browser
snapshots traverse that document and register their ephemeral `eN` refs for
subsequent selector-based actions. Screenshots capture the mounted WebKit view
as a native PNG, including full-document capture with `--full-page`, and PDF
exports print the live document through WebKitGTK. These operations use the
deterministic browser model when no native runtime is available.

Renderer choices are `core`, `gtk`, `ghostty-vt`, and `ghostty`. When
`--renderer` is omitted, `cmux app` reads `CMUX_LINUX_RENDERER` and falls back
to `core`. `core` is display-free, `ghostty-vt` uses the portable Ghostty VT
snapshot library when available, and `gtk`/`ghostty` require building with
`--features gtk`.

For the full Ghostty renderer path, first build the local Ghostty checkout:

```bash
(cd ../ghostty && zig build -Dapp-runtime=none)
cargo run --manifest-path linux/Cargo.toml --features gtk -- app --renderer ghostty
```

Linux feedback uses the production `/api/feedback` endpoint and stages every
report in a private offline queue before delivery. Run `cmux feedback retry`
against the running app to retry reports retained after a timeout, rate limit,
or service outage. The GTK shell presents a native feedback form with image
attachments and submits it in the background.

Linux renders declarative JSON and a bounded interpreted SwiftUI-style custom
sidebar subset from `~/.config/cmux/sidebars`. Use `cmux sidebar validate`,
`cmux sidebar select <name>`, and `cmux sidebar select workspaces`; the GTK
sidebar header includes the same provider picker. Selection persists under XDG
state and malformed edits retain the last good render. Swift files bind to live
workspace state, run parameterized cmux actions, and can use either in-process
evaluation or the isolated worker selected by `customSidebars.renderer`.

To build and install a user-local GTK desktop launcher directly from source:

```bash
./linux/scripts/install-dev.sh
gtk-launch ai.manaflow.cmux
```

## Why cmux?

I run a lot of Claude Code and Codex sessions in parallel. I was using Ghostty with a bunch of split panes, and relying on native macOS notifications to know when an agent needed me. But Claude Code's notification body is always just "Claude is waiting for your input" with no context, and with enough tabs open I couldn't even read the titles anymore.

I tried a few coding orchestrators but most of them were Electron/Tauri apps and the performance bugged me. I also just prefer the terminal since GUI orchestrators lock you into their workflow. So I built cmux first as a native macOS app in Swift/AppKit. The Linux port keeps the same terminal/browser/socket model in the Rust `linux/` tree, with GTK and local Ghostty embedding replacing the AppKit shell. Both paths use Ghostty for terminal rendering and read your existing Ghostty config for themes, fonts, and colors.

The main additions are the sidebar and notification system. The sidebar has vertical tabs that show git branch, linked PR status/number, working directory, listening ports, and the latest notification text for each workspace. The notification system picks up terminal sequences (OSC 9/99/777) and has a CLI (`cmux notify`) you can wire into agent hooks for Claude Code, OpenCode, etc. When an agent is waiting, its pane gets a blue ring and the tab lights up in the sidebar, so I can tell which one needs me across splits and tabs. Cmd+Shift+U jumps to the most recent unread on macOS; the Linux port uses Super+Shift+U for the same action.

The in-app browser has a scriptable API ported from [agent-browser](https://github.com/vercel-labs/agent-browser). Agents can snapshot the accessibility tree, get element refs, click, fill forms, and evaluate JS. You can split a browser pane next to your terminal and have Claude Code interact with your dev server directly.

Everything is scriptable through the CLI and socket API — create workspaces/tabs, split panes, send keystrokes, open URLs in the browser.

## The Zen of cmux

cmux is not prescriptive about how developers hold their tools. It's a terminal and browser with a CLI, and the rest is up to you.

cmux is a primitive, not a solution. It gives you a terminal, a browser, notifications, workspaces, splits, tabs, and a CLI to control all of it. cmux doesn't force you into an opinionated way to use coding agents. What you build with the primitives is yours.

The best developers have always built their own tools. Nobody has figured out the best way to work with agents yet, and the teams building closed products definitely haven't either. The developers closest to their own codebases will figure it out first.

Give a million developers composable primitives and they'll collectively find the most efficient workflows faster than any product team could design top-down.

## Documentation

For more info on how to configure cmux, [head over to our docs](https://cmux.com/docs/getting-started?utm_source=readme).

## Keyboard Shortcuts

The table below shows the macOS defaults. The Linux source port keeps the same
logical bindings and renders them with Linux modifier names: Super/Meta replaces
Command, Alt replaces Option, and Ctrl/Shift keep their normal names. For
example, New surface is Super+T and Jump to latest unread is Super+Shift+U on
Linux.

### Application

| Shortcut | Action |
|----------|--------|
| ⌃ ⌘ F | Toggle full screen |
| ⌘ Q | Quit cmux |
| ⌃ N / ⌃ P | Command palette next / previous |

### Workspaces

| Shortcut | Action |
|----------|--------|
| ⌘ N | New workspace |
| ⌥ ⌘ N | New browser workspace |
| ⌘ 1–8 | Jump to workspace 1–8 |
| ⌘ 9 | Jump to last workspace |
| ⌃ ⌘ ] | Next workspace |
| ⌃ ⌘ [ | Previous workspace |
| ⌘ ⇧ W | Close workspace |
| ⌃ ⌘ . | Toggle focused workspace group collapse |
| ⌘ ⇧ R | Rename workspace |
| ⌥ ⌘ E | Edit workspace description |
| ⌘ B | Toggle sidebar |
| ⌥ ⌘ B | Toggle right sidebar |
| ⌘ ⇧ E | Toggle right sidebar focus |

### Surfaces

| Shortcut | Action |
|----------|--------|
| ⌘ T | New surface |
| ⌘ ⇧ ] | Next surface |
| ⌘ ⇧ [ | Previous surface |
| ⌃ Tab | Next surface |
| ⌃ ⇧ Tab | Previous surface |
| ⌃ 1–8 | Jump to surface 1–8 |
| ⌃ 9 | Jump to last surface |
| ⌘ W | Close surface |
| ⌥ ⌘ T | Close other tabs in pane |
| ⌘ ⇧ K | Clear screen, keep scrollback |

### Split Panes

| Shortcut | Action |
|----------|--------|
| ⌘ D | Split right |
| ⌘ ⇧ D | Split down |
| ⌥ ⌘ D | Split browser right |
| ⌥ ⌘ ⇧ D | Split browser down |
| ⌥ ⌘ ← → ↑ ↓ | Focus pane directionally |
| ⌘ ⇧ H | Flash focused panel |

### Browser

Browser developer-tool shortcuts follow Safari defaults and are customizable in `Settings → Keyboard Shortcuts`.
Command palette navigation shortcuts, including ⌃ P, are also customizable and can be cleared so the keypress reaches the active terminal.

| Shortcut | Action |
|----------|--------|
| ⌘ ⇧ L | Open browser in split |
| ⌘ L | Focus address bar |
| ⌘ [ | Back |
| ⌘ ] | Forward |
| ⌘ R | Reload page |
| ⌥ ⌘ I | Toggle Developer Tools (Safari default) |
| ⌥ ⌘ C | Show JavaScript Console (Safari default) |

### Notifications

| Shortcut | Action |
|----------|--------|
| ⌘ I | Show notifications panel |
| ⌘ ⇧ U | Jump to latest unread |
| ⌥ ⌘ U | Toggle current item unread state |
| ⌃ ⌘ U | Mark current item as oldest unread and jump to next latest unread |

### Find

| Shortcut | Action |
|----------|--------|
| ⌘ F | Find |
| ⌘ ⇧ F | Find in directory |
| ⌘ G / ⌥ ⌘ G | Find next / previous |
| ⌥ ⌘ ⇧ F | Hide find bar |
| ⌘ E | Use selection for find |

### Terminal

| Shortcut | Action |
|----------|--------|
| ⌘ K | Clear scrollback |
| ⌘ C | Copy (with selection) |
| ⌘ V | Paste |
| ⌘ + / ⌘ - | Increase / decrease font size |
| ⌘ 0 | Reset font size |

### Window

| Shortcut | Action |
|----------|--------|
| ⌘ ⇧ N | New window |
| ⌘ ⇧ O | Reopen previous session |
| ⌘ , | Settings |
| ⌘ ⇧ , | Reload configuration |
| ⌘ Q | Quit |

## Nightly Builds

[Download cmux NIGHTLY](https://github.com/manaflow-ai/cmux/releases/download/nightly/cmux-nightly-macos.dmg)

cmux NIGHTLY is a separate app with its own bundle ID, so it runs alongside the stable version. Built automatically from the latest `main` commit and auto-updates via its own Sparkle feed.

Report nightly bugs on [GitHub Issues](https://github.com/manaflow-ai/cmux/issues) or in [#nightly-bugs on Discord](https://discord.gg/xsgFEVrWCZ).

## Session restore

Quitting cmux saves the current session. On relaunch, cmux restores app-owned
state:
- Window/workspace/pane layout
- Working directories
- Terminal scrollback (best effort)
- Browser URL and navigation history

cmux does not checkpoint arbitrary live process state. tmux, vim, shells, and
unsupported terminal apps reopen as normal terminals.

Supported agent sessions can resume when hooks have saved a native session ID.
Install hooks after installing the agent CLI so its binary is on `PATH`:

```bash
cmux hooks setup
cmux hooks setup codex
cmux hooks setup --agent opencode
```

`cmux hooks setup` installs supported agents it can find and prints a summary
for skipped agents. Supported resume integrations include Claude Code, Codex,
Grok, OpenCode, Pi, Amp, Cursor CLI, Gemini, Rovo Dev, Copilot, CodeBuddy,
Factory, and Qoder. Claude Code is handled by the cmux Claude wrapper when Claude
integration is enabled in Settings.

Advanced users and integrations can attach a custom resume command to the
current terminal surface. This is useful for tools with their own durable state,
such as tmux sessions or custom agent CLIs:

```bash
cmux surface resume set --kind tmux --checkpoint work --shell "tmux attach -t work"
cmux surface resume show --json
cmux surface resume clear --checkpoint work
```

The binding stays attached to the cmux surface. Public CLI or socket-created
bindings are stored for inspection and manual restore unless you approve a
signed command prefix for automatic restore. Approved prefixes are also bound to
the working directory and exact environment values, when present. Review or edit
approvals in **Settings > Terminal > Resume Commands**. cmux only auto-runs
resume bindings it marks trusted, such as live process-detected tmux bindings or
user-approved prefixes. Sensitive environment keys such as tokens, passwords,
secrets, and API keys are dropped before a resume binding is stored.

To keep restored agent terminals idle instead of automatically running their resume commands,
turn off **Settings > Terminal > Resume Agent Sessions on Reopen** or set this in
`~/.config/cmux/cmux.json`:

```json
{
  "terminal": {
    "autoResumeAgentSessions": false
  }
}
```

This only disables automatic agent resume commands. cmux still restores the saved layout,
working directories, scrollback, and browser history.

If you need to reapply the last saved snapshot manually, use:
- `File > Reopen Previous Session`
- `⌘ ⇧ O` on macOS, or `Super+Shift+O` in the Linux source port
- `cmux restore-session`

Under the hood, the macOS app writes a versioned snapshot under
`~/Library/Application Support/cmux/` and agent hooks write session mappings
under `~/.cmuxterm/`. The Linux source port exposes the same
`cmux restore-session` and surface resume commands against the running app
state, stores a versioned app snapshot under `$XDG_STATE_HOME/cmux/` or
`~/.local/state/cmux/`, stores cmux configuration under
`~/.config/cmux/cmux.json`, and continues to use `~/.cmuxterm/` for agent hook
mappings. On restore, cmux rebuilds the layout first, then runs
the supported agent's native resume command when automatic agent resume is
enabled.

Read the full guide at <https://cmux.com/docs/session-restore>.

## Star History

<a href="https://star-history.com/#manaflow-ai/cmux&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=manaflow-ai/cmux&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=manaflow-ai/cmux&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=manaflow-ai/cmux&type=Date" width="600" />
 </picture>
</a>

## Contributing

Ways to get involved:

- Follow us on X for updates [@manaflowai](https://x.com/manaflowai), [@lawrencecchen](https://x.com/lawrencecchen), and [@austinywang](https://x.com/austinywang)
- Join the conversation on [Discord](https://discord.gg/xsgFEVrWCZ)
- Create and participate in [GitHub issues](https://github.com/manaflow-ai/cmux/issues) and [discussions](https://github.com/manaflow-ai/cmux/discussions)
- Let us know what you're building with cmux

## Community

- [Discord](https://discord.gg/xsgFEVrWCZ)
- [GitHub](https://github.com/manaflow-ai/cmux)
- [X / Twitter](https://twitter.com/manaflowai)
- [YouTube](https://www.youtube.com/channel/UCAa89_j-TWkrXfk9A3CbASw)
- [LinkedIn](https://www.linkedin.com/company/manaflow-ai/)
- [Reddit](https://www.reddit.com/r/cmux/)

## Founder's Edition

cmux is free, open source, and always will be. If you'd like to support development and get early access to what's coming next:

**[Get Founder's Edition](https://buy.stripe.com/3cI00j2Ld0it5OU33r5EY0q)**

- **Prioritized feature requests/bug fixes**
- **Early access: cmux AI that gives you context on every workspace, tab and panel**
- **Early access: iOS app with terminals synced between desktop and phone**
- **Early access: Cloud VMs**
- **Early access: Voice mode**
- **My personal iMessage/WhatsApp**

## License

cmux is open source under [GPL-3.0-or-later](LICENSE).

If your organization cannot comply with GPL, a commercial license is available. Contact [founders@manaflow.com](mailto:founders@manaflow.com) for details.
