use super::mode::GtkUiMode;
use super::strings;
use super::*;

pub(super) struct GtkSnapshotView {
    pub(super) root: gtk::Widget,
    pub(super) left_slot: gtk::Box,
    pub(super) main_slot: gtk::Box,
    pub(super) right_slot: gtk::Box,
    pub(super) overlay_slot: Option<gtk::Box>,
    pub(super) titlebar: Option<gtk::HeaderBar>,
    title: Option<gtk::Label>,
    context: Option<gtk::Label>,
    actions: Option<gtk::Box>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_snapshot_view(
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
    pane_allocations: &PaneAllocations,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    diff_controls: &DiffSurfaceControlsCache,
    terminal_search_controls: &TerminalSearchControlsCache,
    terminal_text_box_controls: &TerminalTextBoxControlsCache,
    canvas_minimap_states: &GtkCanvasMinimapStates,
    canvas_occlusion_states: &GtkCanvasOcclusionStates,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    window_id: &str,
) -> GtkSnapshotView {
    let left_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left_slot.add_css_class("cmux-left-slot");
    if ui_mode.is_next() {
        left_slot.set_hexpand(false);
    }
    left_slot.append(&workspace_sidebar(snapshot, app_state, ui_mode));

    let main_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main_slot.add_css_class("cmux-main-slot");
    main_slot.set_hexpand(true);
    main_slot.set_vexpand(true);
    main_slot.append(&surface_area(
        snapshot,
        app_state,
        pane_allocations,
        ghostty_widgets,
        browser_controls,
        diff_controls,
        terminal_search_controls,
        terminal_text_box_controls,
        canvas_minimap_states,
        canvas_occlusion_states,
        renderer_mode,
        ui_mode,
    ));

    let right_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right_slot.add_css_class("cmux-right-slot");
    if right_sidebar_visible(snapshot) {
        right_slot.append(&app_chrome_sidebar(snapshot, app_state));
    }
    right_slot.set_visible(right_sidebar_visible(snapshot));

    if !ui_mode.is_next() {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("cmux-root");
        root.add_css_class(ui_mode.root_css_class());
        root.append(&left_slot);
        root.append(&main_slot);
        root.append(&right_slot);
        return GtkSnapshotView {
            root: root.upcast(),
            left_slot,
            main_slot,
            right_slot,
            overlay_slot: None,
            titlebar: None,
            title: None,
            context: None,
            actions: None,
        };
    }

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.add_css_class("cmux-shell-body");
    body.append(&left_slot);
    body.append(&main_slot);
    body.append(&right_slot);

    let overlay = gtk::Overlay::new();
    overlay.add_css_class("cmux-root");
    overlay.add_css_class(ui_mode.root_css_class());
    overlay.set_child(Some(&body));

    let overlay_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    overlay_slot.add_css_class("cmux-shell-overlay-slot");
    overlay_slot.set_halign(gtk::Align::Fill);
    overlay_slot.set_valign(gtk::Align::Fill);
    overlay_slot.set_hexpand(true);
    overlay_slot.set_vexpand(true);
    overlay.add_overlay(&overlay_slot);

    let titlebar = gtk::HeaderBar::new();
    titlebar.add_css_class(ui_mode.root_css_class());
    titlebar.add_css_class("cmux-headerbar");
    titlebar.set_show_title_buttons(true);

    let title_stack = gtk::Box::new(gtk::Orientation::Vertical, 0);
    title_stack.set_halign(gtk::Align::Center);
    title_stack.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(None);
    title.add_css_class("cmux-header-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(48);
    let context = gtk::Label::new(None);
    context.add_css_class("cmux-header-context");
    context.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    context.set_max_width_chars(64);
    title_stack.append(&title);
    title_stack.append(&context);
    titlebar.set_title_widget(Some(&title_stack));

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    actions.set_valign(gtk::Align::Center);
    titlebar.pack_end(&actions);

    let view = GtkSnapshotView {
        root: overlay.upcast(),
        left_slot,
        main_slot,
        right_slot,
        overlay_slot: Some(overlay_slot),
        titlebar: Some(titlebar),
        title: Some(title),
        context: Some(context),
        actions: Some(actions),
    };
    refresh_header(&view, snapshot, app_state);
    refresh_overlay(&view, snapshot, app_state, window_id);
    view
}

pub(super) fn header_rebuild_key(snapshot: &Value) -> Value {
    let selected = selected_workspace(snapshot);
    let branch_status_value = selected
        .and_then(|workspace| workspace.get("status_entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| entry.get("key").and_then(Value::as_str) == Some("branch"))
        .and_then(|entry| entry.get("value"));
    json!({
        "workspace": selected.map(|workspace| json!({
            "title": workspace.get("title"),
            "git_branch": workspace.get("git_branch"),
            "cwd": workspace.get("cwd"),
            "ssh_target": workspace.get("ssh_target"),
            "remote_host": workspace.pointer("/remote/host"),
            "group_id": workspace.get("group_id"),
            "group_ref": workspace.get("group_ref"),
            "branch_status_value": branch_status_value
        })),
        "canvas_mode": snapshot.pointer("/canvas/mode"),
        "new_workspace_placement": snapshot.pointer("/config/app/newWorkspacePlacement"),
        "inherit_working_directory": snapshot.pointer("/config/app/workspaceInheritWorkingDirectory")
    })
}

pub(super) fn overlay_rebuild_key(snapshot: &Value) -> Value {
    json!({
        "command_palette": snapshot.get("command_palette"),
        "shortcut_help": snapshot.get("shortcut_help")
    })
}

pub(super) fn refresh_header(
    view: &GtkSnapshotView,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let Some(title) = view.title.as_ref() else {
        return;
    };
    let selected = selected_workspace(snapshot);
    title.set_text(
        selected
            .and_then(|workspace| workspace.get("title").and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .unwrap_or("cmux"),
    );
    if let Some(context_label) = view.context.as_ref() {
        let context = workspace_context(selected);
        context_label.set_text(&context);
        context_label.set_visible(!context.is_empty());
    }
    let Some(actions) = view.actions.as_ref() else {
        return;
    };
    while let Some(child) = actions.first_child() {
        actions.remove(&child);
    }
    append_header_actions(actions, snapshot, app_state);
}

pub(super) fn refresh_overlay(
    view: &GtkSnapshotView,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
    window_id: &str,
) {
    let Some(slot) = view.overlay_slot.as_ref() else {
        return;
    };
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }
    if let Some(palette) = command_palette_panel(snapshot) {
        palette.add_css_class("cmux-shell-overlay-panel");
        palette.set_halign(gtk::Align::Center);
        palette.set_valign(gtk::Align::Start);
        slot.append(&palette);
    }
    if let Some(shortcuts) = shortcut_help_panel(snapshot, Some((app_state, window_id))) {
        shortcuts.add_css_class("cmux-shell-overlay-panel");
        shortcuts.set_halign(gtk::Align::Center);
        configure_shortcut_help_overlay_panel(&shortcuts);

        let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
        backdrop.add_css_class("cmux-shortcut-help-backdrop");
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);
        let click = gtk::GestureClick::new();
        let app_state = Arc::clone(app_state);
        let window_id = window_id.to_string();
        click.connect_pressed(move |_, _, _, _| {
            handle_shortcut_help_dismissal(
                &app_state,
                &window_id,
                ShortcutHelpDismissInteraction::BackdropPress,
            );
        });
        backdrop.add_controller(click);

        let overlay = gtk::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        overlay.set_child(Some(&backdrop));
        overlay.add_overlay(&shortcuts);
        slot.append(&overlay);
    }
    slot.set_visible(slot.first_child().is_some());
}

fn append_header_actions(actions: &gtk::Box, snapshot: &Value, app_state: &Arc<Mutex<AppState>>) {
    let (new_workspace_method, new_workspace_params) = new_workspace_request_for_snapshot(snapshot);
    actions.append(&header_icon_button(
        "list-add-symbolic",
        &strings::text("header.new_workspace"),
        app_state,
        new_workspace_method,
        new_workspace_params,
    ));
    actions.append(&header_icon_button(
        BROWSER_TOOLBAR_ICON,
        &strings::text("header.open_browser"),
        app_state,
        "browser.open_split",
        json!({"url": "about:blank", "focus": true}),
    ));
    actions.append(&header_icon_button(
        "sidebar-show-right-symbolic",
        &strings::text("header.toggle_right_sidebar"),
        app_state,
        "sidebar.right",
        json!({"action": "toggle", "no_focus": true}),
    ));
    actions.append(&header_icon_button(
        "system-search-symbolic",
        &strings::text("header.command_palette"),
        app_state,
        "debug.command_palette.toggle",
        json!({}),
    ));
    actions.append(&header_icon_button(
        "help-browser-symbolic",
        &strings::text("header.shortcut_help"),
        app_state,
        "help.shortcuts.toggle",
        json!({}),
    ));
    actions.append(&overflow_button(snapshot, app_state));
}

fn header_icon_button(
    icon_name: &str,
    tooltip: &str,
    app_state: &Arc<Mutex<AppState>>,
    method: &'static str,
    params: Value,
) -> gtk::Button {
    let image = gtk::Image::from_icon_name(icon_name);
    let button = gtk::Button::builder().child(&image).build();
    button.add_css_class("cmux-header-action");
    button.set_focusable(true);
    button.set_tooltip_text(Some(tooltip));
    let app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(&app_state, method, params.clone());
    });
    button
}

fn overflow_action_button(
    title: &str,
    app_state: &Arc<Mutex<AppState>>,
    method: &'static str,
    params: Value,
    popover: &gtk::Popover,
) -> gtk::Button {
    let button = action_button(title, app_state, method, params);
    button.set_focusable(true);
    let popover = popover.downgrade();
    button.connect_clicked(move |_| {
        if let Some(popover) = popover.upgrade() {
            popover.popdown();
        }
    });
    button
}

fn overflow_button(snapshot: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_icon_name("view-more-symbolic");
    button.add_css_class("cmux-header-action");
    button.set_tooltip_text(Some(&strings::text("header.more_actions")));
    button.set_focusable(true);

    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);
    menu.add_css_class("cmux-overflow-menu");
    for (key, method, params) in [
        (
            "action.split_right",
            "surface.split",
            json!({"direction": "right"}),
        ),
        (
            "action.split_down",
            "surface.split",
            json!({"direction": "down"}),
        ),
        (
            "action.new_terminal",
            "surface.create",
            json!({"type": "terminal", "focus": true}),
        ),
    ] {
        menu.append(&overflow_action_button(
            &strings::text(key),
            app_state,
            method,
            params,
            &popover,
        ));
    }
    if canvas_mode(snapshot) {
        menu.append(&overflow_action_button(
            &strings::text("action.use_splits"),
            app_state,
            "canvas.set_mode",
            json!({"mode": "splits"}),
            &popover,
        ));
        menu.append(&overflow_action_button(
            &strings::text("action.zoom_in"),
            app_state,
            "canvas.zoom",
            json!({"direction": "in"}),
            &popover,
        ));
        menu.append(&overflow_action_button(
            &strings::text("action.zoom_out"),
            app_state,
            "canvas.zoom",
            json!({"direction": "out"}),
            &popover,
        ));
        menu.append(&overflow_action_button(
            &strings::text("action.canvas_overview"),
            app_state,
            "canvas.overview",
            json!({}),
            &popover,
        ));
    } else {
        menu.append(&overflow_action_button(
            &strings::text("action.use_canvas"),
            app_state,
            "canvas.set_mode",
            json!({"mode": "canvas"}),
            &popover,
        ));
    }
    for (key, method) in [
        ("action.install_claude", "integration.claude.open_installer"),
        ("action.install_codex", "integration.codex.open_installer"),
        (
            "action.install_opencode",
            "integration.opencode.open_installer",
        ),
    ] {
        menu.append(&overflow_action_button(
            &strings::text(key),
            app_state,
            method,
            json!({}),
            &popover,
        ));
    }

