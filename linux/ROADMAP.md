# Linux Roadmap

This roadmap defines when the Linux port is useful enough to be a daily driver.
It is intentionally smaller than macOS feature parity. Reliability in the core
terminal workflow comes before expanding the feature surface.

## Daily-driver contract

A Linux build is considered usable when every release-blocking capability below
works in the native GTK application with the embedded Ghostty renderer.

| Area | Required behavior | Evidence required |
| --- | --- | --- |
| Build and launch | A fresh checkout can initialize the pinned Ghostty submodule, build Ghostty and cmux, and launch the GTK app from documented commands. | A clean build using locked Rust dependencies, followed by a native launch smoke test. |
| Lifecycle | The app can quit cleanly and reopen a saved session without losing valid workspace, pane, or tab topology. | Automated contract coverage plus one launch, quit, and reopen smoke test. |
| Terminal | A Ghostty surface accepts interactive input, renders command output, Unicode, and a long scrollback buffer, and exposes terminal find. | Automated renderer and socket contracts plus a live Ghostty smoke test. |
| Topology | Users can create, select, rename, and close workspaces; create, resize, focus, and close split panes; and create, select, reorder, and close tabs without corrupting the layout. | Behavioral contract tests for state transitions plus a native GTK smoke test. |
| Clipboard and input | Keyboard input, common control keys, copy, paste, selection, and input-method composition behave correctly. | Contract tests where the behavior is display-independent, plus manual checks on supported display backends. |
| Configuration | cmux loads a documented user configuration, reports invalid values clearly, and preserves supported settings across restart. | Parser and persistence tests plus a restart smoke test using a temporary XDG directory. |
| Local installation | A contributor can create and install the relocatable development bundle without editing generated files by hand. | Bundle build and install scripts run successfully from a clean checkout. |

Passing only the display-free model tests does not satisfy this contract. A
release candidate must also exercise the GTK host and the exact Ghostty commit
pinned by the parent repository.

## Release gates

Every daily-driver release must meet all of these gates:

1. The build and launch commands in `DEVELOPMENT.md` work from a clean checkout
   on the primary environment defined below.
2. `cargo test --locked --manifest-path linux/Cargo.toml` passes.
3. `cargo test --locked --manifest-path linux/Cargo.toml --features gtk` passes
   in an environment with the documented GTK and WebKitGTK development files.
4. A live Ghostty smoke test covers launch, typing, Unicode, tab creation,
   splitting, focus, resize, copy and paste, scrollback, find, workspace
   switching, session reopen, and clean quit.
5. Renderer diagnostics report the expected Ghostty ABI, symbols, resources,
   and layout fingerprints for the pinned submodule revision.
6. An isolated temporary-XDG test proves configuration persistence and session
   reopen across a clean application restart.
7. The relocatable bundle builds and installs into a clean user-local prefix,
   and its desktop entry launches the installed binary without source-tree
   paths.
8. There are no open failures in the daily-driver contract. A team may revise
   the contract before a release, but it may not waive a failing contract row
   while still claiming that release satisfies the contract.

Where a behavior cannot be automated reliably, the manual check and its test
environment belong in the release evidence. A successful build by itself is
not evidence that the native terminal is usable.

## Supported environment matrix

The first daily-driver release has one deliberately narrow primary target:

| Role | Environment | Release expectation |
| --- | --- | --- |
| Primary build and desktop | Ubuntu 26.04 LTS with GTK 4, WebKitGTK 6, and a native GNOME Wayland session | Every release gate must pass. |
| Automated display smoke | Xvfb on Ubuntu 26.04 LTS | Useful for repeatable GTK and Ghostty checks, but it does not replace the primary native-display smoke. |
| Secondary desktop | Ubuntu 26.04 LTS X11 or XWayland session | Failures are tracked, but support is not claimed until the environment is promoted into the primary contract. |

