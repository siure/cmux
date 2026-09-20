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

// Reopening launches a fresh shell. Saved commands and partially submitted input
// are display/history data, never instructions to execute again.
fn prepare_closed_surface(snapshot: &mut SessionSurfaceSnapshot) {
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
        let result = match item {
            ClosedItem::Panel(panel) => self.restore_closed_panel(panel)?,
            ClosedItem::Workspace(workspace) => Some(self.restore_closed_workspace(workspace)?),
            ClosedItem::Window(window) => Some(self.restore_closed_window(window)?),
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