    popover.set_child(Some(&menu));
    button.set_popover(Some(&popover));
    button
}

fn selected_workspace(snapshot: &Value) -> Option<&Value> {
    snapshot
        .get("workspaces")
        .and_then(Value::as_array)?
        .iter()
        .find(|workspace| workspace_selected(workspace))
}

fn workspace_context(workspace: Option<&Value>) -> String {
    let Some(workspace) = workspace else {
        return String::new();
    };
    let branch = workspace
        .get("git_branch")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            workspace
                .get("status_entries")
                .and_then(Value::as_array)?
                .iter()
                .find(|entry| entry.get("key").and_then(Value::as_str) == Some("branch"))
                .and_then(|entry| entry.get("value").and_then(Value::as_str))
                .filter(|value| !value.is_empty())
        });
    let cwd = workspace
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let remote = workspace
        .pointer("/remote/host")
        .or_else(|| workspace.get("ssh_target"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    [branch, remote, cwd]
        .into_iter()
        .flatten()
        .take(2)
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_header_uses_workspace_title_and_context_not_internal_refs() {
        let snapshot = json!({
            "workspaces": [{
                "selected": true,
                "title": "Rewrite GTK shell",
                "git_branch": "feat/linux-ui",
                "cwd": "/work/cmux",
                "workspace_ref": "workspace:2"
            }]
        });
        let workspace = selected_workspace(&snapshot).unwrap();
        assert_eq!(
            workspace.get("title").and_then(Value::as_str),
            Some("Rewrite GTK shell")
        );
        assert_eq!(
            workspace_context(Some(workspace)),
            "feat/linux-ui · /work/cmux"
        );
        assert!(!workspace_context(Some(workspace)).contains("workspace:2"));
    }

    #[test]
    fn next_header_rebuild_key_tracks_shell_relevant_state() {
        let original = json!({
            "workspaces": [{
                "selected": true,
                "title": "One",
                "status_entries": [{
                    "key": "branch",
                    "value": "feat/linux-ui",
                    "priority": 10
                }]
            }],
            "canvas": {"mode": "splits"},
            "right_sidebar": {"visible": false},
            "config": {"app": {"newWorkspacePlacement": "afterCurrent"}}
        });
        let mut changed = original.clone();
        changed["workspaces"][0]["title"] = json!("Two");
        assert_ne!(header_rebuild_key(&original), header_rebuild_key(&changed));

        let mut unrelated = original.clone();
        unrelated["workspaces"][0]["progress"] = json!({"value": 0.8});
        unrelated["workspaces"][0]["latest_log"] = json!("building");
        unrelated["workspaces"][0]["status_entries"][0]["priority"] = json!(99);
        unrelated["workspaces"][0]["status_entries"][0]["color"] = json!("blue");
        assert_eq!(
            header_rebuild_key(&original),
            header_rebuild_key(&unrelated)
        );
    }
}
