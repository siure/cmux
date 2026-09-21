use super::mode::GtkUiMode;
use super::strings;
use super::*;

#[derive(Clone)]
pub(super) struct GtkSnapshotView {
    pub(super) root: gtk::Widget,
    pub(super) left_slot: gtk::Box,
    pub(super) main_slot: gtk::Box,
    pub(super) right_slot: gtk::Box,
    pub(super) overlay_slot: Option<gtk::Box>,
    pub(super) titlebar: Option<gtk::HeaderBar>,
    shell_body: Option<gtk::Box>,
    right_drawer: Option<gtk::Box>,
    left_drawer: gtk::Box,
    left_frame: gtk::Overlay,
    right_frame: gtk::Overlay,
    left_width: Rc<Cell<i32>>,
    right_width: Rc<Cell<i32>>,
    compact: Rc<Cell<bool>>,
    title: Option<gtk::Label>,
    start_actions: Option<gtk::Box>,
    end_actions: Option<gtk::Box>,
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
    local_refresh: &GtkLocalRefresh,
) -> GtkSnapshotView {
    let left_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left_slot.add_css_class("cmux-left-slot");
    if ui_mode.is_next() {
        left_slot.set_hexpand(false);
    }
    left_slot.append(&workspace_sidebar(snapshot, app_state, ui_mode));
    left_slot.set_visible(left_sidebar_visible(snapshot));

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
        local_refresh,
    ));

    let right_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right_slot.add_css_class("cmux-right-slot");
    if right_sidebar_visible(snapshot) {
        right_slot.append(&app_chrome_sidebar(snapshot, app_state, ui_mode));
    }
    right_slot.set_visible(right_sidebar_visible(snapshot));

    let left_frame = gtk::Overlay::new();
    left_frame.set_child(Some(&left_slot));
    let right_frame = gtk::Overlay::new();
    right_frame.set_child(Some(&right_slot));
    let left_drawer = sidebar_drawer(gtk::Align::Start);
    let left_width = Rc::new(Cell::new(snapshot_sidebar_width(snapshot, false)));
    let right_width = Rc::new(Cell::new(snapshot_sidebar_width(snapshot, true)));

    if !ui_mode.is_next() {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("cmux-root");
        root.add_css_class(ui_mode.root_css_class());
        root.append(&left_frame);
        root.append(&main_slot);
        root.append(&right_frame);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&root));
        let right_drawer = sidebar_drawer(gtk::Align::End);
        overlay.add_overlay(&left_drawer);
        overlay.add_overlay(&right_drawer);
        let overlay_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        overlay_slot.set_hexpand(true);
        overlay_slot.set_vexpand(true);
        overlay.add_overlay(&overlay_slot);
        let view = GtkSnapshotView {
            root: overlay.upcast(),
            left_slot,
            main_slot,
            right_slot,
            overlay_slot: Some(overlay_slot),
            titlebar: None,
            shell_body: Some(root),
            right_drawer: Some(right_drawer),
            left_drawer,
            left_frame,
            right_frame,
            left_width,
            right_width,
            compact: Rc::new(Cell::new(false)),
            title: None,
            start_actions: None,
            end_actions: None,
        };
        install_sidebar_resizers(&view, app_state, window_id);
        refresh_overlay(&view, snapshot, app_state, window_id);
        return view;
    }

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.add_css_class("cmux-shell-body");
    body.append(&left_frame);
    body.append(&main_slot);
    body.append(&right_frame);

    let overlay = gtk::Overlay::new();
    overlay.add_css_class("cmux-root");
    overlay.add_css_class(ui_mode.root_css_class());
    overlay.set_child(Some(&body));

    let right_drawer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right_drawer.add_css_class("cmux-right-drawer");
    right_drawer.set_halign(gtk::Align::End);
    right_drawer.set_valign(gtk::Align::Fill);
    right_drawer.set_vexpand(true);
    right_drawer.set_visible(false);
    overlay.add_overlay(&left_drawer);
    overlay.add_overlay(&right_drawer);

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
    titlebar.set_height_request(super::metrics::HEADER_HEIGHT);
    titlebar.set_show_title_buttons(true);

    let title = gtk::Label::new(None);
    title.add_css_class("cmux-header-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(56);
    title.set_halign(gtk::Align::Center);
    titlebar.set_title_widget(Some(&title));

    let start_actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    start_actions.set_valign(gtk::Align::Center);
    titlebar.pack_start(&start_actions);

    let end_actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    end_actions.set_valign(gtk::Align::Center);
    titlebar.pack_end(&end_actions);

    let view = GtkSnapshotView {
        root: overlay.upcast(),
        left_slot,
        main_slot,
        right_slot,
        overlay_slot: Some(overlay_slot),
        titlebar: Some(titlebar),
        shell_body: Some(body),
        right_drawer: Some(right_drawer),
        left_drawer,
        left_frame,
        right_frame,
        left_width,
        right_width,
        compact: Rc::new(Cell::new(false)),
        title: Some(title),
        start_actions: Some(start_actions),
        end_actions: Some(end_actions),
    };
    install_sidebar_resizers(&view, app_state, window_id);
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
        "window_id": snapshot.pointer("/window/window_id"),
        "workspace": selected.map(|workspace| json!({
            "workspace_id": workspace.get("workspace_id"),
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
    let context = workspace_context(selected);
    title.set_tooltip_text((!context.is_empty()).then_some(context.as_str()));
    let (Some(start_actions), Some(end_actions)) =
        (view.start_actions.as_ref(), view.end_actions.as_ref())
    else {
        return;
    };
    while let Some(child) = start_actions.first_child() {
        start_actions.remove(&child);
    }
    while let Some(child) = end_actions.first_child() {
        end_actions.remove(&child);
    }
    append_header_actions(start_actions, end_actions, snapshot, app_state);
}

fn sidebar_drawer(alignment: gtk::Align) -> gtk::Box {
    let drawer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    drawer.set_halign(alignment);
    drawer.set_valign(gtk::Align::Fill);
    drawer.set_vexpand(true);
    drawer.set_visible(false);
    drawer
}

fn snapshot_sidebar_width(snapshot: &Value, right: bool) -> i32 {
    let (path, fallback, minimum) = if right {
        (
            "/right_sidebar/width",
            metrics::RIGHT_SIDEBAR_WIDTH,
            metrics::MIN_RIGHT_SIDEBAR_WIDTH,
        )
    } else {
        (
            "/left_sidebar/width",
            metrics::SIDEBAR_WIDTH,
            metrics::SIDEBAR_WIDTH,
        )
    };
    snapshot
        .pointer(path)
        .and_then(Value::as_i64)
        .map(|width| width.clamp(i64::from(minimum), 4096) as i32)
        .unwrap_or(fallback)
}

pub(super) fn refresh_sidebar_widths(view: &GtkSnapshotView, snapshot: &Value) {
    view.left_width.set(snapshot_sidebar_width(snapshot, false));
    view.right_width.set(snapshot_sidebar_width(snapshot, true));
    apply_sidebar_widths(view);
}

fn widget_window_width(root: &gtk::Widget) -> i32 {
    root.native()
        .and_then(|native| native.surface())
        .map(|surface| surface.width())
        .unwrap_or_else(|| {
            // Bound restored sizes before the first allocation so they cannot
            // raise the native window minimum beyond its requested size.
            if root.width() > 0 {
                root.width()
            } else {
                GTK_APP_DEFAULT_WIDTH
            }
        })
}

fn window_width(view: &GtkSnapshotView) -> i32 {
    widget_window_width(&view.root)
}

fn set_sidebar_slot_width(slot: &gtk::Box, width: i32) {
    if let Some(frame) = slot.first_child() {
        frame.set_width_request(width);
        if let Some(viewport) = frame
            .first_child()
            .and_then(|child| child.downcast::<gtk::ScrolledWindow>().ok())
        {
            viewport.set_max_content_width(-1);
            viewport.set_min_content_width(width);
            viewport.set_max_content_width(width);
        }
    }
}

fn apply_sidebar_widths(view: &GtkSnapshotView) {
    let right_max = config::sidebar_settings()
        .right_max_width
        .unwrap_or(1200.0)
        .round() as i32;
    let (left, right) = metrics::sidebar_widths(
        view.left_width.get(),
        view.right_width
            .get()
            .min(right_max.max(metrics::MIN_RIGHT_SIDEBAR_WIDTH)),
        window_width(view),
        view.left_slot.get_visible(),
        view.compact.get(),
    );
    for (slot, width) in [(&view.left_slot, left), (&view.right_slot, right)] {
        set_sidebar_slot_width(slot, width);
    }
}

fn move_sidebar(frame: &gtk::Overlay, target: &gtk::Box, prepend: bool) {
    if frame.parent().as_ref() == Some(target.upcast_ref()) {
        return;
    }
    if let Some(parent) = frame
        .parent()
        .and_then(|parent| parent.downcast::<gtk::Box>().ok())
    {
        parent.remove(frame);
    }
    if prepend {
        target.prepend(frame);
    } else {
        target.append(frame);
    }
}

pub(super) fn set_compact_layout(view: &GtkSnapshotView, compact: bool) {
    view.compact.set(compact);
    let (Some(body), Some(drawer)) = (view.shell_body.as_ref(), view.right_drawer.as_ref()) else {
        return;
    };
    let narrow = window_width(view) > 0
        && window_width(view) < metrics::SIDEBAR_WIDTH + metrics::MIN_TERMINAL_WIDTH;
    move_sidebar(
        &view.left_frame,
        if narrow { &view.left_drawer } else { body },
        true,
    );
    move_sidebar(
        &view.right_frame,
        if compact { drawer } else { body },
        false,
    );
    if compact {
        view.root.add_css_class("cmux-layout-compact");
    } else {
        view.root.remove_css_class("cmux-layout-compact");
    }
    // Use each slot's requested visibility. Effective visibility includes its
    // hidden drawer ancestor and would prevent a closed drawer from opening.
    view.left_drawer
        .set_visible(narrow && view.left_slot.get_visible());
    drawer.set_visible(compact && view.right_slot.get_visible());
    apply_sidebar_widths(view);
}

pub(super) fn set_right_sidebar_visible(view: &GtkSnapshotView, visible: bool) {
    view.right_slot.set_visible(visible);
    view.right_frame.set_visible(visible);
    set_compact_layout(view, view.compact.get());
}

pub(super) fn set_left_sidebar_visible(view: &GtkSnapshotView, visible: bool) {
    view.left_slot.set_visible(visible);
    view.left_frame.set_visible(visible);
    set_compact_layout(view, view.compact.get());
}

fn install_sidebar_resizers(
    view: &GtkSnapshotView,
    app_state: &Arc<Mutex<AppState>>,
    window_id: &str,
) {
    for (right, frame, width) in [
        (false, &view.left_frame, &view.left_width),
        (true, &view.right_frame, &view.right_width),
    ] {
        let handle = gtk::Box::new(gtk::Orientation::Vertical, 0);
        handle.add_css_class("cmux-sidebar-resizer");
        handle.set_width_request(6);
        handle.set_halign(if right {
            gtk::Align::Start
        } else {
            gtk::Align::End
        });
        handle.set_valign(gtk::Align::Fill);
        handle.set_cursor_from_name(Some("col-resize"));
        handle.set_tooltip_text(Some(&strings::text(if right {
            "sidebar.resize_right"
        } else {
            "sidebar.resize_left"
        })));
        let start_width = Rc::new(Cell::new(width.get()));
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let start = Rc::clone(&start_width);
        let preferred_width = Rc::clone(width);
        drag.connect_drag_begin(move |gesture, _, _| {
            start.set(preferred_width.get());
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        let state = Arc::clone(app_state);
        let window_id = window_id.to_string();
        let left_width = Rc::clone(&view.left_width);
        let right_width = Rc::clone(&view.right_width);
        let compact = Rc::clone(&view.compact);
        let weak_root = view.root.downgrade();
        let weak_left = view.left_slot.downgrade();
        let weak_right = view.right_slot.downgrade();
        drag.connect_drag_update(move |_, offset, _| {
            let (Some(root), Some(left_slot), Some(right_slot)) = (
                weak_root.upgrade(),
                weak_left.upgrade(),
                weak_right.upgrade(),
            ) else {
                return;
            };
            let minimum = if right {
                metrics::MIN_RIGHT_SIDEBAR_WIDTH
            } else {
                metrics::SIDEBAR_WIDTH
            };
            let candidate = (start_width.get()
                + (if right { -offset } else { offset }).round() as i32)
                .max(minimum);
            let method = if right {
                "sidebar.right"
            } else {
                "sidebar.left"
            };
            if let Some(result) = call_app_value(
                &state,
                method,
                json!({"action": "resize", "window_id": window_id, "width": candidate}),
            ) {
                if let Some(width) = result["width"].as_i64() {
                    if right {
                        right_width.set(width as i32);
                    } else {
                        left_width.set(width as i32);
                    }
                    let right_max = config::sidebar_settings()
                        .right_max_width
                        .unwrap_or(1200.0)
                        .round() as i32;
                    let (left, right_size) = metrics::sidebar_widths(
                        left_width.get(),
                        right_width
                            .get()
                            .min(right_max.max(metrics::MIN_RIGHT_SIDEBAR_WIDTH)),
                        widget_window_width(&root),
                        left_slot.get_visible(),
                        compact.get(),
                    );
                    set_sidebar_slot_width(&left_slot, left);
                    set_sidebar_slot_width(&right_slot, right_size);
                }
            }
        });
        handle.add_controller(drag);
        frame.add_overlay(&handle);
        frame.set_measure_overlay(&handle, false);
    }
    set_left_sidebar_visible(view, view.left_slot.get_visible());
    set_right_sidebar_visible(view, view.right_slot.get_visible());
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
    let existing_palette = slot.first_child().filter(|child| {
        widget_descendant_with_css_class(child, "cmux-palette")
            .and_then(|panel| panel.downcast::<gtk::Box>().ok())
            .is_some_and(|panel| update_command_palette_panel(&panel, snapshot, app_state))
    });
    let mut child = slot.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if existing_palette.as_ref() != Some(&current) {
            slot.remove(&current);
        }
    }
    if let Some(palette) = existing_palette
        .is_none()
        .then(|| command_palette_panel(snapshot, app_state))
        .flatten()
    {
        palette.add_css_class("cmux-shell-overlay-panel");
        palette.set_halign(gtk::Align::Center);
        palette.set_valign(gtk::Align::Start);
        let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);
        let click = gtk::GestureClick::new();
        let palette_state = Arc::clone(app_state);
        let palette_window = window_id.to_string();
        click.connect_pressed(move |_, _, _, _| {
            dismiss_command_palette(&palette_state, &palette_window);
        });
        backdrop.add_controller(click);
        let overlay = gtk::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        overlay.set_child(Some(&backdrop));
        overlay.add_overlay(&palette);
        slot.append(&overlay);
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

fn dismiss_command_palette(app_state: &Arc<Mutex<AppState>>, window_id: &str) {
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    let params = json!({"window_id": window_id});
    let visible = app
        .handle_ui("debug.command_palette.visible", &params)
        .ok()
        .and_then(|state| state.get("visible").and_then(Value::as_bool))
        .unwrap_or(false);
    if visible {
        let _ = app.handle_ui("debug.command_palette.toggle", &params);
    }
}

fn append_header_actions(
    start_actions: &gtk::Box,
    end_actions: &gtk::Box,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    start_actions.append(&header_icon_button(
        "sidebar-show-symbolic",
        &strings::text("header.toggle_left_sidebar"),
        app_state,
        "sidebar.left",
        json!({"action": "toggle", "window_id": snapshot.pointer("/window/window_id")}),
    ));
    let (new_workspace_method, new_workspace_params) = new_workspace_request_for_snapshot(snapshot);
    start_actions.append(&header_icon_button(
        "list-add-symbolic",
        &strings::text("header.new_workspace"),
        app_state,
        new_workspace_method,
        new_workspace_params,
    ));
    end_actions.append(&header_icon_button(
        "system-search-symbolic",
        &strings::text("header.command_palette"),
        app_state,
        "debug.command_palette.toggle",
        json!({"window_id": snapshot.pointer("/window/window_id")}),
    ));
    end_actions.append(&header_icon_button(
        "sidebar-show-right-symbolic",
        &strings::text("header.toggle_right_sidebar"),
        app_state,
        "sidebar.right",
        json!({"action": "toggle", "no_focus": true, "window_id": snapshot.pointer("/window/window_id")}),
    ));
    end_actions.append(&overflow_button(snapshot, app_state));
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
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button.set_focus_on_click(false);
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
    let more_actions = strings::text("header.more_actions");
    button.set_tooltip_text(Some(&more_actions));
    button.update_property(&[gtk::accessible::Property::Label(&more_actions)]);
    button.set_focusable(true);

    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);
    menu.add_css_class("cmux-overflow-menu");
    for (key, method, params) in [
        (
            "action.new_terminal",
            "surface.create",
            json!({"type": "terminal", "focus": true}),
        ),
        (
            "header.open_browser",
            "browser.open_split",
            json!({"url": "about:blank", "focus": true}),
        ),
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
        ("header.shortcut_help", "help.shortcuts.toggle", json!({})),
        (
            "header.settings",
            "settings.open",
            json!({
                "window_id": snapshot.pointer("/window/window_id"),
                "workspace_id": selected_workspace(snapshot).and_then(|workspace| workspace.get("workspace_id"))
            }),
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
    fn palette_backdrop_dismisses_without_reopening_closed_palette() {
        let app_state = Arc::new(Mutex::new(AppState::with_paths(None, None).unwrap()));
        let windows = call_app_value(&app_state, "window.list", json!({})).unwrap();
        let window_id = windows["windows"][0]["id"]
            .as_str()
            .or_else(|| windows["windows"][0]["window_id"].as_str())
            .unwrap();
        assert!(call_app(
            &app_state,
            "debug.command_palette.toggle",
            json!({"window_id": window_id})
        ));
        dismiss_command_palette(&app_state, window_id);
        assert!(!palette_visible(&app_state));
        dismiss_command_palette(&app_state, window_id);
        assert!(!palette_visible(&app_state));
    }

    #[gtk::test]
    fn compact_right_sidebar_show_mounts_visible_file_content_after_startup() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("drawer-visible.txt"), "test").unwrap();
        let app_state = Arc::new(Mutex::new(AppState::with_paths(None, None).unwrap()));
        let application = gtk::Application::builder()
            .application_id("ai.manaflow.cmux.tests.sidebar-drawer")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application.register(None::<&gio::Cancellable>).unwrap();
        let row = model_window_rows(&app_state).remove(0);
        let window_id = model_window_id(&row).unwrap();
        let local_refresh = GtkLocalRefresh::new(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        let snapshot = snapshot_or_error(&app_state, GtkRendererMode::Gtk, window_id);
        assert!(!right_sidebar_visible(&snapshot));
        let mut host = create_gtk_window_host(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            window_id,
            &row,
            &snapshot,
            &local_refresh,
        );
        host.window.set_default_size(900, 700);
        host.window.present();
        let settle = || {
            let main_loop = glib::MainLoop::new(None, false);
            let quit = main_loop.clone();
            glib::timeout_add_local_once(Duration::from_millis(100), move || quit.quit());
            main_loop.run();
        };
        settle();
        call_app_value(
            &app_state,
            "sidebar.right",
            json!({"action": "show", "window_id": window_id}),
        )
        .unwrap();
        call_app_value(
            &app_state,
            "debug.command_palette.toggle",
            json!({"window_id": window_id}),
        )
        .unwrap();
        let mut shown = snapshot_or_error(&app_state, GtkRendererMode::Gtk, window_id);
        shown["sidebar"]["cwd"] = json!(directory.path());
        refresh_gtk_window_host(
            &mut host,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &row,
            &shown,
            &local_refresh,
        );
        settle();
        let view = &host.snapshot_view;
        let drawer = view.right_drawer.as_ref().unwrap();
        let chrome = widget_descendant_with_css_class(&view.root, "cmux-chrome")
            .expect("files chrome mounted");
        assert!(view.compact.get());
        assert!(drawer.is_visible() && drawer.is_mapped(), "drawer visibility={} mapped={} width={} height={} frame_visible={} slot_visible={} chrome_mapped={}", drawer.is_visible(), drawer.is_mapped(), drawer.width(), drawer.height(), view.right_frame.is_visible(), view.right_slot.is_visible(), chrome.is_mapped());
        assert!(drawer.width() >= metrics::MIN_RIGHT_SIDEBAR_WIDTH && drawer.height() > 300);
        assert!(
            chrome.is_mapped() && chrome.width() >= 250 && chrome.height() > 300,
            "file content mapped={} width={} height={}",
            chrome.is_mapped(),
            chrome.width(),
            chrome.height()
        );
        let bounds = chrome.compute_bounds(&view.root).expect("file bounds");
        assert!(bounds.x() > view.root.width() as f32 / 2.0);
        host.window.destroy();
    }

    #[gtk::test]
    fn sidebar_drag_targets_its_window_and_preserves_width_across_compact_layout() {
        let app_state = Arc::new(Mutex::new(AppState::with_paths(None, None).unwrap()));
        let window_id = call_app_value(&app_state, "window.current", json!({})).unwrap()
            ["window_id"]
            .as_str()
            .unwrap()
            .to_string();
        for (method, width) in [("sidebar.left", 340), ("sidebar.right", 410)] {
            call_app_value(
                &app_state,
                method,
                json!({"action": "resize", "width": width}),
            )
            .unwrap();
        }
        let other = call_app_value(&app_state, "window.create", json!({})).unwrap();
        let application = gtk::Application::builder()
            .application_id("ai.manaflow.cmux.tests.sidebar-resize")
            .build();
        let local_refresh = GtkLocalRefresh::new(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        let snapshot = json!({"window": {"window_id": window_id}, "left_sidebar": {"width": 340}, "right_sidebar": {"visible": true, "width": 410}});
        let view = build_snapshot_view(
            &snapshot,
            &app_state,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &window_id,
            &local_refresh,
        );
        refresh_sidebar_widths(
            &view,
            &json!({"left_sidebar": {"width": 4096}, "right_sidebar": {"width": 4096}}),
        );
        assert!(view.left_slot.first_child().unwrap().width_request() <= GTK_APP_DEFAULT_WIDTH / 3);
        assert_eq!(view.left_width.get(), 4096);
        refresh_sidebar_widths(&view, &snapshot);
        let window = gtk::Window::builder()
            .default_width(1600)
            .default_height(500)
            .child(&view.root)
            .build();
        window.present();
        let responsive_view = view.clone();
        window
            .surface()
            .unwrap()
            .connect_width_notify(move |surface| {
                set_compact_layout(
                    &responsive_view,
                    metrics::compact_layout_for_width(surface.width()),
                );
            });
        let settle = || {
            let main_loop = glib::MainLoop::new(None, false);
            let quit = main_loop.clone();
            glib::timeout_add_local_once(Duration::from_millis(80), move || quit.quit());
            main_loop.run();
        };
        settle();
        assert_eq!(view.left_frame.width(), 340);
        assert_eq!(view.right_frame.width(), 410);
        for (frame, method, offset, expected) in [
            (&view.left_frame, "sidebar.left", 60.0, 400),
            (&view.right_frame, "sidebar.right", -30.0, 440),
        ] {
            let handle = frame.last_child().unwrap();
            let controllers = handle.observe_controllers();
            let gesture = (0..controllers.n_items())
                .find_map(|i| controllers.item(i).and_downcast::<gtk::GestureDrag>())
                .unwrap();
            gesture.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
            gesture.emit_by_name::<()>("drag-update", &[&offset, &0.0f64]);
            gesture.emit_by_name::<()>("drag-end", &[&offset, &0.0f64]);
            assert_eq!(
                call_app_value(
                    &app_state,
                    method,
                    json!({"action": "mode", "window_id": window_id})
                )
                .unwrap()["width"],
                expected
            );
        }
        assert_eq!(
            call_app_value(
                &app_state,
                "sidebar.left",
                json!({"action": "mode", "window_id": other["window_id"]})
            )
            .unwrap()["width"],
            240
        );
        set_left_sidebar_visible(&view, false);
        set_left_sidebar_visible(&view, true);
        assert_eq!(view.left_slot.first_child().unwrap().width_request(), 400);
        window.set_default_size(800, 500);
        settle();
        assert!(view.compact.get());
        assert_eq!(
            view.right_frame.parent().unwrap(),
            view.right_drawer
                .as_ref()
                .unwrap()
                .clone()
                .upcast::<gtk::Widget>()
        );
        // GTK can retain the pre-drawer minimum (840px here) for this first
        // resize. Check the allocated native width, not the requested 800px.
        let compact_width = window.surface().unwrap().width();
        assert!(metrics::compact_layout_for_width(compact_width));
        assert_eq!(
            view.left_slot.first_child().unwrap().width_request(),
            compact_width / 3
        );
        assert!(view.main_slot.width() >= metrics::MIN_TERMINAL_WIDTH);
        assert_eq!(view.left_width.get(), 400);
        let controllers = view.left_frame.last_child().unwrap().observe_controllers();
        let gesture = (0..controllers.n_items())
            .find_map(|i| controllers.item(i).and_downcast::<gtk::GestureDrag>())
            .unwrap();
        gesture.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
        gesture.emit_by_name::<()>("drag-update", &[&10.0f64, &0.0f64]);
        assert_eq!(
            call_app_value(
                &app_state,
                "sidebar.left",
                json!({"action": "mode", "window_id": window_id}),
            )
            .unwrap()["width"],
            400,
            "a drag that cannot move the divider preserves the preference",
        );
        assert_eq!(
            view.left_slot.first_child().unwrap().width_request(),
            compact_width / 3
        );
        gesture.emit_by_name::<()>("drag-update", &[&-10.0f64, &0.0f64]);
        settle();
        assert_eq!(view.left_frame.width(), compact_width / 3 - 10);
        assert_eq!(view.left_width.get(), compact_width / 3 - 10);
        gesture.emit_by_name::<()>("drag-update", &[&0.0f64, &0.0f64]);
        settle();
        assert_eq!(view.left_frame.width(), compact_width / 3);
        assert_eq!(view.left_width.get(), 400);
        gesture.emit_by_name::<()>("drag-update", &[&-10.0f64, &0.0f64]);
        gesture.emit_by_name::<()>("drag-end", &[&-10.0f64, &0.0f64]);
        window.set_default_size(1600, 500);
        settle();
        assert!(!view.compact.get());
        assert_eq!(
            view.left_slot.first_child().unwrap().width_request(),
            compact_width / 3 - 10
        );
        assert_eq!(view.right_slot.first_child().unwrap().width_request(), 440);
        for (frame, method, offset, minimum) in [
            (
                &view.left_frame,
                "sidebar.left",
                -1000.0,
                metrics::SIDEBAR_WIDTH,
            ),
            (
                &view.right_frame,
                "sidebar.right",
                1000.0,
                metrics::MIN_RIGHT_SIDEBAR_WIDTH,
            ),
        ] {
            let controllers = frame.last_child().unwrap().observe_controllers();
            let gesture = (0..controllers.n_items())
                .find_map(|i| controllers.item(i).and_downcast::<gtk::GestureDrag>())
                .unwrap();
            gesture.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
            gesture.emit_by_name::<()>("drag-update", &[&offset, &0.0f64]);
            gesture.emit_by_name::<()>("drag-end", &[&offset, &0.0f64]);
            assert_eq!(
                call_app_value(
                    &app_state,
                    method,
                    json!({"action": "mode", "window_id": window_id})
                )
                .unwrap()["width"],
                minimum,
                "dragging beyond zero clamps to the minimum",
            );
        }
        window.destroy();
    }

    #[gtk::test]
    fn shell_header_exposes_workspace_sidebar_toggle() {
        let app_state = Arc::new(Mutex::new(AppState::with_paths(None, None).unwrap()));
        let windows = call_app_value(&app_state, "window.list", json!({})).unwrap();
        let first_window = windows["windows"][0]["id"].as_str().unwrap();
        let second_window = call_app_value(&app_state, "window.create", json!({})).unwrap();
        let start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        append_header_actions(
            &start,
            &end,
            &json!({"window": {"window_id": first_window}}),
            &app_state,
        );
        let mut child = start.first_child();
        let mut toggle = None;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.tooltip_text().as_deref()
                == Some(strings::text("header.toggle_left_sidebar").as_str())
            {
                toggle = widget.downcast::<gtk::Button>().ok();
                break;
            }
        }
        toggle
            .expect("workspace sidebar toggle in titlebar")
            .emit_clicked();
        let state = call_app_value(
            &app_state,
            "sidebar.left",
            json!({"window_id": first_window}),
        )
        .unwrap();
        assert_eq!(state["visible"], false);
        let other = call_app_value(
            &app_state,
            "sidebar.left",
            json!({"window_id": second_window["window_id"]}),
        )
        .unwrap();
        assert_eq!(other["visible"], true);
    }

    #[gtk::test]
    fn shell_overflow_exposes_settings_action() {
        let app_state = Arc::new(Mutex::new(AppState::with_paths(None, None).unwrap()));
        let windows = call_app_value(&app_state, "window.list", json!({})).unwrap();
        let first_window = windows["windows"][0]["id"].as_str().unwrap();
        let workspaces = call_app_value(&app_state, "workspace.list", json!({})).unwrap();
        let workspace_id = workspaces["workspaces"][0]["id"].as_str().unwrap();
        let snapshot = json!({
            "window": {"window_id": first_window},
            "workspaces": [{"workspace_id": workspace_id, "selected": true}]
        });
        let overflow = overflow_button(&snapshot, &app_state);
        call_app_value(&app_state, "window.create", json!({})).unwrap();
        let menu = overflow.popover().unwrap().child().unwrap();
        let mut child = menu.first_child();
        let mut settings = None;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<gtk::Button>() {
                if button.label().as_deref() == Some(strings::text("header.settings").as_str()) {
                    settings = Some(button);
                    break;
                }
            }
        }
        settings
            .expect("Settings must be reachable through the main menu")
            .emit_clicked();
        let surfaces = call_app_value(
            &app_state,
            "surface.list",
            json!({"workspace_id": workspace_id}),
        )
        .unwrap();
        assert!(surfaces["surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|surface| surface["type"] == "settings"));
    }

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
