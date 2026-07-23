# Custom sidebar interpreter: `@State` engine design

The leaf-tier surface (views, modifiers, shapes, gradients, value methods,
arbitrary-child modifiers) and the interaction engine are implemented on Linux.
This document records the state/binding/control design that converted the
renderer from one-shot to interactive. Stable state identity now works in
dynamic helper and simple stored-property custom `View` instances; the
remaining frontier is the advanced type system and composed bindings. Tagged
enum values also round-trip through the same state store and can drive
`switch`-selected controls and actions.

## Linux implementation status

The Linux port now ships the control portion of stages S1 through S3:

- A bounded host-owned state bag is persisted privately per sidebar provider.
- In-process and isolated-worker evaluation use the same state protocol.
- Top-level keys remain source-compatible, while helper state in `ForEach`,
  `Reorderable`, and ordinary `for` loops receives deterministic per-instance
  keys that survive reorder and app restart.
- Removed generated instances are pruned; active state is bounded to 256 values
  and identity nesting to 32 scopes.
- Button actions support `=`, `+=`, `-=`, `.toggle()`, and `.append(...)`.
- State writes trigger a new document generation.
- Native GTK `Toggle`, `TextField`, `Slider`, `Picker`, and `Stepper` controls
  write through `$bindings`; picker selections retain typed `.tag` values.
- `.onChange(of:)` re-evaluates after matching state writes with old/new closure
  parameters, and inherited `.onSubmit` handlers fire from GTK text fields.
- `cmux sidebar clear-state [name]` resets persisted values.

The remaining state work is composed `@Binding` projection into collection
elements and custom `View` parameters. Custom views already inherit the same
identity path for their own `@State`.

## What exists today

The Linux interpreter now evaluates against a host-owned mutable state bag and
returns a neutral document with typed bindings and state operations. The app
persists that bag, applies action/control writes, and re-evaluates the source.
The original one-shot path remains available for sources with no state.

Top-level declarations use their original variable names as persisted keys.
Nested declarations combine their declaration site with the enclosing
collection and helper-call identity path. This keeps independent row values
stable while preserving existing top-level state files.

## The four pieces

1. **A mutable state bag, keyed by `@State` declaration site.**
   - Parse `@State private var name = <initial>` declarations at the top of the
     sidebar (and inside custom views, later). On first walk, seed the bag with
     the evaluated initial value, keyed by a **stable id** = the declaration's
     source location (`name` is sufficient at top level; for per-instance state
     inside `ForEach`/custom views use `name` + the enclosing identity path).
   - The bag is owned by the **host** (`CustomSidebarModel` / a new
     `SidebarStateStore` `@Observable`), NOT rebuilt each walk — it must survive
     re-interpretation. `evaluate` takes it as an `inout`/reference parameter.
   - `Environment.lookup(name)` reads the bag for `@State` names (falling back to
     the read-only data context).

2. **`$binding` values.** Add `SwiftValue.binding(get:set:)` (or a `RenderNode`
   binding field) carrying a stable key into the bag. `$name` in source resolves
   to a binding over `bag[key]`. A control bound to `$name` reads `bag[key]` for
   its value and writes back through the binding's setter.

3. **An action executor.** Generalize `ButtonAction` beyond `[ActionCommand]` to
   also carry **assignments**: `name = expr`, `name.toggle()`, `name += n`,
   `name.append(x)`. `parseAction` captures these as structured ops; on tap the
   executor evaluates the RHS against the current env+bag, writes the bag, and
   **requests a re-walk**. `cmux(...)` keeps flowing to the host dispatcher.

4. **Re-walk on change + input control kinds.**
   - When the bag changes (control edit or action assignment), the host
     re-invokes `evaluate` with the same bag → new `RenderNode` tree → SwiftUI
     diffs it. This is the existing TimelineView path, now also triggered by
     state changes (an `@Observable` bag the host view observes).
   - New kinds: `textField` (binding + placeholder), `toggle` (binding + label),
     `slider` (binding + range), `picker` (binding + options), `stepper`. Each
     stores its binding key; `RenderNodeView` renders the real control with a
     SwiftUI `Binding` whose get/set go through the host bag + re-walk.

## Suggested staging

- **S1 — state bag + read/`$` + assignment actions (no controls yet):** prove a
  `Button("inc") { count += 1 }` + `Text("\(count)")` round-trips and re-renders.
  Smallest end-to-end slice of the engine.
- **S2 — `Toggle`/`TextField`** bound to `$state` (the two highest-value
  controls). Dogfood typing/toggling.
- **S3 — `Slider`/`Picker`/`Stepper`** + `.onChange`/`on(event:)` author hooks.
  The controls, `.onChange`, and `.onSubmit` are shipped.

## Constraints / gotchas

- Re-entrancy: a state write during a walk must not recurse the walk; mutate the
  bag, then schedule one coalesced re-walk (mirror the existing TimelineView
  cadence; do not sleep).
- Snapshot-boundary rule (CLAUDE.md): the bag is an `@Observable` the host view
  observes; rows still receive value snapshots, not the store.
- Dynamic identity must remain deterministic and bounded. Prefer explicit
  `ForEach(..., id:)` values or an item `id`; value-based fallback identity is
  appropriate only when the value itself is stable.
- Prune only generated instance keys. User-facing top-level keys must survive
  temporary branches that do not render them.
- Keep the one-shot path working when no `@State` is present (zero overhead).

## Touch-points

`RenderNode` (binding field / control kinds + `value`), `SwiftValue` (`.binding`),
`Environment` (bag reference + `$` resolution), `SwiftViewInterpreter`
(`@State` decl parse, control constructors, assignment capture in `parseAction`),
`RenderNodeView` (control rendering with host-backed `Binding`),
`CustomSidebarView`/`CustomSidebarModel` (own the `@Observable` bag, re-walk on
change), `SidebarActionDispatch` (carry assignment ops alongside commands).
