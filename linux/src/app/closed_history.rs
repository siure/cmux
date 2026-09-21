use super::*;

const MAX_RECENTLY_CLOSED_ITEMS: usize = 100;
const MAX_RECENTLY_CLOSED_BYTES: usize = 8 * 1024 * 1024;
const MAX_CLOSED_ITEM_SCROLLBACK_BYTES: usize = 2 * 1024 * 1024;

pub(super) type HistoryRecords = VecDeque<Arc<ClosedRecord>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ClosedRecord {
    id: String,
    item: ClosedItem,
    #[serde(skip)]
    retained_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct HistorySnapshot {
    #[serde(
        serialize_with = "serialize_records",
        deserialize_with = "deserialize_records"
    )]
    records: HistoryRecords,
    windows: Vec<(String, Vec<(String, PaneIdentities)>)>,
    #[serde(default)]
    group_anchors: Vec<(String, String)>,
}

fn serialize_records<S: serde::Serializer>(
    records: &HistoryRecords,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    records
        .iter()
        .map(Arc::as_ref)
        .collect::<Vec<_>>()
        .serialize(serializer)
}

fn deserialize_records<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HistoryRecords, D::Error> {
    let records = Vec::<ClosedRecord>::deserialize(deserializer)?;
    Ok(records
        .into_iter()
        .map(|mut record| {
            record.retained_bytes = retained_bytes(&record.item);
            Arc::new(record)
        })
        .collect())
}

fn retained_bytes(item: &ClosedItem) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(128);
    let _ = serde_json::to_writer(&mut counter, item);
    counter.0
}

impl ClosedItem {
    fn for_each_surface_mut(&mut self, mut apply: impl FnMut(&mut SessionSurfaceSnapshot)) {
        fn visit(
            workspace: &mut SessionWorkspaceSnapshot,
            apply: &mut impl FnMut(&mut SessionSurfaceSnapshot),
        ) {
            for pane in &mut workspace.panes {
                for surface in &mut pane.surfaces {
                    apply(surface);
                }
            }
        }
        match self {
            Self::Panel(panel) => apply(&mut panel.snapshot),
            Self::Workspace(workspace) => visit(&mut workspace.snapshot, &mut apply),
            Self::Window(window) => {
                for workspace in &mut window.snapshot.workspaces {
                    visit(workspace, &mut apply);
                }
            }
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Panel(panel) => &panel.snapshot.title,
            Self::Workspace(workspace) => &workspace.snapshot.title,
            Self::Window(window) => &window.snapshot.title,
        }
    }

    fn kind(&self) -> &str {
        match self {
            Self::Panel(_) => "panel",
            Self::Workspace(_) => "workspace",
            Self::Window(_) => "window",
        }
    }
}

