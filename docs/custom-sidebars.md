# Custom sidebars: vibe-code your own cmux sidebar

cmux lets you build your own sidebar UI by writing a small SwiftUI-style file.
It is interpreted at runtime (no Xcode, no build step, no signing), renders as
native SwiftUI in the real sidebar, hot-reloads on save, binds to live cmux
state, and can run cmux commands on tap. This guide is the authoring contract
for you or a coding agent.

It is a beta, on by default. Turn it off in **Settings → Custom Sidebars**
(`customSidebars.beta.enabled`). While off, custom sidebars do not appear.

## If you are an agent building this for someone

Assume the person asking is not technical. They are describing a result ("a
sidebar that shows my workspaces and lets me jump between them"), not an
implementation. Your job is to turn that into a clean, native-looking, working
sidebar and make the engineering decisions for them. Do not ask them about
SwiftUI, files, or syntax. Concretely:

- Default to real, live data. If they mention workspaces/tabs, bind to the
  `workspaces` context (not hard-coded text) so it stays correct on its own.
- Make it interactive by default. Rows that represent something you can open
  should be tappable and run the matching `cmux(...)` action (e.g. selecting a
  workspace, focusing a tab). A list that just displays text is rarely what
  they wanted.
- If the list is something a person would naturally reorder (workspaces, tasks,
  a queue), make it drag-and-drop reorderable with `Reorderable` (see below).
  When in doubt for a workspace list, prefer `Reorderable`.
- Keep it native and uncluttered: a title, a divider, then the content. Use the
  status dot / pill / highlight patterns below so it is scannable at a glance.
- Lazy-load / cap large lists (see Performance). Do not render hundreds of rows.
- Iterate by saving the file and looking at the result (it hot-reloads); fix
  what looks off. Verify it shows real data and that taps do the right thing
  before declaring it done.
- Stay inside the supported subset below. If something is not supported, choose
  the closest supported approach rather than failing.

## Where to put a sidebar

Write a named file (the name becomes the menu label; use short kebab-case):

    ~/.config/cmux/sidebars/<name>.swift     # interpreted Swift (preferred)
    ~/.config/cmux/sidebars/<name>.json      # declarative JSON (simpler, static)

Each file shows up as an option in the **sidebar toggle button's right-click
menu**. Pick it and it renders in the sidebar; edit the file and save and it
hot-reloads. If both `<name>.swift` and `<name>.json` exist, `.swift` wins.

A sidebar file is a single SwiftUI-style view expression (no `struct`, no
`var body` wrapper, just the view).

### Linux status

The Linux GTK port discovers the same directory and preserves `.swift` before
`.json` precedence. It renders declarative JSON sidebars and interprets the
supported SwiftUI-style subset natively, including live workspace data,
provider selection, hot re-read, last-good fallback, parameterized button/tap
actions, helpers, loops, conditionals, common styles, and `Reorderable` rows.
`cmux sidebar validate|reload|select` uses the same method names and report
shape as macOS. Use `cmux sidebar select workspaces` to return to the built-in
Linux workspace list.

Linux supports both `inProcess` and `remote` renderer values. The remote lane
uses a bounded out-of-process interpreter worker and keeps the last good
document if the worker fails or times out. GTK renders the resulting neutral
document tree in the host, so hover and keyboard behavior remain GTK-native.

The ready-to-copy JSON example is `linux/examples/custom-sidebar.json`; the
Swift examples under `Examples/CustomSidebars/` are also exercised by Linux
tests.

## Choosing the renderer (in-process vs remote)

By default a custom sidebar renders in-process. On macOS the interpreted view
mounts as real SwiftUI; on Linux the neutral document tree mounts as native
GTK. Hover styling, focus, keyboard, and same-frame resize therefore use the
host toolkit. The tradeoff is that the interpreter shares the host process.

For sidebars from sources you do not fully trust you can switch to the
remote renderer, an out-of-process worker. That is the containment lane: a
crash or hang caused by the interpreted file cannot take down cmux. The macOS
remote layer accepts forwarded clicks but not hover, focus, or keyboard. Linux
isolates parsing/evaluation in the worker and renders the returned neutral tree
as native GTK, retaining host-side input behavior.

Set it in **Settings → Custom Sidebars**, or in `~/.config/cmux/cmux.json`:

    { "customSidebars": { "renderer": "remote" } }

Valid values are `"inProcess"` (default) and `"remote"`. The setting is read
live; flipping it re-renders the selected sidebar without a restart. Both
renderers protect the host against pathological sources with an evaluation
budget (nesting depth and total produced nodes): a render that exceeds the
budget is discarded and the last good render stays up.

## Downloadable examples

The repo includes ready-to-copy sidebars in `Examples/CustomSidebars/`:

- `status-board.swift` groups workspaces by live signals like urgent bugs,
  review, progress, research, and done.
- `finder.swift` shows a macOS Finder-style workspace browser with a source
  list, selected workspace details, and tabs.

Install one from a cmux checkout:

    mkdir -p ~/.config/cmux/sidebars
    cp Examples/CustomSidebars/status-board.swift ~/.config/cmux/sidebars/status-board.swift
    cp Examples/CustomSidebars/finder.swift ~/.config/cmux/sidebars/finder.swift

Then validate and select it:

    cmux sidebar validate status-board
    cmux sidebar select status-board

## Quick start

    cat > ~/.config/cmux/sidebars/mine.swift <<'SWIFT'
    VStack(alignment: .leading, spacing: 8) {
        Text("My sidebar").font(.title3).bold()
        Text(clock.time).font(.caption).foregroundColor(.secondary)
        Divider()
        ForEach(workspaces) { w in
            Button(action: { cmux("workspace.select", workspace_id: w.id) }) {
                HStack {
                    Text(w.selected ? "●" : "○").foregroundColor(w.selected ? "#FF8800" : .secondary)
                    Text(w.title)
                    Spacer()
                }
            }
        }
    }
    SWIFT

Then right-click the sidebar button and choose **mine**.

## Live data you can bind to (read-only, refreshes ~1s)

- `workspaces` — array, one per workspace. Always present: `id`, `title`,
  `selected` (Bool), `pinned` (Bool), `index` (Int), `directory`, `ports`
  (array of Int) + `portCount`, `unread` (Int notifications), `tabs` + `tabCount`.
  Present when the workspace has them (use `if let` / ternary): `description`,
  `color` (hex), `branch` + `dirty` (Bool) from git, `pr`
  (`{ number, label, url, status: open|merged|closed, stale, branch }`, the
  workspace's first pull request in sidebar display order) + `prs` (array of
  the same shape with every pull request cmux knows for the workspace),
  `progress` (`{ value: 0..1, label }`), `latestMessage` (last agent message),
  `latestPrompt` (last submitted prompt), `latestAt` (epoch), `remote`
  (`{ target, state, connected }`).
- `tabs` (per workspace) — array of surfaces. Always: `id`, `title`,
  `focused` (Bool), `pinned` (Bool). When available: `directory`, `branch` +
  `dirty`, `ports` (array of Int).
- `workspaceCount` — Int. `selectedTitle` — active workspace's title.
  `selectedId` — its id. `unreadTotal` — total unread notifications.
- `clock` — `{ time ("HH:mm:ss"), hour, minute, second, weekday, epoch }`. The
  sidebar re-renders about once a second, so clocks/countdowns and workspace
  changes are live.

Optional fields are omitted when the workspace doesn't have them, so guard with
`if let b = w.branch { ... }` or `w.pr != nil ? ... : ...` rather than assuming
they exist.

## Views

Containers: `VStack(alignment:spacing:)`, `HStack`, `ZStack`, `LazyVStack`,
`LazyHStack`, `Group`, `EmptyView()`, `List { ... }`, `Section("Header") { ... }`,
`Grid { GridRow { ... } }`, `LazyVGrid`, `LazyHGrid`, `ViewThatFits { ... }`,
`ScrollView { ... }` (use `ScrollView(.horizontal) { HStack { ... } }` for a
horizontal strip — vertical scrolling is automatic), and
`HSplitView { columnA; columnB }` (two resizable, independently-scrolling
columns with a persisted divider).

Content: `Text("...")`, `Label("Title", systemImage: "folder")`,
`Image(systemName: "folder.fill")` (SF Symbols),
`Button("Title") { <action> }` / `Button(action:){ <label> }`,
`Toggle("Title", isOn: $value)`, `TextField("Placeholder", text: $value)`,
`Slider(value: $number, in: 0.0...1.0, step: 0.1)`,
`Picker("Title", selection: $value) { Text("Option").tag("value") }`, and
`Stepper("Title", value: $number, in: 0...10, step: 1)`,
`Menu("Title") { <items> }`, `ProgressView(value: 0.4)` / `ProgressView()`,
`Gauge(value: 0.7)`, `Spacer()`, `Divider()`, `AnyView(<view>)`.

Shapes: `Rectangle`, `RoundedRectangle(cornerRadius:)`,
`UnevenRoundedRectangle`, `Capsule`, `Circle`, `Ellipse` — fill with
`.fill(color)` / `.foregroundColor`, outline with `.stroke("#hex", lineWidth: 2)`,
arc with `.trim(from:to:)`, size with `.frame`.

Reorder: `Reorderable(data, move: "workspace.reorder") { item in <row> }` (see below).

## Modifiers

Text/typography: `.font(.title2|.headline|.caption|.system(size:design:)...)`,
`.bold()`, `.italic()`, `.fontWeight(.semibold)`, `.fontDesign(.monospaced)`,
`.monospaced()`, `.monospacedDigit()`, `.lineLimit(1)`, `.truncationMode(.tail)`,
`.multilineTextAlignment(.center)`, `.textCase(.uppercase)`, `.strikethrough()`,
`.underline()`.

Color/fill: `.foregroundColor`/`.foregroundStyle`/`.fill`/`.tint` taking a hex
string `"#FF8800"` or a token (`primary`, `secondary`, `tertiary`, `accent`,
`red`, `blue`, `mint`, `indigo`, `teal`, `cyan`, `brown`, …). `Color("#hex")` /
`Color(red:green:blue:)` values too.

Layout: `.padding(8)`, `.frame(width:height:maxWidth:.infinity, alignment:)`,
`.fixedSize()`, `.layoutPriority(1)`, `.offset(x:y:)`, `.zIndex(1)`,
`.aspectRatio(contentMode:.fit)`, `.scaledToFit()`/`.scaledToFill()`.

Decoration: `.background("#hex")` **or** `.background { <view> }`,
`.overlay(alignment:.topTrailing) { <view> }`, `.mask { <view> }`,
`.safeAreaInset(edge:.top) { <view> }`, `.cornerRadius(8)`,
`.clipShape(Circle())`, `.clipped()`, `.shadow(color:radius:x:y:)`,
`.border(.gray, width:1)`, `.blur(radius:)`, `.opacity(0.6)`,
`.brightness`/`.contrast`/`.saturation`/`.grayscale`,
`.rotationEffect(.degrees(45))`, `.scaleEffect(1.2)`, `.redacted(reason:.placeholder)`.

SF Symbols: `.imageScale(.large)`, `.symbolRenderingMode(.hierarchical)`,
`.symbolVariant(.fill)`.

Interaction/semantics: `.onTapGesture { <action> }` (any view tappable),
`.contextMenu { <buttons> }`, `.help("tip")`, `.disabled(cond)`,
`.accessibilityLabel("...")`.

The decoration modifiers that take a trailing `{ <view> }` (`.overlay`,
`.background`, `.mask`, `.safeAreaInset`, `.contextMenu`) accept **any** nested
view, so you can compose badges, rings, status dots, etc.

## Language

`let` bindings; user `func` helpers (value helpers and view helpers returning
`some View`, explicit `return` supported); `for i in 0..<n` / `1...n` /
`for x in array`; `ForEach(array) { item in ... }`,
`ForEach(array.indices) { i in }`, and
`ForEach(Array(array.enumerated()), id: \.offset) { i, item in }`; `if/else`;
ternary `cond ? a : b` (works in modifiers and interpolation); string
interpolation `"\(expr)"`; arithmetic `+ - * / %` (safe on `/ 0`); comparisons;
`&& || !` (short-circuiting); ranges; array/dictionary literals; member access
(`obj.field`, `array.count`/`.first`/`.last`/`.indices`, `string.count`);
subscript `array[i]`, `obj["key"]`.

Array methods: `.filter`, `.map`, `.flatMap`, `.reduce`, `.sorted { $0 > $1 }`,
`.first`, `.contains`, `.count`, `.reversed`, `.prefix(n)`, `.suffix(n)`,
`.dropFirst(n)`, `.dropLast(n)`, `.enumerated()`, `.indices`. String methods:
`.hasPrefix`, `.hasSuffix`, `.contains`, `.uppercased()`, `.lowercased()`,
`.split(separator:)`. Numbers: `.formatted(.currency(code:"USD"))` /
`.formatted(.percent)` / `.formatted(.notation(.compactName))`. Builtins:
`min`, `max`, `abs`, `Int(...)`, `Double(...)`, `String(...)`.

## Actions (run real cmux commands on tap)

A button or `.onTapGesture` body calls `cmux("<method>", param: value)`. On tap
it runs that cmux command through the same dispatcher as the `cmux` CLI:

    Button(action: { cmux("workspace.select", workspace_id: w.id) }) { ... }
    ...onTapGesture { cmux("surface.focus", surface_id: t.id) }

Use real method and parameter names. Common ones: `workspace.select`
(`workspace_id`), `surface.focus` (`surface_id`), `workspace.reorder`
(`workspace_id` + `index`). Run `cmux docs api` to discover the full command
surface.

## Local state and input controls

Linux supports top-level and identity-scoped per-row `@State` values that
survive the roughly once-per-second data refresh and normal app restarts:

    @State private var count = 0
    @State private var enabled = true
    @State private var name = "cmux"
    @State private var volume = 0.5
    @State private var mode = "balanced"

    VStack {
        Text("Count \(count)")
        Button("Increment") { count += 1 }
        Button("Toggle") { enabled.toggle() }
        Toggle("Enabled", isOn: $enabled)
        TextField("Name", text: $name)
        Slider(value: $volume, in: 0.0...1.0, step: 0.1)
        Picker("Mode", selection: $mode) {
            Text("Fast").tag("fast")
            Text("Balanced").tag("balanced")
        }
        .onChange(of: mode) { oldValue, newValue in
            log("\(oldValue) -> \(newValue)")
        }
        Stepper("Count \(count)", value: $count, in: 0...10, step: 1)
    }

Button actions support `=`, `+=`, `-=`, `.toggle()`, and `.append(...)` against
declared state. `Toggle` requires a Bool binding, `TextField` requires a String
binding, numeric controls require numeric bindings, and `Picker` matches its
typed selection against unique `.tag(...)` values. State is isolated per
sidebar and stored privately in
`$XDG_STATE_HOME/cmux/custom-sidebar-state.json` or
`~/.local/state/cmux/custom-sidebar-state.json`. Reset the selected sidebar or a
named sidebar with:

    cmux sidebar clear-state
    cmux sidebar clear-state mine

Changing an `@State` declaration to a different value type resets that value to
the new initializer. Editing the initializer while retaining the same type keeps
the user's current value. Declarative JSON controls use the same provider-scoped
store: each binding's document value seeds the key, while later renders reuse
the persisted typed value.

State declared in a helper called from `ForEach`, `Reorderable`, or an ordinary
`for` loop is isolated per row. Prefer an explicit stable identity when rows can
move or their displayed fields can change:

    func row(_ item: Item) -> some View {
        @State private var expanded = false
        return Toggle(item.title, isOn: $expanded)
    }

    ForEach(items, id: \.key) { item in
        row(item)
    }

`ForEach` honors its `id:` key path, then an item `id`, then the item value.
`Reorderable` uses the item's `id`; ordinary `for` loops use an item `id` when
present and otherwise the item value. Removed generated instances are pruned
from persisted state. A sidebar may have at most 256 active state identities
and 32 nested identity scopes.

`.onChange(of: stateValue)` runs after any matching control, button, or socket
state write. Closures may omit parameters, accept the new value, or accept
`oldValue, newValue`; callback mutations can trigger other change hooks, with a
bounded recursion limit. `.onSubmit` runs when Return is pressed in a descendant
`TextField`, including handlers attached to a parent container, and reads the
latest persisted field value.

## Reusable custom views

Simple stored-property `View` structs can package repeated rows:

    struct StatusRow: View {
        let title: String
        let detail: String = "Ready"
        @State private var expanded = false

        var body: some View {
            VStack {
                Text("\(self.title): \(detail)")
                Toggle("Expanded", isOn: $expanded)
            }
        }
    }

    StatusRow(title: "Build")

Linux synthesizes labeled memberwise arguments for stored properties, applies
stored-property defaults, exposes direct and `self.member` reads, evaluates the
computed `body`, and gives nested `@State` the caller's stable identity path.
Required properties without a default must be supplied. Custom initializers,
methods, generic stored content, protocol behavior beyond recognizing `View`,
and `@Binding` properties are not interpreted yet.

Enums can drive custom-view branches, helper return values, actions, state, and
picker tags:

    enum Status: String {
        case idle
        case running
        case failed(message: String)
    }

    struct StatusRow: View {
        let status: Status

        var body: some View {
            switch status {
            case .idle:
                Text("Idle")
            case .running:
                Text("Running")
            case let .failed(message):
                Text("Failed: \(message)")
            }
        }
    }

Supported switch patterns include scalar literals, numeric ranges, multiple
comma-separated patterns, enum cases, associated-value `let` bindings,
qualified case names, `where`, and `default`. String-backed enums expose
implicit or explicit `rawValue` values and can be persisted in `@State`.
Custom enum methods/computed properties, `init?(rawValue:)`, recursive nested
patterns, and compiler-style exhaustiveness diagnostics remain unsupported.

## Drag-and-drop reordering (persisted)

Drag-and-drop is achieved with `Reorderable`. This is the supported way to make
a list draggable, do not reach for `List`/`.onMove`/`.draggable` directly. Wrap
rows in `Reorderable`; the rows become draggable and dropping one onto another
runs the `move` command, which both reorders and persists (cmux remembers
workspace order):

    Reorderable(workspaces, move: "workspace.reorder") { w in
        Button(action: { cmux("workspace.select", workspace_id: w.id) }) {
            HStack { Text(w.title); Spacer() }.padding(6)
        }
    }

The dropped item's id and target index are sent as `workspace_id` and `index`.

## Two-column (Finder-style) example

    HSplitView {
        VStack(alignment: .leading) {
            for i in 0..<workspaces.count {
                Button(action: { cmux("workspace.select", workspace_id: workspaces[i].id) }) {
                    HStack { Image(systemName: "folder.fill"); Text(workspaces[i].title); Spacer() }.padding(4)
                }
            }
        }
        VStack(alignment: .leading) {
            for i in 0..<workspaces.count {
                if workspaces[i].selected {
                    for j in 0..<workspaces[i].tabs.count {
                        Button(action: { cmux("surface.focus", surface_id: workspaces[i].tabs[j].id) }) {
                            HStack { Image(systemName: "doc.text"); Text(workspaces[i].tabs[j].title); Spacer() }.padding(4)
                        }
                    }
                }
            }
        }
    }

## Not yet supported

The interpreter is a growing subset. `.overlay`/`.background`/`.mask`/
`.contextMenu` with arbitrary nested views, `Menu`, `List`/`Section`/grids,
shape `.stroke`/`.trim`, and user `func` helpers are all supported now.

Still missing: custom initializers, generic `@ViewBuilder` containers, custom
type methods/computed properties, optional/guard control flow, and composed
`@Binding` parameters; navigation
(`sheet`/`popover`/`NavigationStack`); `.keyboardShortcut`; and `AsyncImage`. Workspace
data (git branch/dirty, ports, PR, unread, remote, latest agent/prompt messages)
is live; data cmux doesn't track (custom domain collections) won't appear.

If your sidebar needs a missing feature, write it the natural Swift way anyway —
unsupported syntax is skipped (and even deeply nested or pathological source is
rendered best-effort, never crashes) — and ask for the feature.

## Performance and lazy loading

The sidebar re-evaluates roughly once a second (so clocks and data stay live),
and it renders rows eagerly. Keep each render cheap and the list bounded:

- Cap long lists. Show what fits and slice the rest: `for w in workspaces.prefix(20) { ... }`
  or `ForEach(items.prefix(50)) { ... }`. Do not render hundreds of rows.
- Filter/sort to what matters before rendering (`workspaces.filter { ... }`,
  `.sorted()`) rather than rendering everything and hiding most of it.
- Only render detail for the selected item. In a two-column layout, build the
  right column from the selected workspace's tabs, not every workspace's tabs.
- Prefer one focused sidebar over a giant catch-all; deep nesting and huge
  trees cost the most per tick.

## Tips

- Prefer `ForEach`/`Reorderable` over index loops where you can.
- Errors show inline in the sidebar with the failing location; fix and save.
- Keep modifier arguments simple literals or tokens.
- The JSON form is good for static layouts; use Swift for anything dynamic.