Other distributions remain contributor-tested until their package prerequisites
and native smoke results are repeatable. Expanding this matrix is an explicit
contract change, not an assumption hidden behind the word "supported."

## Current baseline

The 2026-08-20 ownership audit established this baseline:

- The repository's pinned Linux Ghostty fork builds in `ReleaseSafe` mode.
- cmux builds with the `gtk` feature using locked Rust dependencies.
- The GTK application launches under Xvfb with a live embedded Ghostty surface.
- Renderer diagnostics confirm ABI 15, required symbols, resource discovery,
  and matching layout and constant fingerprints.
- Live socket-driven checks cover terminal input and output, tab and split
  creation, focus, split equalization, scrollback output, terminal find,
  workspace switching, and clean quit.

The baseline does not yet prove the full daily-driver contract. These gaps must
be closed before calling a Linux build release-ready:

- Launch, quit, and session reopen have not been exercised as one isolated
  persistence scenario.
- Clipboard selection and copy, input-method composition, and real interactive
  Unicode entry need display-backed validation.
- Hardware-accelerated Wayland and X11 sessions need coverage beyond Xvfb.
- Multi-monitor behavior needs a real compositor smoke test.
- The development bundle and desktop installation need a clean-environment
  verification.
- The GTK feature test binary is not reliably isolated from process-global GTK
  initialization. On the 2026-08-20 Ubuntu audit host, full ownership-branch
  runs showed cross-thread GTK initialization failures or a headless SIGSEGV,
  while each reported GTK test passed alone. The pre-cleanup integration commit
  independently reproduces the headless serial SIGSEGV. The GTK release gate
  remains open until display-backed tests run on one owned GTK thread or are
  separated into safe test processes.

## Deferred feature families

The following code may remain present and maintained, but new feature work in
these areas is not release-blocking until the daily-driver contract is reliable:

- browser-profile import and extension migration
- custom sidebars and process-isolated sidebar extensions
- mobile clients and remote mobile-host validation
- remote tmux and cloud workspace integrations
- provider-specific agent recovery and approval polish
- automatic update and production release machinery
- macOS-only settings or system integrations without an agreed Linux equivalent

A bug in a deferred family may still block a release when it causes data loss,
crashes the default application path, breaks security boundaries, or prevents a
daily-driver capability from working.

## Work sequence

### 1. Establish ownership

- Preserve all pre-cleanup Git and working-tree state.
- Work from the canonical `feat/linux-port` integration history on a focused
  topic branch.
- Verify the pinned Ghostty fork and record reproducible build evidence.

Status: complete.

### 2. Make the system understandable

- Keep this contract as the scope boundary.
- Document architecture, development workflows, and the Ghostty integration.
- Turn `README.md` into a short entry point and move detailed reference material
  behind explicit links.

Status: complete.

### 3. Reduce structural friction

- Add a library composition root and keep the executable entry point thin.
- Group terminal and Ghostty integration, GTK shell code, application state,
  and CLI code by responsibility.
- Make only mechanical moves first, then extract behavior behind focused tests.
- Introduce more Cargo crates only when a real dependency boundary requires one.

Status: complete for the hierarchy foundation. Further extraction proceeds one
behavior boundary at a time.

### 4. Close the contract gaps

- Automate the isolated session-reopen and configuration-persistence scenarios.
- Add a repeatable real-display smoke checklist for clipboard, IME, GPU, and
  multi-monitor behavior.
- Validate the relocatable bundle and user-local desktop installation.
- Publish release evidence with known limitations.

Status: planned.

## Feature intake rule

Until every daily-driver row has repeatable evidence, proposed Linux work must
do at least one of the following:

- close a contract gap
- simplify or clarify an owned subsystem
- add focused coverage for an existing daily-driver behavior
- fix a correctness, security, or data-loss issue

Everything else goes into the deferred backlog. This keeps the port usable and
understandable instead of growing its surface faster than it can be verified.