type PaneIdentities = Vec<(String, Vec<String>)>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum ClosedItem {
    Panel(ClosedPanel),
    Workspace(ClosedWorkspace),
    Window(ClosedWindow),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ClosedPanel {
    workspace_id: String,
    surface_id: String,
    pane_id: String,
    tab_index: usize,
    pane_anchor_surface_id: Option<String>,
    fallback_anchor_pane_id: Option<String>,
    fallback_split_direction: Option<String>,
    snapshot: SessionSurfaceSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ClosedWorkspace {
    workspace_id: String,
    window_id: String,
    index: usize,
    group_id: Option<String>,
    pane_identities: PaneIdentities,
    snapshot: SessionWorkspaceSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ClosedWindow {
    window_id: String,
    workspace_identities: Vec<(String, PaneIdentities)>,
    #[serde(default)]
    group_anchors: Vec<(String, String)>,
    snapshot: SessionWindowSnapshot,
}

// Only topology and selection are copied. Existing surfaces and their live PTYs
// stay in place; a failed restore disposes exclusively of newly created objects.
struct RestoreCheckpoint {
    windows: Vec<Window>,
    workspaces: HashMap<String, RestoredWorkspaceLayout>,
    panes: HashMap<String, Pane>,
    surface_ids: HashSet<String>,
    current_window: String,
    history: HistoryRecords,
    last_workspace_by_window: HashMap<String, String>,
    focus_history_by_window: HashMap<String, FocusHistoryState>,
    sidebar_selection: HashMap<String, SidebarWorkspaceSelectionState>,
    command_palettes: HashMap<String, CommandPaletteState>,
    canvas_states: HashMap<String, CanvasWorkspaceState>,
}

struct RestoredWorkspaceLayout {
    panes: Vec<String>,
    selected_pane: Option<String>,
    column_sizes: Vec<f64>,
    row_sizes: Vec<f64>,
    browser_profile: Option<String>,
}

impl RestoreCheckpoint {
    fn capture(app: &AppState) -> Self {
        Self {
            windows: app.windows.clone(),
            workspaces: app
                .workspaces
                .iter()
                .map(|(id, workspace)| {
                    (
                        id.clone(),
                        RestoredWorkspaceLayout {
                            panes: workspace.panes.clone(),
                            selected_pane: workspace.selected_pane.clone(),
                            column_sizes: workspace.debug_column_sizes.clone(),
                            row_sizes: workspace.debug_row_sizes.clone(),
                            browser_profile: workspace.preferred_browser_profile_id.clone(),
                        },
                    )
                })
                .collect(),
            panes: app.panes.clone(),
            surface_ids: app.surfaces.keys().cloned().collect(),
            current_window: app.current_window.clone(),
            history: app.recently_closed_items.clone(),
            last_workspace_by_window: app.last_workspace_by_window.clone(),
            focus_history_by_window: app.focus_history_by_window.clone(),
            sidebar_selection: app.sidebar_workspace_selection_by_window.clone(),
            command_palettes: app.command_palettes.clone(),
            canvas_states: app.canvas_states.clone(),
        }
    }

    fn rollback(self, app: &mut AppState) {
        let created_surfaces = app
            .surfaces
            .keys()
            .filter(|id| !self.surface_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in created_surfaces {
            if let Some(runtime) = app.agent_session_runtimes.remove(&id) {
                let _ = runtime.stop();
            }
            if let Some(surface) = app.surfaces.remove(&id) {
                if let Some(terminal) = surface.terminal {
                    let _ = terminal.kill();
                }
            }
            app.project_panels.remove(&id);
            app.tab_ref_aliases.retain(|_, surface| surface != &id);
        }
        let created_workspaces = app
            .workspaces
            .keys()
            .filter(|id| !self.workspaces.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in created_workspaces {
            app.remove_workspace(&id);
        }
        let created_windows = app
            .windows
            .iter()
            .filter(|window| !self.windows.iter().any(|old| old.id == window.id))
            .map(|window| window.id.clone())
            .collect::<Vec<_>>();
        for id in created_windows {
            let _ = app.close_window(&json!({"window_id": id, "record_history": false}));
        }
        for (id, layout) in self.workspaces {
            if let Some(workspace) = app.workspaces.get_mut(&id) {
                workspace.panes = layout.panes;
                workspace.selected_pane = layout.selected_pane;
                workspace.debug_column_sizes = layout.column_sizes;
                workspace.debug_row_sizes = layout.row_sizes;
                workspace.preferred_browser_profile_id = layout.browser_profile;
            }
        }
        app.resize_attachments
            .retain(|id, _| self.panes.contains_key(id));
        app.panes = self.panes;
        app.windows = self.windows;
        app.current_window = self.current_window;
        app.recently_closed_items = self.history;
        app.last_workspace_by_window = self.last_workspace_by_window;
        app.focus_history_by_window = self.focus_history_by_window;
        app.sidebar_workspace_selection_by_window = self.sidebar_selection;
        app.command_palettes = self.command_palettes;
        app.canvas_states = self.canvas_states;
        // A late resize failure may already have resized another existing pane.
        for id in app.workspaces.keys().cloned().collect::<Vec<_>>() {
            let _ = app.apply_workspace_terminal_sizes(&id);
        }
    }
}

fn prepare_closed_environment(env: &mut HashMap<String, String>) {
    env.retain(|key, _| {
        !matches!(key.as_str(), "CMUX_SOCKET" | "CMUX_SOCKET_PATH")
            && !key.starts_with("CMUX_REMOTE_TMUX_")
    });
}

// Reopening launches a fresh shell. Saved commands and partially submitted input
// are display/history data, never instructions to execute again.
fn prepare_closed_surface(snapshot: &mut SessionSurfaceSnapshot) {
    prepare_closed_environment(&mut snapshot.terminal_env);
    snapshot.terminal_command = None;
    snapshot.terminal_initial_input = None;
    snapshot.terminal_wait_after_command = false;
    snapshot.remote_session_active = false;
    snapshot.ssh_session_id = None;
    snapshot.resume_binding = None;
    snapshot.agent_hibernation = None;
    if let Some(agent) = snapshot.agent_session.as_mut() {
        agent.status = "idle".to_string();
        agent.was_running_at_snapshot = false;
    }
}

fn closed_scrollback_tail(surface: &Surface, remaining_bytes: &mut usize) -> Option<String> {
    fn take_tail(text: &str, remaining_bytes: &mut usize) -> Option<String> {
        let limit = (*remaining_bytes).min(SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT);
        if limit == 0 || text.is_empty() {
            return None;
        }
        let mut start = text.len().saturating_sub(limit);
        while !text.is_char_boundary(start) {
            start += 1;
        }
        let tail = text[start..].to_string();
        *remaining_bytes = remaining_bytes.saturating_sub(tail.len());
        Some(tail)
    }
    if let Some(text) = surface.terminal_scrollback_snapshot.as_deref() {
        return take_tail(text, remaining_bytes);
    }
    let buffer = surface.buffer.lock().ok()?;
    take_tail(&buffer, remaining_bytes)
}

fn prepare_closed_workspace(snapshot: &mut SessionWorkspaceSnapshot) {
    prepare_closed_environment(&mut snapshot.workspace_env);
    for pane in &mut snapshot.panes {
        for surface in &mut pane.surfaces {
            prepare_closed_surface(surface);
        }
    }
}

impl AppState {
    fn push_closed_item(&mut self, mut item: ClosedItem) {
        let mut retained_bytes = retained_bytes(&item);
        if retained_bytes > MAX_RECENTLY_CLOSED_BYTES {
            item.for_each_surface_mut(|surface| surface.scrollback = None);
            retained_bytes = self::retained_bytes(&item);
        }
        if retained_bytes > MAX_RECENTLY_CLOSED_BYTES {
            return;
        }
        self.recently_closed_items.push_back(Arc::new(ClosedRecord {
            id: new_id(),
            item,
            retained_bytes,
        }));
        self.trim_closed_history();
    }

    fn trim_closed_history(&mut self) {
        let mut bytes: usize = self
            .recently_closed_items
            .iter()
            .map(|record| record.retained_bytes)
            .sum();
        while self.recently_closed_items.len() > MAX_RECENTLY_CLOSED_ITEMS
            || bytes > MAX_RECENTLY_CLOSED_BYTES
        {
            let Some(record) = self.recently_closed_items.pop_front() else {
                break;
            };
            bytes = bytes.saturating_sub(record.retained_bytes);
        }
    }

    pub(super) fn closed_history_snapshot(&self) -> Option<HistorySnapshot> {
        if self.recently_closed_items.is_empty() {
            return None;
        }
        Some(HistorySnapshot {
            // Autosaves share the immutable history payloads, including scrollback.
            records: self.recently_closed_items.clone(),
            group_anchors: self
                .workspace_groups
                .values()
                .map(|group| (group.id.clone(), group.anchor_workspace_id.clone()))
                .collect(),
            windows: self
                .windows
                .iter()
                .map(|window| {
                    (
                        window.id.clone(),
                        window
                            .workspaces
                            .iter()
                            .filter_map(|id| self.workspaces.get(id))
                            .map(|workspace| {
                                (
                                    workspace.id.clone(),
                                    self.closed_workspace_pane_identities(workspace),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        })
    }

    pub(super) fn restore_closed_history_snapshot(
        &mut self,
        history: Option<HistorySnapshot>,
        window_ids: &[String],
    ) {
        self.recently_closed_items.clear();
        let Some(history) = history else {
            return;
        };
        self.recently_closed_items = history.records;
        self.trim_closed_history();
        let mut workspace_map = HashMap::new();
        for ((old_window, old_workspaces), new_window) in history.windows.iter().zip(window_ids) {
            let Some(window) = self.windows.iter().find(|window| &window.id == new_window) else {
                continue;
            };
            let workspace_ids = window.workspaces.clone();
            for ((old_id, panes), new_id) in old_workspaces.iter().zip(&workspace_ids) {
                self.remap_closed_workspace(old_id, new_id, panes);
                workspace_map.insert(old_id.clone(), new_id.clone());
            }
            self.remap_closed_window(old_window, new_window);
        }
        self.remap_closed_groups(&history.group_anchors, &workspace_map);
    }

    pub(super) fn closed_history_list(&self) -> Value {
        json!({"entries": self.recently_closed_items.iter().rev().map(|record|
            json!({"id": record.id, "kind": record.item.kind(), "title": record.item.title()})
        ).collect::<Vec<_>>()})
    }

    pub(super) fn clear_closed_history(&mut self) -> Value {
        let cleared = self.recently_closed_items.len();
        self.recently_closed_items.clear();
        json!({"cleared": cleared})
    }

    pub(super) fn closed_history_command_rows(&self) -> Vec<Value> {
        let mut rows = self.recently_closed_items.iter().rev().map(|record| json!({
            "command_id": format!("palette.reopenClosedItem.{}", record.id),
            "title": format!("Reopen {}", record.item.title()), "title_key": "history.reopen_item",
            "title_arg": record.item.title(), "type": "command", "api_method": "history.reopen",
            "shortcut_hint": "", "shortcut_label": ""
        })).collect::<Vec<_>>();
        if !rows.is_empty() {
            rows.push(json!({"command_id": "palette.clearClosedHistory", "title": "Clear Recently Closed History",
                "title_key": "history.clear", "type": "command", "api_method": "history.clear",
                "shortcut_hint": "", "shortcut_label": ""}));
        }
        rows
    }

    fn capture_closed_workspace_scrollback(
        &self,
        workspace: &Workspace,
        snapshot: &mut SessionWorkspaceSnapshot,
        remaining_bytes: &mut usize,
    ) {
        for (pane, saved_pane) in workspace
            .panes
            .iter()
            .filter_map(|id| self.panes.get(id))
            .zip(&mut snapshot.panes)
        {
            for (surface, saved_surface) in pane
                .surfaces
                .iter()
                .filter_map(|id| self.surfaces.get(id))
                .zip(&mut saved_pane.surfaces)
            {
                if surface.agent_hibernation.is_none() {
                    saved_surface.scrollback = closed_scrollback_tail(surface, remaining_bytes);
                }
            }
        }
    }

    fn closed_workspace_pane_identities(&self, workspace: &Workspace) -> PaneIdentities {
        workspace
            .panes
            .iter()
            .filter_map(|id| self.panes.get(id))
            .map(|pane| (pane.id.clone(), pane.surfaces.clone()))
            .collect()
    }

    pub(super) fn capture_closed_panel(
        &mut self,
        surface_id: &str,
        workspace_id: &str,
        pane_id: &str,
    ) {
        let Some(surface) = self.surfaces.get(surface_id) else {
            return;
        };
        let Some(pane) = self.panes.get(pane_id) else {
            return;
        };
        let tab_index = pane
            .surfaces
            .iter()
            .position(|id| id == surface_id)
            .unwrap_or(0);
        let pane_anchor_surface_id = pane
            .surfaces
            .get(tab_index + 1)
            .or_else(|| {
                tab_index
                    .checked_sub(1)
                    .and_then(|index| pane.surfaces.get(index))
            })
            .cloned();
        let (fallback_anchor_pane_id, fallback_split_direction) =
            self.closed_browser_fallback_split(workspace_id, pane_id);
        let mut snapshot = self.session_surface_snapshot(surface, false);
        if surface.agent_hibernation.is_none() {
            let mut remaining_bytes = MAX_CLOSED_ITEM_SCROLLBACK_BYTES;
            snapshot.scrollback = closed_scrollback_tail(surface, &mut remaining_bytes);
        }
        prepare_closed_surface(&mut snapshot);
        self.push_closed_item(ClosedItem::Panel(ClosedPanel {
            workspace_id: workspace_id.to_string(),
            surface_id: surface_id.to_string(),
            pane_id: pane_id.to_string(),
            tab_index,
            pane_anchor_surface_id,
            fallback_anchor_pane_id,
            fallback_split_direction,
            snapshot,
        }));
    }

    pub(super) fn capture_closed_workspace(&mut self, workspace_id: &str, index: usize) {
        let Some(workspace) = self.workspaces.get(workspace_id) else {
            return;
        };
        let mut snapshot = self.session_workspace_snapshot(workspace, false);
        let mut remaining_bytes = MAX_CLOSED_ITEM_SCROLLBACK_BYTES;
        self.capture_closed_workspace_scrollback(workspace, &mut snapshot, &mut remaining_bytes);
        prepare_closed_workspace(&mut snapshot);
        let item = ClosedWorkspace {
            workspace_id: workspace_id.to_string(),
            window_id: workspace.window_id.clone(),
            index,
            group_id: workspace.group_id.clone(),
            pane_identities: self.closed_workspace_pane_identities(workspace),
            snapshot,
        };
        self.push_closed_item(ClosedItem::Workspace(item));
    }

    pub(super) fn capture_closed_window(&mut self, index: usize) {
        let Some(window) = self.windows.get(index) else {
            return;
        };
        let mut snapshot = self.session_window_snapshot(window, false);
        let mut remaining_bytes = MAX_CLOSED_ITEM_SCROLLBACK_BYTES;
        for (workspace, saved_workspace) in window
            .workspaces
            .iter()
            .filter_map(|id| self.workspaces.get(id))
            .zip(&mut snapshot.workspaces)
        {
            self.capture_closed_workspace_scrollback(
                workspace,
                saved_workspace,
                &mut remaining_bytes,
            );
            prepare_closed_workspace(saved_workspace);
        }
        let item = ClosedWindow {
            window_id: window.id.clone(),
            group_anchors: self
                .workspace_groups
                .values()
                .filter(|group| group.window_id == window.id)
                .map(|group| (group.id.clone(), group.anchor_workspace_id.clone()))
                .collect(),
            workspace_identities: window
                .workspaces
                .iter()
                .filter_map(|id| self.workspaces.get(id))
                .map(|workspace| {
                    (
                        workspace.id.clone(),
                        self.closed_workspace_pane_identities(workspace),
                    )
                })
                .collect(),
            snapshot,
        };
        self.push_closed_item(ClosedItem::Window(item));
    }

    fn remap_closed_panel_surface(&mut self, old_id: &str, new_id: &str) {
        for record in &mut self.recently_closed_items {
            let item = &mut Arc::make_mut(record).item;
            if let ClosedItem::Panel(panel) = item {
                if panel.pane_anchor_surface_id.as_deref() == Some(old_id) {
                    panel.pane_anchor_surface_id = Some(new_id.to_string());
                }
            }
        }
    }

    fn remap_closed_workspace(&mut self, old_id: &str, new_id: &str, old_panes: &PaneIdentities) {
        let Some(workspace) = self.workspaces.get(new_id) else {
            return;
        };
        let new_panes = self.closed_workspace_pane_identities(workspace);
        let pane_map: HashMap<_, _> = old_panes
            .iter()
            .zip(&new_panes)
            .map(|((old, _), (new, _))| (old.clone(), new.clone()))
            .collect();
        let surface_map: HashMap<_, _> = old_panes
            .iter()
            .zip(&new_panes)
            .flat_map(|((_, old), (_, new))| {
                old.iter()
                    .zip(new)
                    .map(|(old, new)| (old.clone(), new.clone()))
            })
            .collect();
        for record in &mut self.recently_closed_items {
            let item = &mut Arc::make_mut(record).item;
            let ClosedItem::Panel(panel) = item else {
                continue;
            };
            if panel.workspace_id != old_id {
                continue;
            }
            panel.workspace_id = new_id.to_string();
            if let Some(new) = pane_map.get(&panel.pane_id) {
                panel.pane_id = new.clone();
            }
            if let Some(new) = panel
                .fallback_anchor_pane_id
                .as_ref()
                .and_then(|old| pane_map.get(old))
            {
                panel.fallback_anchor_pane_id = Some(new.clone());
            }
            if let Some(new) = panel
                .pane_anchor_surface_id
                .as_ref()
                .and_then(|old| surface_map.get(old))
            {
                panel.pane_anchor_surface_id = Some(new.clone());
            }
        }
    }

    // Keep the established action identifier so existing shortcut configuration,
    // command palette and socket clients all use the same expanded history.
    pub(super) fn reopen_closed_browser_panel(&mut self) -> AppResult<Value> {
        for index in (0..self.recently_closed_items.len()).rev() {
            if let Some(result) = self.reopen_closed_history_index(index)? {
                return Ok(result);
            }
        }
        Ok(
            json!({"handled": false, "action": "reopenClosedBrowserPanel", "reason": "history_empty"}),
        )
    }

    pub(super) fn reopen_closed_history_id(&mut self, id: &str) -> AppResult<Value> {
        let Some(index) = self
            .recently_closed_items
            .iter()
            .position(|record| record.id == id)
        else {
            return Ok(json!({"handled": false, "reason": "history_entry_not_found"}));
        };
        Ok(self
            .reopen_closed_history_index(index)?
            .unwrap_or_else(|| json!({"handled": false, "reason": "history_entry_unavailable"})))
    }

    fn reopen_closed_history_index(&mut self, index: usize) -> AppResult<Option<Value>> {
        let mut item = self.recently_closed_items[index].item.clone();
        let mut available = true;
        item.for_each_surface_mut(|surface| {
            // Persisted snapshots are data too: never replay a former command.
            prepare_closed_surface(surface);
            if surface.kind == "browser" && !self.browser_enabled {
                available = false;
            }
            if let Some(browser) = surface.browser.as_mut() {
                if !self.browser_profiles.contains_key(&browser.profile_id) {
                    browser.profile_id = BROWSER_BUILT_IN_DEFAULT_PROFILE_ID.to_string();
                }
            }
        });
        if !available {
            return Ok(None);
        }
        match &mut item {
            ClosedItem::Workspace(workspace) => prepare_closed_workspace(&mut workspace.snapshot),
            ClosedItem::Window(window) => {
                for workspace in &mut window.snapshot.workspaces {
                    prepare_closed_workspace(workspace);
                }
            }
            ClosedItem::Panel(_) => {}
        }
        if let Some(socket_path) = self.local_socket_path.as_ref() {
            item.for_each_surface_mut(|surface| {
                if surface.kind == "terminal" {
                    // Override both aliases, including any inherited by the child
                    // process from the app's own launch environment.
                    surface
                        .terminal_env
                        .insert("CMUX_SOCKET_PATH".into(), socket_path.clone());
                    surface
                        .terminal_env
                        .insert("CMUX_SOCKET".into(), socket_path.clone());
                }
            });
        }
        let checkpoint = RestoreCheckpoint::capture(self);
        let result = match item {
            ClosedItem::Panel(panel) => self.restore_closed_panel(panel),
            ClosedItem::Workspace(workspace) => self.restore_closed_workspace(workspace).map(Some),
            ClosedItem::Window(window) => self.restore_closed_window(window).map(Some),
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                checkpoint.rollback(self);
                return Err(error);
            }
        };
        if let Some(mut result) = result {
            self.recently_closed_items.remove(index);
            result["handled"] = json!(true);
            result["action"] = json!("reopenClosedBrowserPanel");
            return Ok(Some(result));
        }
        Ok(None)
    }

    fn remap_closed_pane(&mut self, old_id: &str, new_id: &str) {
        for record in &mut self.recently_closed_items {
            let ClosedItem::Panel(panel) = &mut Arc::make_mut(record).item else {
                continue;
            };
            if panel.pane_id == old_id {
                panel.pane_id = new_id.to_string();
            }
            if panel.fallback_anchor_pane_id.as_deref() == Some(old_id) {
                panel.fallback_anchor_pane_id = Some(new_id.to_string());
            }
        }
    }

    fn remap_closed_groups(
        &mut self,
        group_anchors: &[(String, String)],
        workspace_map: &HashMap<String, String>,
    ) {
        let group_map = group_anchors
            .iter()
            .filter_map(|(group_id, anchor)| {
                let anchor_id = workspace_map.get(anchor)?;
                let restored = self
                    .workspace_groups
                    .values()
                    .find(|group| &group.anchor_workspace_id == anchor_id)?;
                Some((group_id.clone(), restored.id.clone()))
            })
            .collect::<HashMap<_, _>>();
        for record in &mut self.recently_closed_items {
            if let ClosedItem::Workspace(workspace) = &mut Arc::make_mut(record).item {
                if let Some(restored) = workspace.group_id.as_ref().and_then(|id| group_map.get(id))
                {
                    workspace.group_id = Some(restored.clone());
                }
            }
        }
    }

    fn remap_closed_window(&mut self, old_id: &str, new_id: &str) {
        for record in &mut self.recently_closed_items {
            if let ClosedItem::Workspace(workspace) = &mut Arc::make_mut(record).item {
                if workspace.window_id == old_id {
                    workspace.window_id = new_id.to_string();
                }
            }
        }
    }

    fn restore_closed_panel(&mut self, entry: ClosedPanel) -> AppResult<Option<Value>> {
        if !self.workspaces.contains_key(&entry.workspace_id)
            || (entry.snapshot.kind == "browser" && !self.browser_enabled)
        {
            return Ok(None);
        }
        let workspace_id = entry.workspace_id;
        let original_pane_is_live = self
            .panes
            .get(&entry.pane_id)
            .is_some_and(|pane| pane.workspace_id == workspace_id);
        let pane_id = if original_pane_is_live {
            entry.pane_id.clone()
        } else if let Some(pane_id) = entry
            .pane_anchor_surface_id
            .as_ref()
            .and_then(|id| self.surfaces.get(id))
            .filter(|surface| {
                self.panes
                    .get(&surface.pane_id)
                    .is_some_and(|pane| pane.workspace_id == workspace_id)
            })
            .map(|surface| surface.pane_id.clone())
        {
            pane_id
        } else if let Some((anchor, direction)) = entry
            .fallback_anchor_pane_id
            .as_deref()
            .zip(entry.fallback_split_direction.as_deref())
            .filter(|(anchor, _)| {
                self.panes
                    .get(*anchor)
                    .is_some_and(|pane| pane.workspace_id == workspace_id)
            })
        {
            self.create_split_pane(&workspace_id, anchor, direction)?
        } else {
            self.workspaces
                .get(&workspace_id)
                .and_then(|workspace| {
                    workspace
                        .selected_pane
                        .clone()
                        .or_else(|| workspace.panes.first().cloned())
                })
                .unwrap_or_else(|| self.create_pane(&workspace_id))
        };
        let snapshot = entry.snapshot;
        let spec = SurfaceSpec {
            kind: SurfaceKind::from_str(&snapshot.kind),
            title: Some(snapshot.title.clone()),
            custom_title: snapshot.custom_title,
            url: snapshot.url.clone(),
            browser_profile_id: snapshot
                .browser
                .as_ref()
                .map(|browser| browser.profile_id.clone()),
            cwd: snapshot.terminal_cwd.clone(),
            command: None,
            initial_input: None,
            font_size: snapshot.terminal_font_size,
            wait_after_command: false,
            env: snapshot.terminal_env.clone(),
            remote_session_active: false,
            ssh_session_id: None,
        };
        let title = snapshot.title.clone();
        let custom_title = snapshot.custom_title;
        let url = snapshot.url.clone();
        let surface_id = self.create_surface(&workspace_id, &pane_id, spec)?;
        self.restore_surface_snapshot_fields(&surface_id, snapshot, None)?;
        if let Some(surface) = self.surfaces.get_mut(&surface_id) {
            surface.title = title;
            surface.custom_title = custom_title;
        }
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.surfaces.retain(|id| id != &surface_id);
            pane.surfaces
                .insert(entry.tab_index.min(pane.surfaces.len()), surface_id.clone());
            pane.selected_surface = Some(surface_id.clone());
        }
        self.remap_closed_panel_surface(&entry.surface_id, &surface_id);
        if !original_pane_is_live {
            self.remap_closed_pane(&entry.pane_id, &pane_id);
        }
        self.focus_surface(&surface_id)?;
        self.apply_workspace_terminal_sizes(&workspace_id)?;
        let window_id = self.workspaces[&workspace_id].window_id.clone();
        Ok(Some(json!({
            "window_id": window_id, "window_ref": self.window_ref(&window_id),
            "workspace_id": workspace_id, "workspace_ref": self.workspace_ref(&workspace_id),
            "pane_id": pane_id, "pane_ref": self.pane_ref(&pane_id),
            "surface_id": surface_id, "surface_ref": self.surface_ref(&surface_id),
            "url": url, "restored_original_pane": original_pane_is_live
        })))
    }

    fn restore_closed_workspace(&mut self, entry: ClosedWorkspace) -> AppResult<Value> {
        let window_id = if self
            .windows
            .iter()
            .any(|window| window.id == entry.window_id)
        {
            entry.window_id
        } else {
            self.current_window.clone()
        };
        let workspace_id = self.restore_workspace_snapshot(&window_id, entry.snapshot)?;
        if let Some(group_id) = entry.group_id.filter(|id| {
            self.workspace_groups
                .get(id)
                .is_some_and(|group| group.window_id == window_id)
        }) {
            self.workspaces.get_mut(&workspace_id).unwrap().group_id = Some(group_id);
        }
        self.move_workspace_to_index_in_window(&window_id, &workspace_id, entry.index)?;
        self.normalize_workspace_group_contiguity(&window_id, None);
        self.remap_closed_workspace(&entry.workspace_id, &workspace_id, &entry.pane_identities);
        self.select_workspace_by_id(&workspace_id)?;
        if let Some(surface_id) = self.workspace_selected_surface(&workspace_id) {
            self.focus_surface(&surface_id)?;
        }
        self.apply_workspace_terminal_sizes(&workspace_id)?;
        Ok(
            json!({"window_id": window_id, "window_ref": self.window_ref(&window_id),
            "workspace_id": workspace_id, "workspace_ref": self.workspace_ref(&workspace_id)}),
        )
    }

    fn restore_closed_window(&mut self, entry: ClosedWindow) -> AppResult<Value> {
        self.append_session_snapshot(LinuxSessionSnapshot {
            version: SESSION_SNAPSHOT_VERSION,
            saved_at: 0.0,
            current_window_index: 0,
            windows: vec![entry.snapshot],
            closed_history: None,
        })?;
        let window_id = self.current_window.clone();
        let workspace_ids = self
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .unwrap()
            .workspaces
            .clone();
        for ((old_id, panes), new_id) in entry.workspace_identities.iter().zip(&workspace_ids) {
            self.remap_closed_workspace(old_id, new_id, panes);
        }
        self.remap_closed_window(&entry.window_id, &window_id);
        let workspace_map = entry
            .workspace_identities
            .iter()
            .zip(&workspace_ids)
            .map(|((old_id, _), new_id)| (old_id.clone(), new_id.clone()))
            .collect();
        self.remap_closed_groups(&entry.group_anchors, &workspace_map);
        if let Ok(surface_id) = self.current_surface_id() {
            self.focus_surface(&surface_id)?;
        }
        for workspace_id in &workspace_ids {
            self.apply_workspace_terminal_sizes(workspace_id)?;
        }
        Ok(json!({"window_id": window_id, "window_ref": self.window_ref(&window_id)}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_fixture(kind: &str) -> AppState {
        let mut app = AppState::with_paths_and_terminal_startup(
            None,
            Some("/tmp/cmux-history-current.sock".into()),
            TerminalStartupMode::RendererOwned,
        )
        .unwrap();
        match kind {
            "panel" => {
                let surface = app.current_surface_id().unwrap();
                let created = app
                    .handle(
                        "surface.split",
                        &json!({"surface_id": surface, "direction": "right"}),
                    )
                    .unwrap();
                app.handle(
                    "surface.close",
                    &json!({"surface_id": created["surface_id"]}),
                )
                .unwrap();
            }
            "workspace" => {
                let created = app
                    .handle("workspace.create", &json!({"title": "Restore target"}))
                    .unwrap();
                app.handle(
                    "workspace.close",
                    &json!({"workspace_id": created["workspace_id"]}),
                )
                .unwrap();
            }
            "window" => {
                let created = app.handle("window.create", &json!({})).unwrap();
                app.handle("window.close", &json!({"window_id": created["window_id"]}))
                    .unwrap();
            }
            _ => panic!("unknown fixture"),
        }
        app
    }

    #[test]
    fn failed_closed_restore_rolls_back_topology_focus_and_history_before_retry() {
        for kind in ["panel", "workspace", "window"] {
            let mut app = closed_fixture(kind);
            Arc::make_mut(app.recently_closed_items.back_mut().unwrap())
                .item
                .for_each_surface_mut(|surface| {
                    // Command spawning rejects a NUL in the saved environment.
                    surface
                        .terminal_env
                        .insert("RESTORE_TEST".into(), "invalid\0value".into());
                });
            app.terminal_startup_mode = TerminalStartupMode::CorePty;
            let existing_surface = app.current_surface_id().unwrap();
            app.ensure_surface_terminal_started(&existing_surface)
                .unwrap();
            let existing_terminal = app.surfaces[&existing_surface].terminal.clone().unwrap();
            let existing_pid = existing_terminal.pid();
            let before_windows = serde_json::to_value(app.session_snapshot(false).windows).unwrap();
            let before_history = serde_json::to_value(app.closed_history_snapshot()).unwrap();
            let before_current = app.current_window.clone();
            let before_shape = (
                app.windows.len(),
                app.workspaces.len(),
                app.panes.len(),
                app.surfaces.len(),
            );
            for _ in 0..2 {
                assert!(
                    app.handle("history.reopen_closed", &json!({})).is_err(),
                    "{kind}"
                );
                assert_eq!(
                    (
                        app.windows.len(),
                        app.workspaces.len(),
                        app.panes.len(),
                        app.surfaces.len()
                    ),
                    before_shape,
                    "{kind}"
                );
                assert_eq!(
                    serde_json::to_value(app.session_snapshot(false).windows).unwrap(),
                    before_windows,
                    "{kind}"
                );
                assert_eq!(
                    serde_json::to_value(app.closed_history_snapshot()).unwrap(),
                    before_history,
                    "{kind}"
                );
                assert_eq!(app.current_window, before_current, "{kind}");
                assert_eq!(
                    app.surfaces[&existing_surface]
                        .terminal
                        .as_ref()
                        .unwrap()
                        .pid(),
                    existing_pid
                );
                assert_eq!(existing_terminal.try_wait_exit().unwrap(), None);
                existing_terminal.send_text("true\n").unwrap();
            }
            Arc::make_mut(app.recently_closed_items.back_mut().unwrap())
                .item
                .for_each_surface_mut(|surface| {
                    surface.terminal_env.remove("RESTORE_TEST");
                });
            assert_eq!(
                app.handle("history.reopen_closed", &json!({})).unwrap()["handled"],
                true,
                "{kind}"
            );
            assert!(app.recently_closed_items.is_empty(), "{kind}");
        }
    }

    #[test]
    fn closed_restore_rolls_back_when_browser_settings_changed_on_disk() {
        for kind in ["panel", "workspace", "window"] {
            let mut app = closed_fixture(kind);
            let directory = tempfile::tempdir().unwrap();
            app.browser_settings_path = directory.path().join("browser.json");
            app.browser_enabled = true;
            browser_settings::save_enabled(&app.browser_settings_path, false).unwrap();
            Arc::make_mut(app.recently_closed_items.back_mut().unwrap())
                .item
                .for_each_surface_mut(|surface| {
                    surface.kind = "browser".into();
                });
            let before_windows = serde_json::to_value(app.session_snapshot(false).windows).unwrap();
            let before_history = serde_json::to_value(app.closed_history_snapshot()).unwrap();
            let before_shape = (
                app.windows.len(),
                app.workspaces.len(),
                app.panes.len(),
                app.surfaces.len(),
            );
            assert!(
                app.handle("history.reopen_closed", &json!({})).is_err(),
                "{kind}"
            );
            assert_eq!(
                (
                    app.windows.len(),
                    app.workspaces.len(),
                    app.panes.len(),
                    app.surfaces.len()
                ),
                before_shape,
                "{kind}"
            );
            assert_eq!(
                serde_json::to_value(app.session_snapshot(false).windows).unwrap(),
                before_windows,
                "{kind}"
            );
            assert_eq!(
                serde_json::to_value(app.closed_history_snapshot()).unwrap(),
                before_history,
                "{kind}"
            );
        }
    }

    #[test]
    fn closed_restore_rebinds_transport_environment_to_the_current_socket() {
        for kind in ["panel", "workspace", "window"] {
            let mut app = closed_fixture(kind);
            let old_env = HashMap::from([
                (
                    "CMUX_SOCKET_PATH".into(),
                    "/tmp/cmux-history-old.sock".into(),
                ),
                ("CMUX_SOCKET".into(), "/tmp/cmux-history-legacy.sock".into()),
                (
                    REMOTE_TMUX_CONNECTION_ENV.into(),
                    "former-connection".into(),
                ),
                (REMOTE_TMUX_MANUAL_IO_ENV.into(), "1".into()),
                (REMOTE_TMUX_WINDOW_ENV.into(), "1".into()),
                (REMOTE_TMUX_PANE_ENV.into(), "2".into()),
                ("CMUX_REMOTE_TMUX_HOST".into(), "old-host".into()),
                ("CMUX_REMOTE_TMUX_SESSION".into(), "old-session".into()),
                ("PROJECT_VARIABLE".into(), "preserved".into()),
            ]);
            let item = &mut Arc::make_mut(app.recently_closed_items.back_mut().unwrap()).item;
            item.for_each_surface_mut(|surface| surface.terminal_env = old_env.clone());
            match item {
                ClosedItem::Workspace(workspace) => {
                    workspace.snapshot.workspace_env = old_env.clone()
                }
                ClosedItem::Window(window) => {
                    for workspace in &mut window.snapshot.workspaces {
                        workspace.workspace_env = old_env.clone();
                    }
                }
                ClosedItem::Panel(_) => {}
            }
            let existing = app.surfaces.keys().cloned().collect::<HashSet<_>>();
            app.terminal_startup_mode = TerminalStartupMode::CorePty;
            assert_eq!(
                app.handle("history.reopen_closed", &json!({})).unwrap()["handled"],
                true,
                "{kind}"
            );
            let restored = app
                .surfaces
                .values()
                .filter(|surface| !existing.contains(&surface.id))
                .collect::<Vec<_>>();
            assert!(!restored.is_empty());
            for surface in restored {
                assert!(
                    surface.terminal.is_some(),
                    "{kind}: restored shell never started"
                );
                assert!(!surface.remote_session_active, "{kind}");
                assert_eq!(
                    surface
                        .terminal_env
                        .get("CMUX_SOCKET_PATH")
                        .map(String::as_str),
                    Some("/tmp/cmux-history-current.sock"),
                    "{kind}"
                );
                assert_eq!(
                    surface.terminal_env.get("CMUX_SOCKET"),
                    surface.terminal_env.get("CMUX_SOCKET_PATH"),
                    "{kind}"
                );
                let terminal = surface.terminal.as_ref().unwrap();
                let pid = terminal.pid().unwrap();
                let expected_env = [
                    b"CMUX_SOCKET_PATH=/tmp/cmux-history-current.sock".as_slice(),
                    b"CMUX_SOCKET=/tmp/cmux-history-current.sock".as_slice(),
                ];
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    // The child PID may exist before its final exec publishes
                    // the shell environment through procfs.
                    let ready = fs::read(format!("/proc/{pid}/environ")).is_ok_and(|process_env| {
                        expected_env.iter().all(|expected| {
                            process_env
                                .split(|byte| *byte == 0)
                                .any(|entry| entry == *expected)
                        })
                    });
                    if ready {
                        break;
                    }
                    assert_eq!(
                        terminal.try_wait_exit().unwrap(),
                        None,
                        "{kind}: shell exited before environment was ready"
                    );
                    assert!(
                        Instant::now() < deadline,
                        "{kind}: current socket aliases did not reach the child environment"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                assert!(
                    !surface
                        .terminal_env
                        .keys()
                        .any(|key| key.starts_with("CMUX_REMOTE_TMUX_")),
                    "{kind}"
                );
                assert_eq!(
                    surface
                        .terminal_env
                        .get("PROJECT_VARIABLE")
                        .map(String::as_str),
                    Some("preserved")
                );
            }
        }
    }

    #[test]
    fn bulk_closed_workspaces_reopen_with_layout_scrollback_and_order() {
        for action in ["close_above", "close_below", "close_others"] {
            for confirmed in [false, true] {
                let mut app = AppState::with_paths_and_terminal_startup(
                    None,
                    None,
                    TerminalStartupMode::RendererOwned,
                )
                .unwrap();
                let titles = [
                    "Above A",
                    "Pinned above",
                    "Above B",
                    "Anchor",
                    "Below A",
                    "Pinned below",
                    "Below B",
                ];
                let mut ids = vec![app.current_workspace_id().unwrap()];
                app.workspaces.get_mut(&ids[0]).unwrap().title = titles[0].into();
                for title in &titles[1..] {
                    ids.push(
                        app.handle("workspace.create", &json!({"title": title}))
                            .unwrap()["workspace_id"]
                            .as_str()
                            .unwrap()
                            .into(),
                    );
                }
                for index in [1, 5] {
                    app.workspaces.get_mut(&ids[index]).unwrap().pinned = true;
                }
                let target_indices: &[usize] = match action {
                    "close_above" => &[0, 2],
                    "close_below" => &[4, 6],
                    _ => &[0, 2, 4, 6],
                };
                for &index in target_indices {
                    let original = app.workspace_selected_surface(&ids[index]).unwrap();
                    app.handle("surface.split", &json!({"workspace_id": ids[index], "surface_id": original, "direction": "right"})).unwrap();
                    for surface_id in app.workspace_surface_ids(&ids[index]) {
                        app.surfaces
                            .get_mut(&surface_id)
                            .unwrap()
                            .terminal_scrollback_snapshot =
                            Some(format!("{} scrollback", titles[index]));
                    }
                }
                app.app_workspace_settings.warn_before_closing_tab = confirmed;
                let request = app
                    .handle(
                        "workspace.action",
                        &json!({
                            "workspace_id": ids[3], "action": action,
                            "source": if confirmed { "context_menu" } else { "api" }
                        }),
                    )
                    .unwrap();
                let result = if confirmed {
                    assert_eq!(request["confirmation_required"], true);
                    assert!(app.recently_closed_items.is_empty());
                    app.handle(
                        "app.close_confirmation.reply",
                        &json!({"id": request["confirmation"]["id"], "confirmed": true}),
                    )
                    .unwrap()
                } else {
                    request
                };
                assert_eq!(result["closed"], target_indices.len());
                assert!(app.workspaces.contains_key(&ids[1]));
                assert!(app.workspaces.contains_key(&ids[5]));
                let history = app.handle("history.list", &json!({})).unwrap();
                assert_eq!(
                    history["entries"].as_array().unwrap().len(),
                    target_indices.len(),
                    "{action}, confirmed={confirmed}"
                );
                for &index in target_indices.iter().rev() {
                    let reopened = app.handle("history.reopen_closed", &json!({})).unwrap();
                    assert_eq!(reopened["handled"], true);
                    let workspace_id = reopened["workspace_id"].as_str().unwrap();
                    assert_eq!(app.workspaces[workspace_id].title, titles[index]);
                    assert_eq!(app.workspaces[workspace_id].panes.len(), 2);
                    for surface_id in app.workspace_surface_ids(workspace_id) {
                        assert_eq!(
                            *app.surfaces[&surface_id].buffer.lock().unwrap(),
                            format!("{} scrollback", titles[index])
                        );
                    }
                }
                let window = app
                    .windows
                    .iter()
                    .find(|window| window.id == app.workspaces[&ids[3]].window_id)
                    .unwrap();
                let restored_titles = window
                    .workspaces
                    .iter()
                    .map(|id| app.workspaces[id].title.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(restored_titles, titles, "{action}, confirmed={confirmed}");
                assert!(app.recently_closed_items.is_empty());
            }
        }
    }

    #[test]
    fn closed_history_limits_scrollback_bytes_and_shares_saved_records() {
        let mut app = AppState::with_paths(None, None).unwrap();
        let surface_id = app.current_surface_id().unwrap();
        let workspace_id = app.current_workspace_id().unwrap();
        let pane_id = app.surfaces[&surface_id].pane_id.clone();
        let surface = app.surfaces.get_mut(&surface_id).unwrap();
        surface.terminal_scrollback_snapshot =
            Some("終".repeat(SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT));
        let mut remaining = 5;
        assert_eq!(
            closed_scrollback_tail(surface, &mut remaining).as_deref(),
            Some("終")
        );
        assert_eq!(remaining, 2);
        app.capture_closed_panel(&surface_id, &workspace_id, &pane_id);
        let snapshot = app.closed_history_snapshot().unwrap();
        assert!(Arc::ptr_eq(
            app.recently_closed_items.front().unwrap(),
            snapshot.records.front().unwrap()
        ));
        let ClosedItem::Panel(panel) = &snapshot.records.front().unwrap().item else {
            panic!("panel");
        };
        assert!(
            panel.snapshot.scrollback.as_ref().unwrap().len()
                <= SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT
        );
    }

    #[test]
    fn closed_history_evicts_oldest_payloads_at_byte_budget() {
        let mut app = AppState::with_paths(None, None).unwrap();
        let surface_id = app.current_surface_id().unwrap();
        let workspace_id = app.current_workspace_id().unwrap();
        let pane_id = app.surfaces[&surface_id].pane_id.clone();
        app.surfaces
            .get_mut(&surface_id)
            .unwrap()
            .terminal_scrollback_snapshot =
            Some("x".repeat(SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT));
        for _ in 0..40 {
            app.capture_closed_panel(&surface_id, &workspace_id, &pane_id);
        }
        assert!(app.recently_closed_items.len() < 40);
        assert!(
            app.recently_closed_items
                .iter()
                .map(|record| record.retained_bytes)
                .sum::<usize>()
                <= MAX_RECENTLY_CLOSED_BYTES
        );
    }
}
