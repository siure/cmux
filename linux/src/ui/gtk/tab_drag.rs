use super::*;

// A private boxed type keeps tab identity out of text/file drag destinations.
#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "CmuxPaneTabTransfer")]
struct PaneTabTransfer {
    app: std::sync::Weak<Mutex<AppState>>,
    pane_id: String,
    surface_id: String,
}

pub(super) fn attach_tab(
    container: &gtk::Box,
    select: &gtk::Button,
    pane_id: &str,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: Option<&GtkLocalRefresh>,
) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    source.set_button(gdk::BUTTON_PRIMARY);
    let payload = PaneTabTransfer {
        app: Arc::downgrade(app_state),
        pane_id: pane_id.to_string(),
        surface_id: surface_id.to_string(),
    };
    source
        .connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&payload.to_value())));
    source.connect_drag_begin(|source, _| {
        if let Some(widget) = source.widget() {
            source.set_icon(Some(&gtk::WidgetPaintable::new(Some(&widget))), 0, 0);
        }
    });
    // The close button is a sibling, so it cannot initiate this gesture.
    select.add_controller(source);
    attach_drop_target(
        container,
        pane_id,
        Some(surface_id),
        app_state,
        local_refresh,
    );
}

pub(super) fn attach_strip(
    scroller: &gtk::ScrolledWindow,
    pane_id: &str,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: Option<&GtkLocalRefresh>,
) {
    attach_drop_target(scroller, pane_id, None, app_state, local_refresh);
}

fn clear_marker(target: &gtk::DropTarget) {
    if let Some(widget) = target.widget() {
        widget.remove_css_class("cmux-pane-tab-drop-before");
        widget.remove_css_class("cmux-pane-tab-drop-after");
    }
}

fn show_marker(target: &gtk::DropTarget, x: f64, append: bool) -> gdk::DragAction {
    clear_marker(target);
    let Some(widget) = target.widget() else {
        return gdk::DragAction::empty();
    };
    widget.add_css_class(if append || x >= f64::from(widget.width()) / 2.0 {
        "cmux-pane-tab-drop-after"
    } else {
        "cmux-pane-tab-drop-before"
    });
    gdk::DragAction::MOVE
}

fn attach_drop_target(
    widget: &impl IsA<gtk::Widget>,
    pane_id: &str,
    anchor: Option<&str>,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: Option<&GtkLocalRefresh>,
) {
    let target = gtk::DropTarget::new(PaneTabTransfer::static_type(), gdk::DragAction::MOVE);
    let append = anchor.is_none();
    target.connect_enter(move |target, x, _| show_marker(target, x, append));
    target.connect_motion(move |target, x, _| show_marker(target, x, append));
    target.connect_leave(clear_marker);
    let pane_id = pane_id.to_string();
    let anchor = anchor.map(ToString::to_string);
    let app_state = Arc::clone(app_state);
    let local_refresh = local_refresh.cloned();
    target.connect_drop(move |target, value, x, _| {
        clear_marker(target);
        let Ok(payload) = value.get::<PaneTabTransfer>() else {
            return false;
        };
        if !payload
            .app
            .upgrade()
            .is_some_and(|app| Arc::ptr_eq(&app, &app_state))
        {
            return false;
        }
        let Some(widget) = target.widget() else {
            return false;
        };
        let accepted = {
            let Ok(mut app) = app_state.lock() else {
                return false;
            };
            // A tab can close or move while dragging. Validate before mutating,
            // under the same lock as the shared action.
            let Ok(source) = app.handle_ui("pane.surfaces", &json!({"pane_id": payload.pane_id}))
            else {
                return false;
            };
            if !source["surfaces"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["surface_id"] == payload.surface_id)
            }) {
                return false;
            }
            let mut params = json!({
                "surface_id": payload.surface_id,
                "pane_id": pane_id,
                "focus": true
            });
            if let Some(anchor) = anchor.as_ref() {
                if anchor == &payload.surface_id && pane_id == payload.pane_id {
                    return true;
                }
                let key = if x < f64::from(widget.width()) / 2.0 {
                    "before_surface_id"
                } else {
                    "after_surface_id"
                };
                params[key] = json!(anchor);
            }
            app.handle_ui("surface.move", &params).is_ok()
        };
        if accepted {
            if let Some(refresh) = local_refresh.as_ref() {
                refresh.schedule();
            }
        }
        accepted
    });
    widget.add_controller(target);
}
