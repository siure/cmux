# cmux for Linux

This directory contains the Rust and GTK Linux port of cmux. The port launches
and embeds the repository's pinned Linux Ghostty fork, but it is still a
development project rather than a published daily-driver release.

The [roadmap](ROADMAP.md) is the source of truth for release blockers. macOS
feature parity is not the current goal.

## Quick start

From the repository root on Ubuntu 26.04 LTS, install the compiler, native
libraries, and bootstrap tools:

```bash
sudo apt update
sudo apt install --no-install-recommends \
  gcc g++ libegl1-mesa-dev libgtk-4-dev libwebkitgtk-6.0-dev \
  mesa-utils pkg-config curl python3 xz-utils
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
  . "$HOME/.cargo/env"
fi
rustup toolchain install 1.92.0 --profile minimal --component rustfmt
export RUSTUP_TOOLCHAIN=1.92.0
```

Then build the pinned Ghostty revision, build cmux with GTK, and launch the
native app:

```bash
./linux/scripts/run-dev.sh
```

The script verifies the submodule revision, provisions Ghostty's required Zig
version when necessary, uses locked Rust dependencies, and exports the runtime
paths needed by the embedded renderer. The Rust 1.92.0 baseline matches Linux
CI; newer toolchains may work but are not the reproducible baseline.

Run a display-free contract check without GTK or Ghostty:

```bash
cargo test --locked --manifest-path linux/Cargo.toml
```

See [Development](DEVELOPMENT.md) for isolated sockets, GTK tests, renderer
diagnostics, Xvfb smoke checks, and bundle validation.

## Control a running app

The app owns a JSON-lines Unix socket under `$XDG_STATE_HOME/cmux` or
`~/.local/state/cmux`. From a second terminal:

```bash
linux/target/debug/cmux ping
linux/target/debug/cmux --json identify
linux/target/debug/cmux tree
```

Use `--socket <path>` for an isolated development instance.

## Documentation map

| Document | Use it for |
| --- | --- |
| [Architecture](ARCHITECTURE.md) | Runtime flow, module ownership, Ghostty integration, state boundaries, and structural debt. |
| [Development](DEVELOPMENT.md) | Setup, build, test, diagnostics, installation, and contributor workflow. |
| [Roadmap](ROADMAP.md) | Daily-driver contract, release gates, supported environment, and deferred features. |
| [Reference](REFERENCE.md) | Detailed capabilities, command examples, configuration, browser, sidebar, mobile, and distribution notes. |
| [Linux Git workflow](../docs/linux-port-git-workflow.md) | Canonical branches, remotes, topic branches, and integration rules. |
| [Ghostty fork notes](../docs/ghostty-fork.md) | Fork-specific changes and submodule maintenance. |

## Directory map

| Path | Responsibility |
| --- | --- |
| `src/` | Rust application, protocol, renderer, terminal, and GTK implementation. |
| `tests/` | End-to-end socket and behavioral contracts. |
| `scripts/` | Reproducible development, bundle, install, and visual-smoke commands. |
| `dist/` | Desktop entry, launcher, and bundle installer inputs. |
| `examples/` | Custom-sidebar and WebKit smoke examples. |
| `webkit/` | WebKit web-process request-header extension. |

## Scope

The daily-driver path is the native GTK application with embedded Ghostty:
launch and reopen, interactive terminals, tabs, splits, workspaces, focus,
resize, close, keyboard and Unicode input, clipboard, scrollback, find,
configuration, and a reproducible local install.

Browser import, custom sidebars and extensions, mobile, remote tmux, agent
recovery, and update machinery remain in the tree but are not release blockers
unless they break the daily-driver path, security, or stored data.
