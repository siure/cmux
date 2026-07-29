use crate::{
    app::{current_unix_millis, AppState, GlobalWindowCommand},
    browser_omnibar, config, diff_viewer,
    global_shortcuts::GlobalShortcutManager,
    renderer, ui,
};
use anyhow::{anyhow, Result};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

mod mode;
mod shell;
mod strings;
mod style;

use mode::GtkUiMode;
use shell::GtkSnapshotView;

const GTK_CELL_WIDTH: i32 = 10;
const GTK_CELL_HEIGHT: i32 = 20;
const GTK_SPLIT_INITIAL_SETTLE_INTERVAL: Duration = Duration::from_millis(150);
const GTK_SPLIT_STABLE_INTERVAL: Duration = Duration::from_millis(50);
const BROWSER_FOCUS_ESCAPE_INTERVAL: Duration = Duration::from_millis(1600);
const BROWSER_FOCUS_RETRY_ATTEMPTS: u8 = 8;
const BROWSER_RECORDING_CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);
const REACT_GRAB_VERSION: &str = "0.1.29";
const REACT_GRAB_INTEGRITY: &str = "sha256-Sh5xCQ6K2LtgSd6AzMzcD1uxR7n4+4iIbYcWEqx8oEs=";
const BROWSER_TOOLBAR_ICON: &str = "web-browser-symbolic";
const NEW_WORKSPACE_TOOLBAR_LABEL: &str = "+";
const GTK_APPLICATION_ID: &str = "ai.manaflow.cmux";
const GROUP_NEW_WORKSPACE_LABEL: &str = "New Workspace in Group";
const GROUP_EDIT_CONFIG_LABEL: &str = "Edit Group Config";
const GROUP_DOCS_LABEL: &str = "Open Workspace Groups Docs";
const GROUP_DELETE_LABEL: &str = "Delete Group (Close Workspaces)";
const GROUP_DELETE_CONFIRM_LABEL: &str = "Confirm close workspaces";
const WORKSPACE_NEW_GROUP_LABEL: &str = "New Group from Workspace";
const WORKSPACE_REMOVE_FROM_GROUP_LABEL: &str = "Remove from Group";
const WORKSPACE_MOVE_TO_GROUP_PREFIX: &str = "Move to";
const WORKSPACE_CLEAR_NAME_LABEL: &str = "Clear Workspace Name";
const SURFACE_RENAME_LABEL: &str = "Rename Tab";
const SURFACE_CLEAR_NAME_LABEL: &str = "Clear Tab Name";
const SURFACE_DETACH_LABEL: &str = "Move Tab to New Workspace";
pub(crate) const GTK_APP_DEFAULT_WIDTH: i32 = 1180;
pub(crate) const GTK_APP_DEFAULT_HEIGHT: i32 = 760;

pub fn run_gtk_app(app_state: Arc<Mutex<AppState>>, single_instance: bool) -> Result<()> {
    run_gtk_app_with_renderer(
        app_state,
        GtkRendererMode::Gtk,
        GtkUiMode::from_env()?,
        single_instance,
    )
}

pub fn run_gtk_app_with_ghostty(
    app_state: Arc<Mutex<AppState>>,
    single_instance: bool,
) -> Result<()> {
    run_gtk_app_with_renderer(
        app_state,
        GtkRendererMode::Ghostty,
        GtkUiMode::from_env()?,
        single_instance,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GtkRendererMode {
    Gtk,
    Ghostty,
}

fn run_gtk_app_with_renderer(
    app_state: Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    single_instance: bool,
) -> Result<()> {
    crate::browser_runtime::activate_browser_runtime();
    let application = gtk::Application::builder()
        .application_id(GTK_APPLICATION_ID)
        .flags(gtk_application_flags(single_instance))
        .build();
    let window_hosts = Rc::new(RefCell::new(HashMap::new()));
    let desktop_notifications = Rc::new(RefCell::new(None));
    let presented_model_window = Rc::new(RefCell::new(None));
    let sync_started = Rc::new(Cell::new(false));
    let global_visibility = Rc::new(RefCell::new(GtkGlobalVisibilityState::default()));
    let _global_shortcut_manager = GlobalShortcutManager::start(Arc::clone(&app_state));

    let open_notification =
        gio::SimpleAction::new("open-notification", Some(glib::VariantTy::STRING));
    let notification_app_state = Arc::clone(&app_state);
    let notification_window_hosts = Rc::clone(&window_hosts);
    let notification_global_visibility = Rc::clone(&global_visibility);
    open_notification.connect_activate(move |_, parameter| {
        let Some(notification_id) = parameter.and_then(|value| value.get::<String>()) else {
            return;
        };
        call_app(
            &notification_app_state,
            "notification.open",
            json!({"notification_id": notification_id}),
        );
        notification_global_visibility.borrow_mut().hidden = false;
        present_current_gtk_window(&notification_app_state, &notification_window_hosts);
    });
    application.add_action(&open_notification);

    let activate_window_hosts = Rc::clone(&window_hosts);
    let activate_desktop_notifications = Rc::clone(&desktop_notifications);
    let activate_presented_model_window = Rc::clone(&presented_model_window);
    let activate_sync_started = Rc::clone(&sync_started);
    let activate_global_visibility = Rc::clone(&global_visibility);
    application.connect_activate(move |application| {
        if let Err(err) = style::install() {
            eprintln!("error: failed to install GTK resources: {err:#}");
            application.quit();
            return;
        }
        activate_global_visibility.borrow_mut().hidden = false;
        let local_refresh = GtkLocalRefresh::new(
            application,
            &app_state,
            renderer_mode,
            ui_mode,
            &activate_window_hosts,
            &activate_desktop_notifications,
            &activate_presented_model_window,
            &activate_global_visibility,
        );
        if !sync_gtk_window_hosts(
            application,
            &app_state,
            renderer_mode,
            ui_mode,
            &activate_window_hosts,
            &activate_desktop_notifications,
            &activate_presented_model_window,
            &activate_global_visibility,
            &local_refresh,
        ) {
            return;
        }
        if activate_sync_started.replace(true) {
            present_current_gtk_window(&app_state, &activate_window_hosts);
            return;
        }

        let runtime_window_hosts = Rc::clone(&activate_window_hosts);
        glib::timeout_add_local(Duration::from_millis(10), move || {
            process_browser_evaluation_requests(&runtime_window_hosts);
            process_browser_screenshot_requests(&runtime_window_hosts);
            process_browser_pdf_requests(&runtime_window_hosts);
            glib::ControlFlow::Continue
        });

        let recording_app_state = Arc::clone(&app_state);
        let recording_window_hosts = Rc::clone(&activate_window_hosts);
        let recording_in_flight = Rc::new(RefCell::new(HashMap::new()));
        glib::timeout_add_local(Duration::from_millis(250), move || {
            process_browser_recording_captures(
                &recording_app_state,
                &recording_window_hosts,
                &recording_in_flight,
            );
            glib::ControlFlow::Continue
        });

        let sync_application = application.clone();
        let sync_app_state = Arc::clone(&app_state);
        let sync_window_hosts = Rc::clone(&activate_window_hosts);
        let sync_desktop_notifications = Rc::clone(&activate_desktop_notifications);
        let sync_presented_model_window = Rc::clone(&activate_presented_model_window);
        let sync_global_visibility = Rc::clone(&activate_global_visibility);
        let sync_local_refresh = local_refresh.clone();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            if !process_global_window_commands(
                &sync_application,
                &sync_app_state,
                &sync_window_hosts,
                &sync_global_visibility,
            ) {
                return glib::ControlFlow::Break;
            }
            if !sync_gtk_window_hosts(
                &sync_application,
                &sync_app_state,
                renderer_mode,
                ui_mode,
                &sync_window_hosts,
                &sync_desktop_notifications,
                &sync_presented_model_window,
                &sync_global_visibility,
                &sync_local_refresh,
            ) {
                return glib::ControlFlow::Break;
            }
            persist_dirty_ghostty_session_snapshot(&sync_app_state);
            glib::ControlFlow::Continue
        });
    });

    // cmux has already parsed its CLI. Do not let GApplication reinterpret
    // options such as `--renderer` and `--socket` as GTK arguments.
    let _ = application.run_with_args(&["cmux"]);
    crate::browser_runtime::deactivate_browser_runtime();
    Ok(())
}

fn gtk_application_flags(single_instance: bool) -> gio::ApplicationFlags {
    if single_instance {
        gio::ApplicationFlags::empty()
    } else {
        gio::ApplicationFlags::NON_UNIQUE
    }
}

#[derive(Clone)]
struct GtkLocalRefresh {
    application: gtk::Application,
    app_state: Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    hosts: GtkWindowHosts,
    desktop_notifications: Rc<RefCell<Option<DesktopNotificationTracker>>>,
    presented_model_window: Rc<RefCell<Option<String>>>,
    global_visibility: Rc<RefCell<GtkGlobalVisibilityState>>,
    pending: Rc<Cell<bool>>,
}

impl GtkLocalRefresh {
    #[allow(clippy::too_many_arguments)]
    fn new(
        application: &gtk::Application,
        app_state: &Arc<Mutex<AppState>>,
        renderer_mode: GtkRendererMode,
        ui_mode: GtkUiMode,
        hosts: &GtkWindowHosts,
        desktop_notifications: &Rc<RefCell<Option<DesktopNotificationTracker>>>,
        presented_model_window: &Rc<RefCell<Option<String>>>,
        global_visibility: &Rc<RefCell<GtkGlobalVisibilityState>>,
    ) -> Self {
        Self {
            application: application.clone(),
            app_state: Arc::clone(app_state),
            renderer_mode,
            ui_mode,
            hosts: Rc::clone(hosts),
            desktop_notifications: Rc::clone(desktop_notifications),
            presented_model_window: Rc::clone(presented_model_window),
            global_visibility: Rc::clone(global_visibility),
            pending: Rc::new(Cell::new(false)),
        }
    }

    fn schedule(&self) {
        if self.pending.replace(true) {
            return;
        }
        let refresh = self.clone();
        glib::idle_add_local_once(move || {
            refresh.pending.set(false);
            sync_gtk_window_hosts(
                &refresh.application,
                &refresh.app_state,
                refresh.renderer_mode,
                refresh.ui_mode,
                &refresh.hosts,
                &refresh.desktop_notifications,
                &refresh.presented_model_window,
                &refresh.global_visibility,
                &refresh,
            );
        });
    }
}

struct GtkWindowHost {
    window: gtk::ApplicationWindow,
    snapshot_view: GtkSnapshotView,
    pane_allocations: PaneAllocations,
    ghostty_widgets: GhosttySurfaceWidgets,
    browser_controls: BrowserSurfaceControlsCache,
    diff_controls: DiffSurfaceControlsCache,
    pending_browser_shortcut_actions: PendingBrowserShortcutActions,
    terminal_search_controls: TerminalSearchControlsCache,
    terminal_text_box_controls: TerminalTextBoxControlsCache,
    canvas_minimap_states: GtkCanvasMinimapStates,
    canvas_occlusion_states: GtkCanvasOcclusionStates,
    presented_resume_prompts: HashSet<String>,
    presented_close_confirmations: HashSet<String>,
    last_left_rebuild_key: Value,
    last_main_rebuild_key: Value,
    last_main_non_tab_rebuild_key: Value,
    last_main_structure_rebuild_key: Value,
    last_pane_rebuild_keys: HashMap<String, Value>,
    last_right_rebuild_key: Value,
    last_header_rebuild_key: Value,
    last_overlay_rebuild_key: Value,
    last_right_sidebar_focus_generation: u64,
}

type GtkWindowHosts = Rc<RefCell<HashMap<String, GtkWindowHost>>>;
type PendingBrowserShortcutActions = Rc<RefCell<Vec<Value>>>;

fn sync_ghostty_scrollback_widgets(ghostty_widgets: &GhosttySurfaceWidgets) {
    let widgets = ghostty_widgets
        .borrow()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for widget in widgets {
        widget.sync_scrollback_snapshot();
    }
}

fn sync_all_ghostty_scrollback(hosts: &GtkWindowHosts) {
    let widget_caches = hosts
        .borrow()
        .values()
        .map(|host| Rc::clone(&host.ghostty_widgets))
        .collect::<Vec<_>>();
    for cache in widget_caches {
        sync_ghostty_scrollback_widgets(&cache);
    }
}

fn persist_ghostty_session_snapshot(app_state: &Arc<Mutex<AppState>>) {
    if let Ok(mut app) = app_state.lock() {
        let _ = app.persist_embedded_terminal_session_snapshot();
    }
}

fn persist_dirty_ghostty_session_snapshot(app_state: &Arc<Mutex<AppState>>) {
    if let Ok(mut app) = app_state.lock() {
        let _ = app.persist_dirty_embedded_terminal_session_snapshot();
    }
}

fn process_browser_evaluation_requests(hosts: &GtkWindowHosts) {
    for request in crate::browser_runtime::take_browser_evaluation_requests() {
        if request.expired() {
            request.unavailable();
            continue;
        }
        let controls = {
            let hosts = hosts.borrow();
            hosts.values().find_map(|host| {
                host.browser_controls
                    .borrow()
                    .get(request.surface_id())
                    .cloned()
            })
        };
        let Some(controls) = controls else {
            crate::browser_runtime::requeue_browser_evaluation_request(request);
            continue;
        };
        let Some(view) = controls.web_view else {
            request.unavailable();
            continue;
        };
        let script = request.script().to_string();
        let callback_request = request.clone();
        if let Err(err) = view.evaluate_javascript_with_result(&script, move |result| {
            callback_request.complete(result);
        }) {
            eprintln!("cmux: native WebKit evaluation request failed: {err}");
            request.complete(Err(err));
        }
    }
}

fn process_browser_screenshot_requests(hosts: &GtkWindowHosts) {
    for request in crate::browser_runtime::take_browser_screenshot_requests() {
        if request.expired() {
            request.unavailable();
            continue;
        }
        let controls = {
            let hosts = hosts.borrow();
            hosts.values().find_map(|host| {
                host.browser_controls
                    .borrow()
                    .get(request.surface_id())
                    .cloned()
            })
        };
        let Some(controls) = controls else {
            crate::browser_runtime::requeue_browser_screenshot_request(request);
            continue;
        };
        let Some(view) = controls.web_view else {
            request.unavailable();
            continue;
        };
        let full_document = request.full_document();
        view.capture_snapshot(full_document, move |result| {
            request.complete(
                result.map(|snapshot| crate::browser_runtime::BrowserScreenshot {
                    png: snapshot.png,
                    width: snapshot.width,
                    height: snapshot.height,
                }),
            );
        });
    }
}

fn process_browser_pdf_requests(hosts: &GtkWindowHosts) {
    for request in crate::browser_runtime::take_browser_pdf_requests() {
        if request.expired() {
            request.unavailable();
            continue;
        }
        let controls = {
            let hosts = hosts.borrow();
            hosts.values().find_map(|host| {
                host.browser_controls
                    .borrow()
                    .get(request.surface_id())
                    .cloned()
            })
        };
        let Some(controls) = controls else {
            crate::browser_runtime::requeue_browser_pdf_request(request);
            continue;
        };
        let Some(view) = controls.web_view else {
            request.unavailable();
            continue;
        };
        let callback_request = request.clone();
        if let Err(err) = view.print_to_pdf(move |result| {
            callback_request.complete(
                result.map(|pdf| crate::browser_runtime::BrowserPdf { bytes: pdf.bytes }),
            );
        }) {
            eprintln!("cmux: native WebKit PDF request failed: {err}");
            request.complete(Err(err));
        }
    }
}

fn process_browser_recording_captures(
    app_state: &Arc<Mutex<AppState>>,
    hosts: &GtkWindowHosts,
    in_flight: &Rc<RefCell<HashMap<String, Instant>>>,
) {
    in_flight
        .borrow_mut()
        .retain(|_, started_at| started_at.elapsed() < BROWSER_RECORDING_CAPTURE_TIMEOUT);
    let targets = match app_state.lock() {
        Ok(app) => app.browser_runtime_recording_targets(),
        Err(_) => return,
    };
    for target in targets {
        if in_flight.borrow().contains_key(&target.surface_id) {
            continue;
        }
        let view = {
            let hosts = hosts.borrow();
            hosts.values().find_map(|host| {
                host.browser_controls
                    .borrow()
                    .get(&target.surface_id)
                    .and_then(|controls| controls.web_view.clone())
            })
        };
        let Some(view) = view else {
            continue;
        };

        let started_at = Instant::now();
        in_flight
            .borrow_mut()
            .insert(target.surface_id.clone(), started_at);
        let callback_app_state = Arc::clone(app_state);
        let callback_in_flight = Rc::clone(in_flight);
        view.capture_snapshot(false, move |result| {
            let mut in_flight = callback_in_flight.borrow_mut();
            if in_flight.get(&target.surface_id) == Some(&started_at) {
                in_flight.remove(&target.surface_id);
            }
            drop(in_flight);
            let snapshot = match result {
                Ok(snapshot) => snapshot,
                Err(err) => {
                    eprintln!("cmux: native WebKit recording snapshot failed: {err}");
                    return;
                }
            };
            let Ok(mut app) = callback_app_state.lock() else {
                return;
            };
            if let Err(err) = app.browser_runtime_record_frame(
                &target,
                snapshot.png,
                snapshot.width,
                snapshot.height,
            ) {
                eprintln!("cmux: native WebKit recording frame was rejected: {err}");
            }
        });
    }
}

#[derive(Default)]
struct GtkGlobalVisibilityState {
    hidden: bool,
    restore_window_ids: HashSet<String>,
}

fn sync_gtk_window_hosts(
    application: &gtk::Application,
    app_state: &Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    hosts: &GtkWindowHosts,
    desktop_notifications: &Rc<RefCell<Option<DesktopNotificationTracker>>>,
    presented_model_window: &Rc<RefCell<Option<String>>>,
    global_visibility: &Rc<RefCell<GtkGlobalVisibilityState>>,
    local_refresh: &GtkLocalRefresh,
) -> bool {
    let rows = model_window_rows(app_state);
    if rows.is_empty() {
        for (_, host) in hosts.borrow_mut().drain() {
            host.window.destroy();
        }
        application.quit();
        return false;
    }

    let live_ids = rows
        .iter()
        .filter_map(model_window_id)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let removed_ids = hosts
        .borrow()
        .keys()
        .filter(|id| !live_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in removed_ids {
        if let Some(host) = hosts.borrow_mut().remove(&id) {
            host.window.destroy();
        }
    }

    let mut selected_snapshot = None;
    let mut selected_window_id = None;
    for row in rows {
        let Some(window_id) = model_window_id(&row).map(str::to_string) else {
            continue;
        };
        let snapshot = snapshot_or_error(app_state, renderer_mode, &window_id);
        let selected = row
            .get("selected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !hosts.borrow().contains_key(&window_id) {
            let host = create_gtk_window_host(
                application,
                app_state,
                renderer_mode,
                ui_mode,
                &window_id,
                &row,
                &snapshot,
                local_refresh,
            );
            if !global_visibility.borrow().hidden {
                host.window.present();
            }
            hosts.borrow_mut().insert(window_id.clone(), host);
        } else if let Some(host) = hosts.borrow_mut().get_mut(&window_id) {
            refresh_gtk_window_host(
                host,
                app_state,
                renderer_mode,
                ui_mode,
                &row,
                &snapshot,
                local_refresh,
            );
        }
        if selected {
            selected_window_id = Some(window_id);
            selected_snapshot = Some(snapshot);
        }
    }

    if let Some(snapshot) = selected_snapshot.as_ref() {
        let mut tracker = desktop_notifications.borrow_mut();
        if let Some(tracker) = tracker.as_mut() {
            sync_desktop_notifications(application, snapshot, tracker);
        } else {
            *tracker = Some(DesktopNotificationTracker::from_snapshot(snapshot));
        }
    }

    if !global_visibility.borrow().hidden && selected_window_id != *presented_model_window.borrow()
    {
        if let Some(window_id) = selected_window_id.as_ref() {
            if let Some(host) = hosts.borrow().get(window_id) {
                host.window.present();
            }
        }
        *presented_model_window.borrow_mut() = selected_window_id;
    }
    true
}

fn create_gtk_window_host(
    application: &gtk::Application,
    app_state: &Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    window_id: &str,
    row: &Value,
    snapshot: &Value,
    local_refresh: &GtkLocalRefresh,
) -> GtkWindowHost {
    let pane_allocations = Rc::new(RefCell::new(HashMap::new()));
    let ghostty_widgets = Rc::new(RefCell::new(HashMap::new()));
    let browser_controls = Rc::new(RefCell::new(HashMap::new()));
    let diff_controls = Rc::new(RefCell::new(HashMap::new()));
    let pending_browser_shortcut_actions = Rc::new(RefCell::new(Vec::new()));
    let terminal_search_controls = Rc::new(RefCell::new(HashMap::new()));
    let terminal_text_box_controls = Rc::new(RefCell::new(HashMap::new()));
    let canvas_minimap_states = Rc::new(RefCell::new(HashMap::new()));
    let canvas_occlusion_states = Rc::new(RefCell::new(HashMap::new()));
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title(model_window_title(row))
        .default_width(GTK_APP_DEFAULT_WIDTH)
        .default_height(GTK_APP_DEFAULT_HEIGHT)
        .fullscreened(model_window_fullscreen(row))
        .build();
    connect_terminal_keys(
        &window,
        app_state,
        &ghostty_widgets,
        &browser_controls,
        &diff_controls,
        &pending_browser_shortcut_actions,
        window_id,
    );
    let snapshot_view = shell::build_snapshot_view(
        snapshot,
        app_state,
        &pane_allocations,
        &ghostty_widgets,
        &browser_controls,
        &diff_controls,
        &terminal_search_controls,
        &terminal_text_box_controls,
        &canvas_minimap_states,
        &canvas_occlusion_states,
        renderer_mode,
        ui_mode,
        window_id,
        local_refresh,
    );
    if let Some(titlebar) = snapshot_view.titlebar.as_ref() {
        window.set_titlebar(Some(titlebar));
    }
    window.set_child(Some(&snapshot_view.root));

    let focus_app_state = Arc::clone(app_state);
    let focus_application = application.clone();
    let focus_window_id = window_id.to_string();
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            call_app(
                &focus_app_state,
                "window.focus",
                json!({"window_id": focus_window_id}),
            );
        }
        let state = if focus_application.active_window().is_some() {
            "active"
        } else {
            "inactive"
        };
        call_app(
            &focus_app_state,
            "app.focus_override.set",
            json!({"state": state}),
        );
    });

    let fullscreen_app_state = Arc::clone(app_state);
    let fullscreen_window_id = window_id.to_string();
    window.connect_fullscreened_notify(move |window| {
        call_app(
            &fullscreen_app_state,
            "window.fullscreen.set",
            json!({
                "window_id": fullscreen_window_id,
                "fullscreen": window.is_fullscreen()
            }),
        );
    });

    let close_app_state = Arc::clone(app_state);
    let close_application = application.clone();
    let close_window_id = window_id.to_string();
    let close_ghostty_widgets = Rc::clone(&ghostty_widgets);
    window.connect_close_request(move |_| {
        sync_ghostty_scrollback_widgets(&close_ghostty_widgets);
        let Some(result) = call_app_value(
            &close_app_state,
            "debug.window.close_request",
            json!({"window_id": close_window_id, "source": "window_button"}),
        ) else {
            return glib::Propagation::Stop;
        };
        if result.get("blocked").and_then(Value::as_bool) == Some(true) {
            return glib::Propagation::Stop;
        }
        if result.get("quit").and_then(Value::as_bool) == Some(true) {
            persist_ghostty_session_snapshot(&close_app_state);
            close_application.quit();
        }
        glib::Propagation::Proceed
    });

    let rebuild_keys = snapshot_region_rebuild_keys_for_mode(snapshot, ui_mode);
    let mut host = GtkWindowHost {
        window,
        snapshot_view,
        pane_allocations,
        ghostty_widgets,
        browser_controls,
        diff_controls,
        pending_browser_shortcut_actions,
        terminal_search_controls,
        terminal_text_box_controls,
        canvas_minimap_states,
        canvas_occlusion_states,
        presented_resume_prompts: HashSet::new(),
        presented_close_confirmations: HashSet::new(),
        last_left_rebuild_key: rebuild_keys.left,
        last_main_rebuild_key: rebuild_keys.main,
        last_main_non_tab_rebuild_key: rebuild_keys.main_without_tabs,
        last_main_structure_rebuild_key: snapshot_main_structure_rebuild_key(snapshot),
        last_pane_rebuild_keys: snapshot_pane_rebuild_keys(snapshot),
        last_right_rebuild_key: rebuild_keys.right,
        last_header_rebuild_key: shell::header_rebuild_key(snapshot),
        last_overlay_rebuild_key: shell::overlay_rebuild_key(snapshot),
        last_right_sidebar_focus_generation: 0,
    };
    sync_resume_command_prompts(&mut host, snapshot, app_state);
    sync_close_confirmation_prompts(&mut host, snapshot, app_state);
    host
}

fn refresh_gtk_window_host(
    host: &mut GtkWindowHost,
    app_state: &Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    row: &Value,
    snapshot: &Value,
    local_refresh: &GtkLocalRefresh,
) {
    host.window.set_title(Some(model_window_title(row)));
    let fullscreen = model_window_fullscreen(row);
    if host.window.is_fullscreen() != fullscreen {
        host.window.set_fullscreened(fullscreen);
    }
    sync_resume_command_prompts(host, snapshot, app_state);
    sync_close_confirmation_prompts(host, snapshot, app_state);
    sync_terminal_search_controls(snapshot, &host.terminal_search_controls);
    let focus_generation = right_sidebar_focus_generation(snapshot);
    let focus_right_sidebar =
        focus_generation != host.last_right_sidebar_focus_generation && focus_generation > 0;
    let rebuild_keys = snapshot_region_rebuild_keys_for_mode(snapshot, ui_mode);
    let header_rebuild_key = shell::header_rebuild_key(snapshot);
    if host.last_header_rebuild_key != header_rebuild_key {
        shell::refresh_header(&host.snapshot_view, snapshot, app_state);
        host.last_header_rebuild_key = header_rebuild_key;
    }
    let overlay_rebuild_key = shell::overlay_rebuild_key(snapshot);
    if ui_mode.is_next() && host.last_overlay_rebuild_key != overlay_rebuild_key {
        let window_id = model_window_id(row).unwrap_or_default();
        shell::refresh_overlay(&host.snapshot_view, snapshot, app_state, window_id);
        host.last_overlay_rebuild_key = overlay_rebuild_key;
    }
    let focused = gtk::prelude::GtkWindowExt::focus(&host.window);
    let left_rebuild_suppressed =
        widget_or_ancestor_has_css_class(focused.as_ref(), "cmux-custom-sidebar-input");
    let main_rebuild_suppressed =
        widget_or_ancestor_has_css_class(focused.as_ref(), "cmux-browser-location")
            || widget_or_ancestor_has_css_class(focused.as_ref(), "cmux-terminal-search");
    let right_structure_changed = host.last_right_rebuild_key.pointer("/state/visible")
        != rebuild_keys.right.pointer("/state/visible")
        || host.last_right_rebuild_key.pointer("/state/mode")
            != rebuild_keys.right.pointer("/state/mode");
    let right_rebuild_suppressed =
        widget_or_ancestor_has_css_class(focused.as_ref(), "cmux-right-sidebar-input")
            && !focus_right_sidebar
            && !right_structure_changed;

    if host.last_left_rebuild_key != rebuild_keys.left && !left_rebuild_suppressed {
        replace_snapshot_slot_child(
            &host.snapshot_view.left_slot,
            Some(workspace_sidebar(snapshot, app_state, ui_mode).upcast()),
        );
        host.last_left_rebuild_key = rebuild_keys.left;
    }

    let main_changed = host.last_main_rebuild_key != rebuild_keys.main;
    if main_changed {
        let main_structure_rebuild_key = snapshot_main_structure_rebuild_key(snapshot);
        let pane_rebuild_keys = snapshot_pane_rebuild_keys(snapshot);
        if host.last_main_non_tab_rebuild_key == rebuild_keys.main_without_tabs {
            sync_pane_tab_strips(
                &host.window,
                snapshot,
                &host.last_pane_rebuild_keys,
                &pane_rebuild_keys,
                app_state,
                local_refresh,
            );
            host.last_main_rebuild_key = rebuild_keys.main;
            host.last_main_non_tab_rebuild_key = rebuild_keys.main_without_tabs;
            host.last_pane_rebuild_keys = pane_rebuild_keys;
        } else if host.last_main_structure_rebuild_key == main_structure_rebuild_key
            && snapshot.pointer("/canvas/mode").and_then(Value::as_str) != Some("canvas")
            && sync_pane_surface_cards(
                &host.window,
                snapshot,
                &host.last_pane_rebuild_keys,
                &pane_rebuild_keys,
                app_state,
                &host.pane_allocations,
                &host.ghostty_widgets,
                &host.browser_controls,
                &host.diff_controls,
                &host.terminal_search_controls,
                &host.terminal_text_box_controls,
                renderer_mode,
                ui_mode,
                local_refresh,
            )
        {
            host.last_main_rebuild_key = rebuild_keys.main;
            host.last_main_non_tab_rebuild_key = rebuild_keys.main_without_tabs;
            host.last_main_structure_rebuild_key = main_structure_rebuild_key;
            host.last_pane_rebuild_keys = pane_rebuild_keys;
        } else if !main_rebuild_suppressed {
            replace_snapshot_slot_child(
                &host.snapshot_view.main_slot,
                Some(
                    surface_area(
                        snapshot,
                        app_state,
                        &host.pane_allocations,
                        &host.ghostty_widgets,
                        &host.browser_controls,
                        &host.diff_controls,
                        &host.terminal_search_controls,
                        &host.terminal_text_box_controls,
                        &host.canvas_minimap_states,
                        &host.canvas_occlusion_states,
                        renderer_mode,
                        ui_mode,
                        local_refresh,
                    )
                    .upcast(),
                ),
            );
            flush_pending_browser_shortcut_actions(
                &host.browser_controls,
                &host.pending_browser_shortcut_actions,
            );
            host.last_main_rebuild_key = rebuild_keys.main;
            host.last_main_non_tab_rebuild_key = rebuild_keys.main_without_tabs;
            host.last_main_structure_rebuild_key = main_structure_rebuild_key;
            host.last_pane_rebuild_keys = pane_rebuild_keys;
        }
    }
    if host.last_right_rebuild_key != rebuild_keys.right && !right_rebuild_suppressed {
        let sidebar = right_sidebar_visible(snapshot)
            .then(|| app_chrome_sidebar(snapshot, app_state).upcast());
        replace_snapshot_slot_child(&host.snapshot_view.right_slot, sidebar);
        host.last_right_rebuild_key = rebuild_keys.right;
    }

    sync_ghostty_surface_widgets(
        snapshot,
        app_state,
        &host.ghostty_widgets,
        &host.canvas_occlusion_states,
        renderer_mode,
    );
    if focus_right_sidebar {
        focus_right_sidebar_widget(&host.window);
    }
    host.last_right_sidebar_focus_generation = focus_generation;
}

fn replace_snapshot_slot_child(slot: &gtk::Box, child: Option<gtk::Widget>) {
    let visible = child.is_some();
    while let Some(current) = slot.first_child() {
        slot.remove(&current);
    }
    if let Some(child) = child {
        slot.append(&child);
    }
    slot.set_visible(visible);
}

fn sync_resume_command_prompts(
    host: &mut GtkWindowHost,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let pending = resume_command_prompts(snapshot);
    let pending_ids = pending
        .iter()
        .map(|prompt| prompt.surface_id.clone())
        .collect::<HashSet<_>>();
    host.presented_resume_prompts
        .retain(|surface_id| pending_ids.contains(surface_id));
    for prompt in pending {
        let GtkResumeCommandPrompt {
            surface_id,
            command,
            cwd,
        } = prompt;
        if !host.presented_resume_prompts.insert(surface_id.clone()) {
            continue;
        }
        let dialog = gtk::Dialog::builder()
            .title("Run Resume Command?")
            .transient_for(&host.window)
            .modal(true)
            .destroy_with_parent(true)
            .build();
        dialog.add_button("Skip", gtk::ResponseType::Reject);
        dialog.add_button("Run", gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Accept);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        let message = gtk::Label::new(Some(
            "cmux is restoring a terminal with this approved resume command:",
        ));
        message.set_xalign(0.0);
        message.set_wrap(true);
        content.append(&message);
        let command_label = gtk::Label::new(Some(&command));
        command_label.set_xalign(0.0);
        command_label.set_selectable(true);
        command_label.set_wrap(true);
        command_label.add_css_class("monospace");
        content.append(&command_label);
        let cwd_label = gtk::Label::new(Some(&format!(
            "Working directory: {}",
            cwd.as_deref().unwrap_or("None")
        )));
        cwd_label.set_xalign(0.0);
        cwd_label.add_css_class("cmux-muted");
        content.append(&cwd_label);
        dialog.content_area().append(&content);
        let app_state = Arc::clone(app_state);
        dialog.connect_response(move |dialog, response| {
            call_app(
                &app_state,
                "surface.resume.run",
                json!({
                    "surface_id": surface_id,
                    "run": response == gtk::ResponseType::Accept
                }),
            );
            dialog.destroy();
        });
        dialog.present();
    }
}

#[derive(Debug, PartialEq, Eq)]
struct GtkResumeCommandPrompt {
    surface_id: String,
    command: String,
    cwd: Option<String>,
}

fn resume_command_prompts(snapshot: &Value) -> Vec<GtkResumeCommandPrompt> {
    snapshot
        .get("surfaces")
        .or_else(|| snapshot.get("surface_views"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|surface| {
            surface.get("resume_restore_state").and_then(Value::as_str) == Some("prompt")
        })
        .filter_map(|surface| {
            let surface_id =
                value_string(surface, "surface_id").or_else(|| value_string(surface, "id"))?;
            let binding = surface.get("resume_binding")?;
            Some(GtkResumeCommandPrompt {
                surface_id,
                command: value_str(binding, "command", "").to_string(),
                cwd: value_string(binding, "cwd"),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkCloseConfirmationPrompt {
    id: String,
    title: String,
    message: String,
    accept_label: String,
    cancel_label: String,
}

fn close_confirmation_prompts(snapshot: &Value) -> Vec<GtkCloseConfirmationPrompt> {
    snapshot
        .get("close_confirmations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|request| {
            Some(GtkCloseConfirmationPrompt {
                id: value_string(request, "id")?,
                title: value_str(request, "title", "Confirm close").to_string(),
                message: value_str(request, "message", "Close this item?").to_string(),
                accept_label: value_str(request, "accept_label", "Close").to_string(),
                cancel_label: value_str(request, "cancel_label", "Cancel").to_string(),
            })
        })
        .collect()
}

fn sync_close_confirmation_prompts(
    host: &mut GtkWindowHost,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let pending = close_confirmation_prompts(snapshot);
    let pending_ids = pending
        .iter()
        .map(|prompt| prompt.id.clone())
        .collect::<HashSet<_>>();
    host.presented_close_confirmations
        .retain(|id| pending_ids.contains(id));
    for prompt in pending {
        if !host.presented_close_confirmations.insert(prompt.id.clone()) {
            continue;
        }
        let dialog = gtk::Dialog::builder()
            .title(&prompt.title)
            .transient_for(&host.window)
            .modal(true)
            .destroy_with_parent(true)
            .build();
        dialog.add_button(&prompt.cancel_label, gtk::ResponseType::Reject);
        dialog.add_button(&prompt.accept_label, gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Reject);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        let message = gtk::Label::new(Some(&prompt.message));
        message.set_xalign(0.0);
        message.set_wrap(true);
        content.append(&message);
        dialog.content_area().append(&content);
        let app_state = Arc::clone(app_state);
        let request_id = prompt.id;
        dialog.connect_response(move |dialog, response| {
            call_app(
                &app_state,
                "app.close_confirmation.reply",
                json!({
                    "id": request_id,
                    "confirmed": response == gtk::ResponseType::Accept
                }),
            );
            dialog.destroy();
        });
        dialog.present();
    }
}

fn right_sidebar_focus_generation(snapshot: &Value) -> u64 {
    snapshot
        .pointer("/right_sidebar/focus_generation")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn focus_right_sidebar_widget(window: &gtk::ApplicationWindow) {
    let Some(root) = window.child() else {
        return;
    };
    if let Some(sidebar) = widget_descendant_with_css_class(&root, "cmux-chrome") {
        sidebar.grab_focus();
    }
}

fn widget_descendant_with_css_class(root: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if root.has_css_class(class) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = widget_descendant_with_css_class(&widget, class) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn model_window_rows(app_state: &Arc<Mutex<AppState>>) -> Vec<Value> {
    let Ok(mut app) = app_state.lock() else {
        return Vec::new();
    };
    app.handle("window.list", &json!({}))
        .ok()
        .and_then(|value| value.get("windows").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

fn model_window_id(window: &Value) -> Option<&str> {
    window
        .get("id")
        .or_else(|| window.get("window_id"))
        .and_then(Value::as_str)
}

fn model_window_title(window: &Value) -> &str {
    window
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())
        .unwrap_or("cmux Linux")
}

fn model_window_fullscreen(window: &Value) -> bool {
    window
        .get("fullscreen")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn present_current_gtk_window(app_state: &Arc<Mutex<AppState>>, hosts: &GtkWindowHosts) {
    let window_id = app_state.lock().ok().and_then(|mut app| {
        app.handle("window.current", &json!({}))
            .ok()
            .and_then(|value| {
                value
                    .get("window_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    });
    if let Some(host) = window_id.and_then(|window_id| {
        hosts
            .borrow()
            .get(&window_id)
            .map(|host| host.window.clone())
    }) {
        host.present();
    }
}

fn process_global_window_commands(
    application: &gtk::Application,
    app_state: &Arc<Mutex<AppState>>,
    hosts: &GtkWindowHosts,
    visibility: &Rc<RefCell<GtkGlobalVisibilityState>>,
) -> bool {
    let commands = app_state
        .lock()
        .map(|mut app| app.drain_global_window_commands())
        .unwrap_or_default();
    for command in commands {
        match command {
            GlobalWindowCommand::ShowCurrent => {
                visibility.borrow_mut().hidden = false;
                present_current_gtk_window(app_state, hosts);
            }
            GlobalWindowCommand::ToggleAll => {
                let should_hide = application
                    .active_window()
                    .is_some_and(|window| window.is_visible());
                if should_hide {
                    let visible_ids = hosts
                        .borrow()
                        .iter()
                        .filter(|(_, host)| host.window.is_visible())
                        .map(|(id, _)| id.clone())
                        .collect::<HashSet<_>>();
                    let mut state = visibility.borrow_mut();
                    state.hidden = true;
                    state.restore_window_ids = visible_ids.clone();
                    drop(state);
                    for id in visible_ids {
                        if let Some(host) = hosts.borrow().get(&id) {
                            host.window.set_visible(false);
                        }
                    }
                    call_app(
                        app_state,
                        "app.focus_override.set",
                        json!({"state": "inactive"}),
                    );
                } else {
                    let restore_ids = {
                        let mut state = visibility.borrow_mut();
                        state.hidden = false;
                        std::mem::take(&mut state.restore_window_ids)
                    };
                    let mut restored = false;
                    let mut focus_window = None;
                    for id in restore_ids {
                        if let Some(host) = hosts.borrow().get(&id) {
                            host.window.set_visible(true);
                            restored = true;
                            if focus_window.is_none() {
                                focus_window = Some(host.window.clone());
                            }
                        }
                    }
                    if !restored {
                        present_current_gtk_window(app_state, hosts);
                    } else if let Some(window) = focus_window {
                        window.present();
                    }
                }
            }
            GlobalWindowCommand::Quit => {
                sync_all_ghostty_scrollback(hosts);
                persist_ghostty_session_snapshot(app_state);
                application.quit();
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GtkPaneAllocation {
    width: i32,
    height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GtkSurfaceFrame {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkCanvasPlacement {
    view_index: usize,
    pane_id: String,
    surface_target: String,
    focused: bool,
    logical_x: f64,
    logical_y: f64,
    logical_width: f64,
    logical_height: f64,
    scale: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkCanvasLayout {
    workspace_target: String,
    scale: f64,
    logical_origin_x: f64,
    logical_origin_y: f64,
    padding: f64,
    metrics: GtkCanvasMetrics,
    width: f64,
    height: f64,
    viewport_x: f64,
    viewport_y: f64,
    placements: Vec<GtkCanvasPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GtkCanvasFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GtkCanvasMinimapProjection {
    scale: f64,
    origin_x: f64,
    origin_y: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkCanvasMinimapSnapshot {
    panes: Vec<(String, GtkCanvasFrame, bool)>,
    visible: GtkCanvasFrame,
    content_bounds: Option<GtkCanvasFrame>,
    navigation_bounds: GtkCanvasFrame,
}

impl GtkCanvasMinimapSnapshot {
    fn new(panes: Vec<(String, GtkCanvasFrame, bool)>, visible: GtkCanvasFrame) -> Self {
        let content_bounds = panes
            .iter()
            .map(|(_, frame, _)| *frame)
            .reduce(canvas_frame_union);
        let mut navigation_bounds = content_bounds
            .map(|content| canvas_frame_union(content, visible))
            .unwrap_or(visible);
        if navigation_bounds.width < 1.0 {
            navigation_bounds.x -= 0.5;
            navigation_bounds.width = 1.0;
        }
        if navigation_bounds.height < 1.0 {
            navigation_bounds.y -= 0.5;
            navigation_bounds.height = 1.0;
        }
        Self {
            panes,
            visible,
            content_bounds,
            navigation_bounds,
        }
    }

    fn should_show(&self) -> bool {
        if self.visible.width <= 1.0 || self.visible.height <= 1.0 || self.panes.is_empty() {
            return false;
        }
        let Some(content) = self.content_bounds else {
            return false;
        };
        self.panes.len() > 1
            || !canvas_frame_contains(canvas_frame_expand(self.visible, 24.0), content)
    }

    fn projection(&self, drawing: GtkCanvasFrame) -> GtkCanvasMinimapProjection {
        if drawing.width <= 0.0 || drawing.height <= 0.0 {
            return GtkCanvasMinimapProjection {
                scale: 1.0,
                origin_x: drawing.x,
                origin_y: drawing.y,
            };
        }
        let scale = (drawing.width / self.navigation_bounds.width)
            .min(drawing.height / self.navigation_bounds.height);
        let used_width = self.navigation_bounds.width * scale;
        let used_height = self.navigation_bounds.height * scale;
        GtkCanvasMinimapProjection {
            scale,
            origin_x: drawing.x + (drawing.width - used_width) / 2.0,
            origin_y: drawing.y + (drawing.height - used_height) / 2.0,
        }
    }

    fn minimap_frame(&self, canvas: GtkCanvasFrame, drawing: GtkCanvasFrame) -> GtkCanvasFrame {
        let projection = self.projection(drawing);
        GtkCanvasFrame {
            x: projection.origin_x + (canvas.x - self.navigation_bounds.x) * projection.scale,
            y: projection.origin_y + (canvas.y - self.navigation_bounds.y) * projection.scale,
            width: canvas.width * projection.scale,
            height: canvas.height * projection.scale,
        }
    }

    fn projected_navigation_bounds(&self, drawing: GtkCanvasFrame) -> GtkCanvasFrame {
        let projection = self.projection(drawing);
        GtkCanvasFrame {
            x: projection.origin_x,
            y: projection.origin_y,
            width: self.navigation_bounds.width * projection.scale,
            height: self.navigation_bounds.height * projection.scale,
        }
    }

    fn canvas_point(&self, x: f64, y: f64, drawing: GtkCanvasFrame) -> (f64, f64) {
        let projection = self.projection(drawing);
        (
            self.navigation_bounds.x + (x - projection.origin_x) / projection.scale,
            self.navigation_bounds.y + (y - projection.origin_y) / projection.scale,
        )
    }
}

#[derive(Debug, Default)]
struct GtkCanvasMinimapVisibility {
    held: bool,
    visible_until: Option<Instant>,
}

type GtkCanvasMinimapStates = Rc<RefCell<HashMap<String, Rc<RefCell<GtkCanvasMinimapVisibility>>>>>;
type GtkCanvasOcclusionStates = Rc<RefCell<HashMap<String, bool>>>;

fn canvas_frame_union(left: GtkCanvasFrame, right: GtkCanvasFrame) -> GtkCanvasFrame {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    GtkCanvasFrame {
        x,
        y,
        width: (left.x + left.width).max(right.x + right.width) - x,
        height: (left.y + left.height).max(right.y + right.height) - y,
    }
}

fn canvas_frame_expand(frame: GtkCanvasFrame, amount: f64) -> GtkCanvasFrame {
    GtkCanvasFrame {
        x: frame.x - amount,
        y: frame.y - amount,
        width: frame.width + amount * 2.0,
        height: frame.height + amount * 2.0,
    }
}

fn canvas_frame_contains(container: GtkCanvasFrame, content: GtkCanvasFrame) -> bool {
    content.x >= container.x
        && content.y >= container.y
        && content.x + content.width <= container.x + container.width
        && content.y + content.height <= container.y + container.height
}

fn canvas_frames_intersect(left: GtkCanvasFrame, right: GtkCanvasFrame) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

fn canvas_rendering_targets(
    placements: &[GtkCanvasPlacement],
    visible: GtkCanvasFrame,
    margin_fraction: f64,
) -> HashSet<String> {
    let margin_fraction = margin_fraction.max(0.0);
    let render_region = GtkCanvasFrame {
        x: visible.x - visible.width * margin_fraction,
        y: visible.y - visible.height * margin_fraction,
        width: visible.width * (1.0 + margin_fraction * 2.0),
        height: visible.height * (1.0 + margin_fraction * 2.0),
    };
    placements
        .iter()
        .filter(|placement| {
            canvas_frames_intersect(render_region, canvas_placement_frame(placement))
        })
        .map(|placement| placement.surface_target.clone())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GtkCanvasMetrics {
    gap: f64,
    snap_threshold: f64,
    min_width: f64,
    min_height: f64,
    snapping_enabled: bool,
}

impl Default for GtkCanvasMetrics {
    fn default() -> Self {
        Self {
            gap: 16.0,
            snap_threshold: 8.0,
            min_width: 200.0,
            min_height: 120.0,
            snapping_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkCanvasGuideAxis {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GtkCanvasGuide {
    axis: GtkCanvasGuideAxis,
    position: f64,
    span_start: f64,
    span_end: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkCanvasSnapResult {
    frame: GtkCanvasFrame,
    guides: Vec<GtkCanvasGuide>,
}

#[derive(Debug, Clone, Copy)]
struct GtkCanvasSnapCandidate {
    delta: f64,
    guide_position: f64,
    priority: u8,
    neighbor: GtkCanvasFrame,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GtkCanvasResizeEdges {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkCanvasDragRegion {
    Move,
    Resize(GtkCanvasResizeEdges),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkSplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GtkSplitLayout {
    Leaf {
        view_index: usize,
        bounds: GtkSurfaceFrame,
    },
    Split {
        axis: GtkSplitAxis,
        bounds: GtkSurfaceFrame,
        divider: i64,
        leading: Box<GtkSplitLayout>,
        trailing: Box<GtkSplitLayout>,
    },
}

type PaneAllocations = Rc<RefCell<HashMap<String, GtkPaneAllocation>>>;
type GhosttySurfaceWidgets = Rc<RefCell<HashMap<String, crate::gtk_ghostty::GhosttySurfaceWidget>>>;

#[derive(Clone)]
struct BrowserSurfaceControls {
    root: gtk::Box,
    find_bar: gtk::Box,
    find_entry: gtk::SearchEntry,
    profile_id: String,
    profile_data_generation: u64,
    profile_selector: gtk::ComboBoxText,
    profile_choices: Rc<RefCell<Vec<(String, String)>>>,
    profile_change_suppressed: Rc<Cell<bool>>,
    location: gtk::Entry,
    back: gtk::Button,
    forward: gtk::Button,
    focus_mode: gtk::ToggleButton,
    focus_mode_active: Rc<Cell<bool>>,
    web_view: Option<crate::gtk_webkit::GtkWebKitView>,
    model_url: Rc<RefCell<String>>,
    global_search_needle: Rc<RefCell<Option<String>>>,
    page_zoom: Rc<Cell<f64>>,
    user_agent: Rc<RefCell<String>>,
    applied_offline: Rc<Cell<Option<bool>>>,
    request_configuration_generation: Rc<Cell<u64>>,
    applied_init_scripts: Rc<RefCell<Vec<String>>>,
    applied_storage: Rc<RefCell<Option<(String, u64)>>>,
    developer_tools_visible: Rc<Cell<bool>>,
    last_runtime_action_sequence: Rc<Cell<u64>>,
    model_focused: Rc<Cell<bool>>,
    browser_chrome_focused: Rc<Cell<bool>>,
}

type BrowserSurfaceControlsCache = Rc<RefCell<HashMap<String, BrowserSurfaceControls>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct BrowserOmnibarSuggestion {
    kind: String,
    completion: String,
    url: String,
    title: String,
    badge: Option<String>,
    surface_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserFocusEscapeDecision {
    Forward,
    Consume,
    Exit,
}

#[derive(Default)]
struct BrowserFocusEscapeState {
    surface_id: Option<String>,
    armed_at: Option<Instant>,
    released: bool,
}

impl BrowserFocusEscapeState {
    fn press(&mut self, surface_id: &str, now: Instant) -> BrowserFocusEscapeDecision {
        let armed = self.surface_id.as_deref() == Some(surface_id)
            && self.armed_at.is_some_and(|armed_at| {
                now.duration_since(armed_at) <= BROWSER_FOCUS_ESCAPE_INTERVAL
            });
        if armed && self.released {
            self.clear();
            return BrowserFocusEscapeDecision::Exit;
        }
        if armed {
            return BrowserFocusEscapeDecision::Consume;
        }
        self.surface_id = Some(surface_id.to_string());
        self.armed_at = Some(now);
        self.released = false;
        BrowserFocusEscapeDecision::Forward
    }

    fn release(&mut self, surface_id: &str) {
        if self.surface_id.as_deref() == Some(surface_id) && self.armed_at.is_some() {
            self.released = true;
        }
    }

    fn clear(&mut self) {
        self.surface_id = None;
        self.armed_at = None;
        self.released = false;
    }
}

#[derive(Clone)]
struct DiffSurfaceControls {
    root: gtk::Box,
    search_row: gtk::Box,
    search: gtk::SearchEntry,
    scroll: gtk::ScrolledWindow,
    document_key: Value,
}

type DiffSurfaceControlsCache = Rc<RefCell<HashMap<String, DiffSurfaceControls>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkTerminalSearchState {
    surface_id: String,
    query: String,
    total: Option<u64>,
    selected: Option<u64>,
}

#[derive(Clone)]
struct TerminalSearchControls {
    root: gtk::Box,
    entry: gtk::SearchEntry,
    count: gtk::Label,
}

type TerminalSearchControlsCache = Rc<RefCell<HashMap<String, TerminalSearchControls>>>;

#[derive(Clone)]
struct TerminalTextBoxControls {
    root: gtk::Box,
    text_view: gtk::TextView,
    attachments: gtk::Box,
    send: gtk::Button,
    syncing: Rc<Cell<bool>>,
    focus_generation: Rc<Cell<u64>>,
    file_picker_generation: Rc<Cell<u64>>,
}

type TerminalTextBoxControlsCache = Rc<RefCell<HashMap<String, TerminalTextBoxControls>>>;

#[derive(Debug, Default, PartialEq)]
struct DesktopNotificationDelta {
    deliver: Vec<Value>,
    withdraw: Vec<String>,
}

#[derive(Debug, Default)]
struct DesktopNotificationTracker {
    known: HashSet<String>,
    active: HashSet<String>,
}

impl DesktopNotificationTracker {
    fn from_snapshot(snapshot: &Value) -> Self {
        Self {
            known: notification_rows(snapshot)
                .filter_map(notification_id)
                .map(str::to_string)
                .collect(),
            active: HashSet::new(),
        }
    }

    fn update(&mut self, snapshot: &Value) -> DesktopNotificationDelta {
        let rows = notification_rows(snapshot).collect::<Vec<_>>();
        let unread_ids = rows
            .iter()
            .filter(|row| !notification_is_read(row))
            .filter_map(|row| notification_id(row))
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let mut withdraw = self
            .active
            .difference(&unread_ids)
            .cloned()
            .collect::<Vec<_>>();
        withdraw.sort();
        self.active.retain(|id| unread_ids.contains(id));

        let mut deliver = Vec::new();
        for row in rows {
            let Some(id) = notification_id(row) else {
                continue;
            };
            let is_new = self.known.insert(id.to_string());
            if is_new && !notification_is_read(row) {
                self.active.insert(id.to_string());
                deliver.push(row.clone());
            }
        }
        DesktopNotificationDelta { deliver, withdraw }
    }
}

fn notification_rows(snapshot: &Value) -> impl Iterator<Item = &Value> {
    snapshot
        .get("notifications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn notification_id(notification: &Value) -> Option<&str> {
    notification
        .get("id")
        .or_else(|| notification.get("notification_id"))
        .and_then(Value::as_str)
}

fn notification_is_read(notification: &Value) -> bool {
    notification
        .get("read")
        .or_else(|| notification.get("is_read"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn sync_desktop_notifications(
    application: &gtk::Application,
    snapshot: &Value,
    tracker: &mut DesktopNotificationTracker,
) {
    let delta = tracker.update(snapshot);
    for id in delta.withdraw {
        application.withdraw_notification(&id);
    }
    for row in delta.deliver {
        let Some(id) = notification_id(&row) else {
            continue;
        };
        let title = value_str(&row, "title", "cmux");
        let notification = gio::Notification::new(if title.is_empty() { "cmux" } else { title });
        let body = desktop_notification_body(&row);
        if !body.is_empty() {
            notification.set_body(Some(&body));
        }
        notification
            .set_default_action_and_target_value("app.open-notification", Some(&id.to_variant()));
        application.send_notification(Some(id), &notification);
    }
}

fn desktop_notification_body(notification: &Value) -> String {
    [
        value_str(notification, "subtitle", ""),
        value_str(notification, "body", ""),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" - ")
}

fn snapshot_or_error(
    app_state: &Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    window_id: &str,
) -> Value {
    snapshot_with_previews(app_state, renderer_mode, window_id).unwrap_or_else(|err| {
        json!({
            "renderer": {"backend": "gtk", "state": "error"},
            "window": {"window_id": window_id},
            "surface_views": [],
            "workspaces": [],
            "diagnostics": {"error": err.to_string()}
        })
    })
}

#[cfg(test)]
fn snapshot_rebuild_key(snapshot: &Value) -> Value {
    let keys = snapshot_region_rebuild_keys(snapshot);
    json!({
        "left": keys.left,
        "main": keys.main,
        "right": keys.right
    })
}

#[cfg(test)]
fn snapshot_rebuild_key_without_tabs(snapshot: &Value) -> Value {
    let keys = snapshot_region_rebuild_keys(snapshot);
    json!({
        "left": keys.left,
        "main": keys.main_without_tabs,
        "right": keys.right
    })
}

#[derive(Debug, Clone, PartialEq)]
struct GtkSnapshotRegionRebuildKeys {
    left: Value,
    main: Value,
    main_without_tabs: Value,
    right: Value,
}

#[cfg(test)]
fn snapshot_region_rebuild_keys(snapshot: &Value) -> GtkSnapshotRegionRebuildKeys {
    snapshot_region_rebuild_keys_for_mode(snapshot, GtkUiMode::Legacy)
}

fn snapshot_region_rebuild_keys_for_mode(
    snapshot: &Value,
    ui_mode: GtkUiMode,
) -> GtkSnapshotRegionRebuildKeys {
    let include_overlays = !ui_mode.is_next();
    GtkSnapshotRegionRebuildKeys {
        left: snapshot_left_rebuild_key(snapshot),
        main: snapshot_main_rebuild_key(snapshot, true, include_overlays),
        main_without_tabs: snapshot_main_rebuild_key(snapshot, false, include_overlays),
        right: snapshot_right_rebuild_key(snapshot),
    }
}

fn snapshot_left_rebuild_key(snapshot: &Value) -> Value {
    json!({
        "workspaces": snapshot.get("workspaces"),
        "workspace_groups": snapshot.get("workspace_groups"),
        "custom_sidebar": snapshot.get("custom_sidebar"),
        "config_reload_generation": snapshot.pointer("/config/reload_generation")
    })
}

fn snapshot_main_rebuild_key(
    snapshot: &Value,
    include_tabs: bool,
    include_overlays: bool,
) -> Value {
    let workspaces = snapshot
        .get("workspaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|workspace| {
            json!({
                "workspace_id": workspace.get("workspace_id"),
                "workspace_ref": workspace.get("workspace_ref"),
                "group_id": workspace.get("group_id"),
                "group_ref": workspace.get("group_ref"),
                "selected": workspace.get("selected"),
                "pinned": workspace.get("pinned")
            })
        })
        .collect::<Vec<_>>();
    let surfaces = snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|surface| {
            let text_box = surface.get("text_box").unwrap_or(&Value::Null);
            let agent_session = surface.get("agent_session").unwrap_or(&Value::Null);
            json!({
                "surface_id": surface.get("surface_id"),
                "surface_ref": surface.get("surface_ref"),
                "pane_id": surface.get("pane_id"),
                "pane_ref": surface.get("pane_ref"),
                "workspace_id": surface.get("workspace_id"),
                "kind": surface.get("kind"),
                "visible": surface.get("visible"),
                "frame": surface.get("frame"),
                "tabs": include_tabs.then(|| surface.get("tabs")).flatten(),
                "agent_hibernation": surface.get("agent_hibernation"),
                "hibernated": surface.get("hibernated"),
                "terminal_search_active": surface
                    .pointer("/terminal_search/active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "text_box": json!({
                    "active": text_box.get("active"),
                    "focus": text_box.get("focus"),
                    "attachments": text_box.get("attachments"),
                    "focus_generation": text_box.get("focus_generation"),
                    "file_picker_generation": text_box.get("file_picker_generation"),
                    "max_lines": text_box.get("max_lines")
                }),
                "browser": surface.get("browser"),
                "settings": surface.get("settings"),
                "document": surface.get("document"),
                "agent_session": if agent_session.is_null() {
                    Value::Null
                } else {
                    json!({
                        "provider_id": agent_session.get("provider_id"),
                        "renderer_kind": agent_session.get("renderer_kind"),
                        "status": agent_session.get("status"),
                        "last_error": agent_session.get("last_error"),
                        "permission_mode": agent_session.get("permission_mode"),
                        "pending_attachments": agent_session.get("pending_attachments")
                    })
                },
                "project": surface.get("project")
            })
        })
        .collect::<Vec<_>>();
    let canvas = snapshot_canvas_rebuild_key(snapshot, include_tabs, true);
    json!({
        "workspaces": workspaces,
        "surfaces": surfaces,
        "command_palette": include_overlays.then(|| snapshot.get("command_palette")).flatten(),
        "shortcut_help": include_overlays.then(|| snapshot.get("shortcut_help")).flatten(),
        "canvas": canvas,
        "app_config": snapshot.pointer("/config/app"),
        "config_reload_generation": snapshot.pointer("/config/reload_generation")
    })
}

fn snapshot_canvas_rebuild_key(
    snapshot: &Value,
    include_tab_lists: bool,
    include_selection: bool,
) -> Value {
    let mut canvas = snapshot.get("canvas").cloned().unwrap_or(Value::Null);
    for pane in canvas
        .get_mut("panes")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(pane) = pane.as_object_mut() else {
            continue;
        };
        if !include_tab_lists {
            pane.remove("surface_ids");
            pane.remove("surface_refs");
        }
        if !include_selection {
            for key in [
                "surface_id",
                "surface_ref",
                "selected_surface_id",
                "selected_surface_ref",
            ] {
                pane.remove(key);
            }
        }
    }
    canvas
}

fn snapshot_pane_rebuild_keys(snapshot: &Value) -> HashMap<String, Value> {
    let config = json!({
        "app": snapshot.pointer("/config/app"),
        "reload_generation": snapshot.pointer("/config/reload_generation")
    });
    snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|view| value_bool_or(view, "visible", true))
        .filter_map(|view| {
            Some((
                pane_id_or_ref(view)?,
                json!({"view": view, "config": config}),
            ))
        })
        .collect()
}

fn snapshot_main_structure_rebuild_key(snapshot: &Value) -> Value {
    let panes = snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|view| value_bool_or(view, "visible", true))
        .map(|view| {
            json!({
                "pane_id": view.get("pane_id"),
                "pane_ref": view.get("pane_ref"),
                "workspace_id": view.get("workspace_id"),
                "frame": view.get("frame")
            })
        })
        .collect::<Vec<_>>();
    let canvas = snapshot_canvas_rebuild_key(snapshot, false, false);
    json!({
        "panes": panes,
        "canvas": canvas,
        "config_reload_generation": snapshot.pointer("/config/reload_generation")
    })
}

fn snapshot_right_rebuild_key(snapshot: &Value) -> Value {
    let visible = right_sidebar_visible(snapshot);
    let mode = right_sidebar_mode(snapshot);
    let mut state = snapshot
        .get("right_sidebar")
        .cloned()
        .unwrap_or(Value::Null);
    let feed_items = state
        .as_object_mut()
        .and_then(|state| state.remove("feed_items"))
        .unwrap_or(Value::Null);
    let content = if !visible {
        Value::Null
    } else {
        match mode.as_str() {
            "files" | "find" => json!({
                "cwd": snapshot.pointer("/sidebar/cwd")
            }),
            "sessions" | "dock" => {
                let surfaces = snapshot
                    .get("surfaces")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|surface| {
                        json!({
                            "id": surface.get("id"),
                            "surface_id": surface.get("surface_id"),
                            "surface_ref": surface.get("surface_ref"),
                            "type": surface.get("type"),
                            "title": surface.get("title"),
                            "has_resume_binding": surface
                                .get("resume_binding")
                                .is_some_and(Value::is_object)
                        })
                    })
                    .collect::<Vec<_>>();
                json!({"surfaces": surfaces})
            }
            "feed" => json!({
                "progress": snapshot.pointer("/sidebar/progress"),
                "statuses": snapshot.pointer("/sidebar/statuses"),
                "logs": snapshot.pointer("/sidebar/logs"),
                "feed_items": feed_items,
                "notifications": snapshot.get("notifications")
            }),
            _ => Value::Null,
        }
    };
    json!({
        "state": state,
        "content": content,
        "config_reload_generation": snapshot.pointer("/config/reload_generation")
    })
}

fn snapshot_with_previews(
    app_state: &Arc<Mutex<AppState>>,
    renderer_mode: GtkRendererMode,
    window_id: &str,
) -> Result<Value> {
    let mut app = app_state
        .lock()
        .map_err(|_| anyhow!("app state lock poisoned"))?;
    let mut snapshot = renderer::snapshot_value(
        &mut app,
        &json!({
            "backend": gtk_snapshot_backend(renderer_mode),
            "window_id": window_id
        }),
    )
    .map_err(|err| anyhow!("{err}"))?;

    if renderer_mode == GtkRendererMode::Ghostty {
        return Ok(snapshot);
    }

    if let Some(views) = snapshot
        .get_mut("surface_views")
        .and_then(Value::as_array_mut)
    {
        for view in views {
            let Some(surface_id) = view.get("surface_id").and_then(Value::as_str) else {
                continue;
            };
            let preview = app
                .handle("surface.read_text", &json!({"surface_id": surface_id}))
                .ok()
                .and_then(|value| value.get("text").and_then(Value::as_str).map(trim_preview))
                .unwrap_or_default();
            if let Some(object) = view.as_object_mut() {
                object.insert("preview".to_string(), json!(preview));
            }
        }
    }

    Ok(snapshot)
}

fn gtk_snapshot_backend(renderer_mode: GtkRendererMode) -> &'static str {
    match renderer_mode {
        GtkRendererMode::Gtk => "ghostty-vt",
        GtkRendererMode::Ghostty => "ghostty",
    }
}

fn workspace_sidebar(
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
    ui_mode: GtkUiMode,
) -> gtk::Box {
    let width = if ui_mode.is_next() { 228 } else { 260 };
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sidebar.add_css_class("cmux-sidebar");
    sidebar.set_hexpand(true);
    sidebar.set_vexpand(true);
    let sidebar_settings = config::sidebar_settings();
    if sidebar_settings.match_terminal_background {
        install_custom_sidebar_style(sidebar.upcast_ref(), "background: transparent;");
    }

    let custom_sidebar = snapshot.get("custom_sidebar").unwrap_or(&Value::Null);
    sidebar.append(&custom_sidebar_header(custom_sidebar, app_state));
    if custom_sidebar
        .get("selected_provider_id")
        .and_then(Value::as_str)
        .is_some_and(|provider| {
            provider.starts_with("cmux.sidebar.custom.")
                || provider == crate::sidebar_extension::HOSTED_PROVIDER_ID
        })
    {
        append_custom_sidebar(&sidebar, custom_sidebar, app_state);
        return bounded_workspace_sidebar(sidebar, width);
    }

    let drag_state = Rc::new(RefCell::new(GtkWorkspaceDragState::default()));
    let color_settings = config::workspace_color_settings();
    for row in workspace_sidebar_rows(snapshot) {
        match row.kind {
            GtkWorkspaceSidebarRowKind::GroupHeader => {
                sidebar.append(&workspace_group_sidebar_row(
                    &row,
                    app_state,
                    &drag_state,
                    &color_settings,
                    &sidebar_settings,
                ));
            }
            GtkWorkspaceSidebarRowKind::Workspace => {
                sidebar.append(&workspace_sidebar_row(
                    &row,
                    app_state,
                    &drag_state,
                    &color_settings,
                    &sidebar_settings,
                ));
            }
        }
    }

    bounded_workspace_sidebar(sidebar, width)
}

fn bounded_workspace_sidebar(sidebar: gtk::Box, width: i32) -> gtk::Box {
    let viewport = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_width(width)
        .max_content_width(width)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .hexpand(true)
        .vexpand(true)
        .child(&sidebar)
        .build();
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.set_width_request(width);
    frame.set_hexpand(false);
    frame.set_vexpand(true);
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.append(&viewport);
    frame
}

fn custom_sidebar_header(custom_sidebar: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.set_hexpand(true);
    let title = custom_sidebar
        .get("selected_name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("Workspaces");
    let heading = label(title, "cmux-heading");
    configure_workspace_bounded_label(&heading);
    header.append(&heading);

    if !custom_sidebar
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return header;
    }

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("view-more-symbolic");
    menu_button.set_tooltip_text(Some("Choose Sidebar"));
    menu_button.add_css_class("cmux-icon-action");
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 4);
    choices.set_margin_top(6);
    choices.set_margin_bottom(6);
    choices.set_margin_start(6);
    choices.set_margin_end(6);

    let workspaces = gtk::Button::with_label("Workspaces");
    workspaces.add_css_class("cmux-sidebar-provider");
    workspaces.set_halign(gtk::Align::Fill);
    let default_app_state = Arc::clone(app_state);
    workspaces.connect_clicked(move |_| {
        call_app(
            &default_app_state,
            "sidebar.custom.select",
            json!({"provider_id": "cmux.sidebar.workspaces"}),
        );
    });
    choices.append(&workspaces);

    for provider in custom_sidebar
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let provider_id = value_str(provider, "id", "");
        if provider_id.is_empty() {
            continue;
        }
        let title = value_str(provider, "title", provider_id);
        let button = gtk::Button::with_label(title);
        button.add_css_class("cmux-sidebar-provider");
        button.set_halign(gtk::Align::Fill);
        if !provider.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            button.add_css_class("cmux-sidebar-provider-invalid");
            button.set_tooltip_text(
                provider
                    .get("error")
                    .and_then(Value::as_str)
                    .filter(|error| !error.is_empty()),
            );
        }
        let provider_id = provider_id.to_string();
        let provider_app_state = Arc::clone(app_state);
        button.connect_clicked(move |_| {
            call_app(
                &provider_app_state,
                "sidebar.custom.select",
                json!({"provider_id": provider_id}),
            );
        });
        choices.append(&button);
    }
    popover.set_child(Some(&choices));
    menu_button.set_popover(Some(&popover));
    header.append(&menu_button);
    header
}

fn append_custom_sidebar(
    sidebar: &gtk::Box,
    custom_sidebar: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    if custom_sidebar
        .get("selected_provider_id")
        .and_then(Value::as_str)
        == Some(crate::sidebar_extension::HOSTED_PROVIDER_ID)
    {
        append_sidebar_extension_controls(sidebar, custom_sidebar, app_state);
    }
    if let Some(error) = custom_sidebar
        .get("error")
        .and_then(Value::as_str)
        .filter(|error| !error.is_empty())
    {
        let banner = gtk::Box::new(gtk::Orientation::Vertical, 3);
        banner.add_css_class("cmux-custom-sidebar-error");
        banner.append(&label(
            if custom_sidebar
                .get("using_last_good")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "Sidebar error - showing last good version"
            } else {
                "Sidebar error"
            },
            "cmux-heading",
        ));
        let message = label(error, "cmux-muted");
        message.set_wrap(true);
        banner.append(&message);
        sidebar.append(&banner);
    }

    let Some(root) = custom_sidebar.pointer("/document/root") else {
        if custom_sidebar.get("error").is_none_or(Value::is_null) {
            sidebar.append(&label("Sidebar file is empty or missing.", "cmux-muted"));
        }
        return;
    };
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_hexpand(true);
    let drag_state = Rc::new(RefCell::new(CustomSidebarDragState::default()));
    let provider_id = value_str(custom_sidebar, "selected_provider_id", "").to_string();
    let content = custom_sidebar_widget(root, app_state, &drag_state, &provider_id);
    content.set_halign(gtk::Align::Fill);
    content.set_valign(gtk::Align::Start);
    scroller.set_child(Some(&content));
    sidebar.append(&scroller);
}

fn append_sidebar_extension_controls(
    sidebar: &gtk::Box,
    custom_sidebar: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let extensions = custom_sidebar
        .pointer("/extension_host/extensions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if extensions.is_empty() {
        sidebar.append(&label(
            "Install a sidebar extension under ~/.config/cmux/extensions.",
            "cmux-muted",
        ));
        return;
    }
    let selector = gtk::ComboBoxText::new();
    let mut selected_id = None;
    for extension in &extensions {
        if extension.get("ok").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let id = value_str(extension, "id", "");
        if id.is_empty() {
            continue;
        }
        selector.append(Some(id), value_str(extension, "displayName", id));
        if extension
            .get("selected")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            selected_id = Some(id.to_string());
        }
    }
    if let Some(selected_id) = selected_id.as_deref() {
        selector.set_active_id(Some(selected_id));
    } else {
        selector.set_active(Some(0));
    }
    let selector_state = Arc::clone(app_state);
    selector.connect_changed(move |selector| {
        let Some(id) = selector.active_id() else {
            return;
        };
        call_app(
            &selector_state,
            "sidebar.extension.select",
            json!({"id": id.as_str()}),
        );
    });
    sidebar.append(&selector);

    let selected = extensions
        .iter()
        .find(|extension| {
            extension
                .get("selected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .or_else(|| extensions.iter().find(|extension| extension["ok"] == true));
    let Some(selected) = selected else {
        return;
    };
    let extension_id = value_str(selected, "id", "").to_string();
    let approved = selected
        .get("approved")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let access = gtk::Button::with_label(if approved {
        "Revoke Access"
    } else {
        "Approve Requested Access"
    });
    access.add_css_class("cmux-sidebar-provider");
    let access_state = Arc::clone(app_state);
    access.connect_clicked(move |_| {
        call_app(
            &access_state,
            if approved {
                "sidebar.extension.revoke"
            } else {
                "sidebar.extension.grant"
            },
            json!({"id": extension_id}),
        );
    });
    sidebar.append(&access);
}

#[derive(Clone, Debug)]
struct CustomSidebarDragPayload {
    method: String,
    id_parameter: String,
    item_id: String,
}

#[derive(Clone)]
struct CustomSidebarDropHover {
    method: String,
    index: usize,
    widget: glib::WeakRef<gtk::Widget>,
}

#[derive(Default)]
struct CustomSidebarDragState {
    payload: Option<CustomSidebarDragPayload>,
    hover: Option<CustomSidebarDropHover>,
}

type CustomSidebarDragStateRef = Rc<RefCell<CustomSidebarDragState>>;

fn custom_sidebar_widget(
    node: &Value,
    app_state: &Arc<Mutex<AppState>>,
    drag_state: &CustomSidebarDragStateRef,
    provider_id: &str,
) -> gtk::Widget {
    custom_sidebar_widget_with_submit_events(node, app_state, drag_state, provider_id, &[])
}

fn custom_sidebar_widget_with_submit_events(
    node: &Value,
    app_state: &Arc<Mutex<AppState>>,
    drag_state: &CustomSidebarDragStateRef,
    provider_id: &str,
    inherited_submit_events: &[Value],
) -> gtk::Widget {
    let submit_events = custom_sidebar_submit_events(node, inherited_submit_events);
    let kind = value_str(node, "type", "text");
    let widget: gtk::Widget = match kind {
        "vstack" | "hstack" => {
            let vertical = kind == "vstack";
            let container = gtk::Box::new(
                if vertical {
                    gtk::Orientation::Vertical
                } else {
                    gtk::Orientation::Horizontal
                },
                custom_sidebar_spacing(node),
            );
            container.set_hexpand(vertical);
            container.set_vexpand(!vertical);
            apply_custom_sidebar_alignment(&container, node, vertical);
            for child in node
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                container.append(&custom_sidebar_widget_with_submit_events(
                    child,
                    app_state,
                    drag_state,
                    provider_id,
                    &submit_events,
                ));
            }
            container.upcast()
        }
        "zstack" => {
            let overlay = gtk::Overlay::new();
            let mut children = node
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            if let Some(first) = children.next() {
                overlay.set_child(Some(&custom_sidebar_widget_with_submit_events(
                    first,
                    app_state,
                    drag_state,
                    provider_id,
                    &submit_events,
                )));
            }
            for child in children {
                overlay.add_overlay(&custom_sidebar_widget_with_submit_events(
                    child,
                    app_state,
                    drag_state,
                    provider_id,
                    &submit_events,
                ));
            }
            overlay.upcast()
        }
        "button" => {
            let button = gtk::Button::new();
            let children = node.get("children").and_then(Value::as_array);
            if children.is_some_and(|children| !children.is_empty()) {
                let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                for child in children.into_iter().flatten() {
                    content.append(&custom_sidebar_widget_with_submit_events(
                        child,
                        app_state,
                        drag_state,
                        provider_id,
                        &submit_events,
                    ));
                }
                button.set_child(Some(&content));
            } else {
                button.set_label(value_str(node, "title", ""));
            }
            button.add_css_class("cmux-custom-sidebar-button");
            if let Some(action) = node.get("action").cloned() {
                let action_app_state = Arc::clone(app_state);
                let action_provider_id = provider_id.to_string();
                button.connect_clicked(move |_| {
                    dispatch_custom_sidebar_action(&action_app_state, &action_provider_id, &action);
                });
            } else {
                button.set_sensitive(false);
            }
            button.upcast()
        }
        "toggle" => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let toggle = gtk::CheckButton::new();
            let binding_key = node
                .pointer("/binding/key")
                .and_then(Value::as_str)
                .map(str::to_string);
            let active = node
                .pointer("/binding/value")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            toggle.set_active(active);
            if let Some(binding_key) = binding_key {
                let toggle_state = Arc::clone(app_state);
                let toggle_provider_id = provider_id.to_string();
                toggle.connect_toggled(move |toggle| {
                    call_app(
                        &toggle_state,
                        "sidebar.custom.state.set",
                        json!({
                            "provider_id": toggle_provider_id,
                            "key": binding_key,
                            "value": toggle.is_active()
                        }),
                    );
                });
            } else {
                toggle.set_sensitive(false);
            }
            row.append(&toggle);
            let children = node.get("children").and_then(Value::as_array);
            if children.is_some_and(|children| !children.is_empty()) {
                for child in children.into_iter().flatten() {
                    row.append(&custom_sidebar_widget_with_submit_events(
                        child,
                        app_state,
                        drag_state,
                        provider_id,
                        &submit_events,
                    ));
                }
            } else if let Some(title) = node
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
            {
                let title = gtk::Label::new(Some(title));
                title.set_xalign(0.0);
                title.set_hexpand(true);
                row.append(&title);
            }
            row.upcast()
        }
        "textfield" => {
            let entry = gtk::Entry::new();
            entry.add_css_class("cmux-custom-sidebar-input");
            if let Some(placeholder) = node
                .get("placeholder")
                .and_then(Value::as_str)
                .filter(|placeholder| !placeholder.is_empty())
            {
                entry.set_placeholder_text(Some(placeholder));
            }
            if let Some(value) = node.pointer("/binding/value").and_then(Value::as_str) {
                entry.set_text(value);
            }
            if let Some(binding_key) = node
                .pointer("/binding/key")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let entry_state = Arc::clone(app_state);
                let entry_provider_id = provider_id.to_string();
                let changed_binding_key = binding_key.clone();
                entry.connect_changed(move |entry| {
                    call_app(
                        &entry_state,
                        "sidebar.custom.state.set",
                        json!({
                            "provider_id": entry_provider_id,
                            "key": changed_binding_key,
                            "value": entry.text().as_str()
                        }),
                    );
                });
                if !submit_events.is_empty() {
                    let submit_state = Arc::clone(app_state);
                    let submit_provider_id = provider_id.to_string();
                    entry.connect_activate(move |entry| {
                        for event in &submit_events {
                            let Some(event_id) = event.get("id").and_then(Value::as_str) else {
                                continue;
                            };
                            call_app(
                                &submit_state,
                                "sidebar.custom.event.submit",
                                custom_sidebar_submit_params(
                                    &submit_provider_id,
                                    event_id,
                                    &binding_key,
                                    entry.text().as_str(),
                                ),
                            );
                        }
                    });
                }
            } else {
                entry.set_sensitive(false);
            }
            entry.upcast()
        }
        "slider" => {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
            for child in node
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                container.append(&custom_sidebar_widget_with_submit_events(
                    child,
                    app_state,
                    drag_state,
                    provider_id,
                    &submit_events,
                ));
            }
            let minimum = custom_sidebar_number(node, "minimum").unwrap_or(0.0);
            let maximum = custom_sidebar_number(node, "maximum").unwrap_or(1.0);
            let step = custom_sidebar_number(node, "step")
                .filter(|step| *step > 0.0)
                .unwrap_or_else(|| ((maximum - minimum) / 100.0).max(0.000_001));
            let slider =
                gtk::Scale::with_range(gtk::Orientation::Horizontal, minimum, maximum, step);
            slider.add_css_class("cmux-custom-sidebar-input");
            slider.set_hexpand(true);
            slider.set_draw_value(true);
            slider.set_digits(custom_sidebar_control_digits(step) as i32);
            let integer_binding = node
                .pointer("/binding/value")
                .and_then(Value::as_i64)
                .is_some();
            if let Some(value) = node
                .pointer("/binding/value")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
            {
                slider.set_value(value.clamp(minimum, maximum));
            }
            if let Some(binding_key) = node
                .pointer("/binding/key")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let slider_state = Arc::clone(app_state);
                let slider_provider_id = provider_id.to_string();
                slider.connect_value_changed(move |slider| {
                    call_app(
                        &slider_state,
                        "sidebar.custom.state.set",
                        json!({
                            "provider_id": slider_provider_id,
                            "key": binding_key,
                            "value": custom_sidebar_numeric_state_value(
                                slider.value(),
                                integer_binding
                            )
                        }),
                    );
                });
            } else {
                slider.set_sensitive(false);
            }
            container.append(&slider);
            container.upcast()
        }
        "picker" => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            if let Some(title) = node
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
            {
                let label = gtk::Label::new(Some(title));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                row.append(&label);
            }
            let picker = gtk::ComboBoxText::new();
            picker.add_css_class("cmux-custom-sidebar-input");
            let options = node
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let option_values = options
                .iter()
                .filter_map(|option| option.get("value").cloned())
                .collect::<Vec<_>>();
            for (index, option) in options.iter().enumerate() {
                let id = index.to_string();
                picker.append(
                    Some(&id),
                    option
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
            }
            if let Some(selected) = node.pointer("/binding/value") {
                if let Some(index) = option_values
                    .iter()
                    .position(|candidate| candidate == selected)
                {
                    picker.set_active(Some(index as u32));
                }
            }
            if let Some(binding_key) = node
                .pointer("/binding/key")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let picker_state = Arc::clone(app_state);
                let picker_provider_id = provider_id.to_string();
                picker.connect_changed(move |picker| {
                    let Some(index) = picker.active().map(|index| index as usize) else {
                        return;
                    };
                    let Some(value) = option_values.get(index) else {
                        return;
                    };
                    call_app(
                        &picker_state,
                        "sidebar.custom.state.set",
                        json!({
                            "provider_id": picker_provider_id,
                            "key": binding_key,
                            "value": value
                        }),
                    );
                });
            } else {
                picker.set_sensitive(false);
            }
            row.append(&picker);
            row.upcast()
        }
        "stepper" => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let children = node.get("children").and_then(Value::as_array);
            if children.is_some_and(|children| !children.is_empty()) {
                for child in children.into_iter().flatten() {
                    row.append(&custom_sidebar_widget_with_submit_events(
                        child,
                        app_state,
                        drag_state,
                        provider_id,
                        &submit_events,
                    ));
                }
            } else if let Some(title) = node
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
            {
                let label = gtk::Label::new(Some(title));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                row.append(&label);
            }
            let minimum = custom_sidebar_number(node, "minimum").unwrap_or(-1_000_000_000.0);
            let maximum = custom_sidebar_number(node, "maximum").unwrap_or(1_000_000_000.0);
            let step = custom_sidebar_number(node, "step")
                .filter(|step| *step > 0.0)
                .unwrap_or(1.0);
            let stepper = gtk::SpinButton::with_range(minimum, maximum, step);
            stepper.add_css_class("cmux-custom-sidebar-input");
            stepper.set_digits(custom_sidebar_control_digits(step));
            let integer_binding = node
                .pointer("/binding/value")
                .and_then(Value::as_i64)
                .is_some();
            if let Some(value) = node
                .pointer("/binding/value")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
            {
                stepper.set_value(value.clamp(minimum, maximum));
            }
            if let Some(binding_key) = node
                .pointer("/binding/key")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let stepper_state = Arc::clone(app_state);
                let stepper_provider_id = provider_id.to_string();
                stepper.connect_value_changed(move |stepper| {
                    call_app(
                        &stepper_state,
                        "sidebar.custom.state.set",
                        json!({
                            "provider_id": stepper_provider_id,
                            "key": binding_key,
                            "value": custom_sidebar_numeric_state_value(
                                stepper.value(),
                                integer_binding
                            )
                        }),
                    );
                });
            } else {
                stepper.set_sensitive(false);
            }
            row.append(&stepper);
            row.upcast()
        }
        "image" => {
            let image = gtk::Image::from_icon_name(custom_sidebar_icon_name(value_str(
                node,
                "systemName",
                "",
            )));
            image.upcast()
        }
        "progress" => {
            let progress = gtk::ProgressBar::new();
            if let Some(value) = custom_sidebar_number(node, "value") {
                progress.set_fraction(value.clamp(0.0, 1.0));
            } else {
                progress.pulse();
            }
            progress.upcast()
        }
        "shape" => {
            let shape = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            shape.add_css_class("cmux-custom-sidebar-shape");
            shape.upcast()
        }
        "spacer" => {
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.set_hexpand(true);
            spacer.set_vexpand(true);
            if let Some(size) = custom_sidebar_number(node, "size") {
                let size = size.round().clamp(0.0, 1024.0) as i32;
                spacer.set_size_request(size, size);
            }
            spacer.upcast()
        }
        "divider" => gtk::Separator::new(gtk::Orientation::Horizontal).upcast(),
        _ => {
            let text = gtk::Label::new(Some(value_str(node, "text", "")));
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_selectable(true);
            text.upcast()
        }
    };
    apply_custom_sidebar_style(&widget, node);
    attach_custom_sidebar_reorder(&widget, node, drag_state, app_state);
    widget
}

fn custom_sidebar_reorder_payload(node: &Value) -> Option<CustomSidebarDragPayload> {
    let reorder = node.get("reorder")?;
    let method = value_str(reorder, "method", "").trim();
    let id_parameter = value_str(reorder, "idParameter", "").trim();
    let item_id = value_str(reorder, "itemId", "").trim();
    if method.is_empty() || id_parameter.is_empty() || item_id.is_empty() {
        return None;
    }
    Some(CustomSidebarDragPayload {
        method: method.to_string(),
        id_parameter: id_parameter.to_string(),
        item_id: item_id.to_string(),
    })
}

fn custom_sidebar_reorder_request(
    payload: &CustomSidebarDragPayload,
    target_method: &str,
    target_index: usize,
) -> Option<(String, Value)> {
    if payload.method != target_method {
        return None;
    }
    let mut params = serde_json::Map::new();
    params.insert(payload.id_parameter.clone(), json!(payload.item_id));
    params.insert("index".to_string(), json!(target_index));
    Some((payload.method.clone(), Value::Object(params)))
}

fn attach_custom_sidebar_reorder(
    widget: &gtk::Widget,
    node: &Value,
    drag_state: &CustomSidebarDragStateRef,
    app_state: &Arc<Mutex<AppState>>,
) {
    let Some(payload) = custom_sidebar_reorder_payload(node) else {
        return;
    };
    let index = node
        .pointer("/reorder/index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .unwrap_or(0);
    widget.add_css_class("cmux-custom-sidebar-reorderable");

    let gesture = gtk::GestureDrag::new();
    gesture.set_button(gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let begin_state = Rc::clone(drag_state);
    let begin_widget = widget.downgrade();
    gesture.connect_drag_begin(move |_, _, _| {
        let Some(begin_widget) = begin_widget.upgrade() else {
            return;
        };
        let mut state = begin_state.borrow_mut();
        state.payload = Some(payload.clone());
        state.hover = None;
        begin_widget.add_css_class("cmux-drag-source");
    });
    let end_state = Rc::clone(drag_state);
    let end_widget = widget.downgrade();
    let app_state = Arc::clone(app_state);
    gesture.connect_drag_end(move |_, _, _| {
        if let Some(end_widget) = end_widget.upgrade() {
            end_widget.remove_css_class("cmux-drag-source");
        }
        let (payload, hover) = {
            let mut state = end_state.borrow_mut();
            (state.payload.take(), state.hover.take())
        };
        let (Some(payload), Some(hover)) = (payload, hover) else {
            return;
        };
        if let Some(widget) = hover.widget.upgrade() {
            widget.remove_css_class("cmux-drop-target");
        }
        if let Some((method, params)) =
            custom_sidebar_reorder_request(&payload, &hover.method, hover.index)
        {
            call_app(&app_state, &method, params);
        }
    });
    widget.add_controller(gesture);

    let motion = gtk::EventControllerMotion::new();
    let enter_state = Rc::clone(drag_state);
    let enter_widget = widget.downgrade();
    let enter_method = node
        .pointer("/reorder/method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    motion.connect_enter(move |_, _, _| {
        if enter_state.borrow().payload.is_none() {
            return;
        }
        let Some(enter_widget) = enter_widget.upgrade() else {
            return;
        };
        enter_widget.add_css_class("cmux-drop-target");
        enter_state.borrow_mut().hover = Some(CustomSidebarDropHover {
            method: enter_method.clone(),
            index,
            widget: enter_widget.downgrade(),
        });
    });
    let leave_state = Rc::clone(drag_state);
    let leave_widget = widget.downgrade();
    motion.connect_leave(move |_| {
        let Some(leave_widget) = leave_widget.upgrade() else {
            return;
        };
        leave_widget.remove_css_class("cmux-drop-target");
        let mut state = leave_state.borrow_mut();
        if state
            .hover
            .as_ref()
            .and_then(|hover| hover.widget.upgrade())
            .is_some_and(|widget| widget == leave_widget)
        {
            state.hover = None;
        }
    });
    widget.add_controller(motion);
}

fn custom_sidebar_spacing(node: &Value) -> i32 {
    custom_sidebar_number(node, "spacing")
        .unwrap_or(0.0)
        .round()
        .clamp(0.0, 256.0) as i32
}

fn custom_sidebar_number(node: &Value, key: &str) -> Option<f64> {
    node.get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn apply_custom_sidebar_alignment(container: &gtk::Box, node: &Value, vertical: bool) {
    match value_str(node, "alignment", "")
        .to_ascii_lowercase()
        .as_str()
    {
        "leading" if vertical => {
            container.set_halign(gtk::Align::Start);
        }
        "trailing" if vertical => {
            container.set_halign(gtk::Align::End);
        }
        "top" if !vertical => {
            container.set_valign(gtk::Align::Start);
        }
        "bottom" if !vertical => {
            container.set_valign(gtk::Align::End);
        }
        _ => {}
    }
}

fn apply_custom_sidebar_style(widget: &gtk::Widget, node: &Value) {
    if let Some(padding) = custom_sidebar_number(node, "padding") {
        let padding = padding.round().clamp(0.0, 256.0) as i32;
        widget.set_margin_top(padding);
        widget.set_margin_bottom(padding);
        widget.set_margin_start(padding);
        widget.set_margin_end(padding);
    }
    let color = node
        .get("color")
        .and_then(Value::as_str)
        .and_then(custom_sidebar_css_color);
    let background = node
        .get("background")
        .and_then(Value::as_str)
        .and_then(custom_sidebar_css_color);
    let size = custom_sidebar_font_size(node);
    let weight = node
        .get("weight")
        .and_then(Value::as_str)
        .and_then(custom_sidebar_css_weight);
    let corner_radius = custom_sidebar_number(node, "cornerRadius");
    if let Some(opacity) = custom_sidebar_number(node, "opacity") {
        widget.set_opacity(opacity.clamp(0.0, 1.0));
    }
    let width = custom_sidebar_number(node, "width")
        .map(|value| value.round().clamp(0.0, 4096.0) as i32)
        .unwrap_or(-1);
    let height = custom_sidebar_number(node, "height")
        .map(|value| value.round().clamp(0.0, 4096.0) as i32)
        .unwrap_or(-1);
    if width >= 0 || height >= 0 {
        widget.set_size_request(width, height);
    }
    if color.is_none()
        && background.is_none()
        && size.is_none()
        && weight.is_none()
        && corner_radius.is_none()
    {
        return;
    }
    let mut declarations = String::new();
    if let Some(color) = color {
        declarations.push_str(&format!("color: {color};"));
    }
    if let Some(background) = background {
        declarations.push_str(&format!("background-color: {background};"));
    }
    if let Some(size) = size {
        declarations.push_str(&format!("font-size: {size}px;"));
    }
    if let Some(weight) = weight {
        declarations.push_str(&format!("font-weight: {weight};"));
    }
    if let Some(radius) = corner_radius {
        declarations.push_str(&format!("border-radius: {}px;", radius.clamp(0.0, 256.0)));
    }
    install_custom_sidebar_style(widget, &declarations);
}

fn install_custom_sidebar_style(widget: &gtk::Widget, declarations: &str) {
    static INSTALLED: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    declarations.hash(&mut hasher);
    let hash = hasher.finish();
    let class = format!("cmux-custom-style-{hash:x}");
    widget.add_css_class(&class);
    let installed = INSTALLED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut installed) = installed.lock() else {
        return;
    };
    if !installed.insert(hash) {
        return;
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&format!(".{class} {{ {declarations} }}"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
}

fn custom_sidebar_font_size(node: &Value) -> Option<f64> {
    custom_sidebar_number(node, "size").or_else(|| {
        match value_str(node, "font", "").to_ascii_lowercase().as_str() {
            "largetitle" => Some(34.0),
            "title" => Some(28.0),
            "title2" => Some(22.0),
            "title3" => Some(20.0),
            "headline" => Some(17.0),
            "subheadline" => Some(15.0),
            "body" => Some(17.0),
            "callout" => Some(16.0),
            "footnote" => Some(13.0),
            "caption" => Some(12.0),
            "caption2" => Some(11.0),
            _ => None,
        }
    })
}

fn custom_sidebar_css_weight(token: &str) -> Option<&'static str> {
    match token.trim().to_ascii_lowercase().as_str() {
        "ultralight" | "thin" => Some("100"),
        "light" => Some("300"),
        "regular" => Some("400"),
        "medium" => Some("500"),
        "semibold" => Some("600"),
        "bold" => Some("700"),
        "heavy" => Some("800"),
        "black" => Some("900"),
        _ => None,
    }
}

fn custom_sidebar_css_color(token: &str) -> Option<&str> {
    let token = token.trim();
    if token.starts_with('#')
        && matches!(token.len(), 7 | 9)
        && token[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Some(token);
    }
    match token.to_ascii_lowercase().as_str() {
        "accent" | "accentcolor" => Some("#4aa3c7"),
        "primary" => Some("#f4f0e8"),
        "secondary" => Some("#a7b0b8"),
        "tertiary" => Some("#7d858c"),
        "quaternary" => Some("#60676d"),
        "red" => Some("#ff453a"),
        "orange" => Some("#ff9f0a"),
        "yellow" => Some("#ffd60a"),
        "green" => Some("#30d158"),
        "mint" => Some("#63e6be"),
        "teal" => Some("#40c8e0"),
        "cyan" => Some("#64d2ff"),
        "blue" => Some("#0a84ff"),
        "indigo" => Some("#5e5ce6"),
        "purple" => Some("#bf5af2"),
        "pink" => Some("#ff375f"),
        "brown" => Some("#ac8e68"),
        "gray" | "grey" => Some("#8e8e93"),
        "white" => Some("#ffffff"),
        "black" => Some("#000000"),
        "clear" => Some("transparent"),
        _ => None,
    }
}

fn custom_sidebar_icon_name(system_name: &str) -> &'static str {
    match system_name {
        "folder" | "folder.fill" => "folder-symbolic",
        "terminal" | "terminal.fill" => "utilities-terminal-symbolic",
        "globe" | "safari" => "web-browser-symbolic",
        "magnifyingglass" => "edit-find-symbolic",
        "gear" | "gearshape" | "gearshape.fill" => "emblem-system-symbolic",
        "star" | "star.fill" => "starred-symbolic",
        "bolt" | "bolt.fill" => "weather-storm-symbolic",
        "checkmark" | "checkmark.circle" | "checkmark.circle.fill" => "emblem-ok-symbolic",
        "exclamationmark.triangle" | "exclamationmark.triangle.fill" => "dialog-warning-symbolic",
        _ => "image-missing-symbolic",
    }
}

fn custom_sidebar_control_digits(step: f64) -> u32 {
    if !step.is_finite() || step <= 0.0 || step.fract() == 0.0 {
        return 0;
    }
    let mut scaled = step.abs();
    for digits in 1..=6 {
        scaled *= 10.0;
        if (scaled - scaled.round()).abs() < 0.000_001 {
            return digits;
        }
    }
    6
}

fn custom_sidebar_submit_events(node: &Value, inherited: &[Value]) -> Vec<Value> {
    let mut events = inherited.to_vec();
    events.extend(
        node.get("onSubmit")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned(),
    );
    events
}

fn custom_sidebar_submit_params(
    provider_id: &str,
    event_id: &str,
    binding_key: &str,
    value: &str,
) -> Value {
    json!({
        "provider_id": provider_id,
        "event_id": event_id,
        "key": binding_key,
        "value": value
    })
}

fn custom_sidebar_numeric_state_value(value: f64, integer: bool) -> Value {
    if integer {
        json!(value.round() as i64)
    } else {
        json!(value)
    }
}

fn dispatch_custom_sidebar_action(
    app_state: &Arc<Mutex<AppState>>,
    provider_id: &str,
    action: &Value,
) {
    if let Some(commands) = action.get("commands").and_then(Value::as_array) {
        for command in commands {
            dispatch_custom_sidebar_command(app_state, provider_id, command);
        }
        return;
    }
    dispatch_custom_sidebar_command(app_state, provider_id, action);
}

fn dispatch_custom_sidebar_command(
    app_state: &Arc<Mutex<AppState>>,
    provider_id: &str,
    action: &Value,
) {
    let kind = value_str(action, "type", "");
    if kind.is_empty() {
        return;
    }
    if kind == "cmux" {
        let method = value_str(action, "method", "");
        if !method.is_empty() {
            call_app(
                app_state,
                method,
                action.get("params").cloned().unwrap_or_else(|| json!({})),
            );
        }
        return;
    }
    if kind == "extension" {
        call_app(
            app_state,
            "sidebar.extension.action",
            json!({"action": action}),
        );
        return;
    }
    if matches!(kind, "state" | "log" | "openURL" | "open") {
        call_app(
            app_state,
            "sidebar.custom.action",
            json!({"provider_id": provider_id, "action": action}),
        );
    } else {
        call_app(
            app_state,
            kind,
            action.get("params").cloned().unwrap_or_else(|| json!({})),
        );
    }
}

fn workspace_sidebar_row(
    row_model: &GtkWorkspaceSidebarRow,
    app_state: &Arc<Mutex<AppState>>,
    drag_state: &GtkWorkspaceDragStateRef,
    color_settings: &config::WorkspaceColorSettings,
    sidebar_settings: &config::SidebarSettings,
) -> gtk::Box {
    let row_container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row_container.add_css_class("cmux-workspace-row");
    if row_model.indented {
        row_container.add_css_class("cmux-workspace-member");
    }
    row_container.set_hexpand(true);

    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("cmux-workspace");
    if row_model.selected {
        row.add_css_class("cmux-workspace-selected");
    }
    if row_model.multi_selected {
        row.add_css_class("cmux-workspace-multi-selected");
    }
    apply_workspace_color_style(&row, row_model, color_settings);
    row.set_hexpand(true);
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let title = label(&row_model.title, "cmux-heading");
    configure_workspace_title_label(&title, sidebar_settings.wrap_workspace_titles);
    title.set_hexpand(true);
    title_row.append(&title);
    if row_model.unread {
        let unread = label("●", "cmux-workspace-unread");
        unread.set_tooltip_text(Some("Unread notification"));
        if let Some(color) = color_settings.notification_badge_color.as_deref() {
            install_custom_sidebar_style(unread.upcast_ref(), &format!("color: {color};"));
        }
        title_row.append(&unread);
    }
    row.append(&title_row);
    append_workspace_sidebar_details(&row, row_model, sidebar_settings, app_state);
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("cmux-workspace-select");
    button.set_focusable(false);
    button.set_hexpand(true);
    let click_modifiers = Rc::new(Cell::new(gdk::ModifierType::empty()));
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pressed_modifiers = Rc::clone(&click_modifiers);
    gesture.connect_pressed(move |gesture, _, _, _| {
        pressed_modifiers.set(gesture.current_event_state());
    });
    button.add_controller(gesture);
    let target = row_model.target.clone();
    let click_app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(
            &click_app_state,
            "workspace.sidebar_select",
            workspace_sidebar_select_params(
                &target,
                click_modifiers.replace(gdk::ModifierType::empty()),
            ),
        );
    });
    attach_workspace_context_menu_for(&button, app_state, row_model);
    attach_workspace_drag_source(&button, row_model, drag_state, app_state);
    attach_workspace_drop_target(&button, row_model, drag_state);
    row_container.append(&button);

    if row_model.close_visible {
        let close = gtk::Button::with_label("x");
        close.add_css_class("cmux-workspace-close");
        close.set_focusable(false);
        let target = row_model.target.clone();
        let app_state = Arc::clone(app_state);
        close.connect_clicked(move |_| {
            call_app(
                &app_state,
                "workspace.close",
                json!({"workspace_id": target}),
            );
        });
        row_container.append(&close);
    }

    row_container
}

fn workspace_group_sidebar_row(
    row_model: &GtkWorkspaceSidebarRow,
    app_state: &Arc<Mutex<AppState>>,
    drag_state: &GtkWorkspaceDragStateRef,
    color_settings: &config::WorkspaceColorSettings,
    sidebar_settings: &config::SidebarSettings,
) -> gtk::Box {
    let row_container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row_container.add_css_class("cmux-workspace-row");
    row_container.set_hexpand(true);

    let toggle = gtk::Button::with_label(if row_model.collapsed { ">" } else { "v" });
    toggle.add_css_class("cmux-group-toggle");
    toggle.set_focusable(false);
    let group_target = row_model.target.clone();
    let method = if row_model.collapsed {
        "workspace.group.expand"
    } else {
        "workspace.group.collapse"
    };
    let toggle_app_state = Arc::clone(app_state);
    toggle.connect_clicked(move |_| {
        call_app(&toggle_app_state, method, json!({"group_id": group_target}));
    });
    row_container.append(&toggle);

    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("cmux-workspace");
    row.add_css_class("cmux-workspace-group");
    if row_model.selected {
        row.add_css_class("cmux-workspace-selected");
    }
    apply_workspace_color_style(&row, row_model, color_settings);
    row.set_hexpand(true);
    let title = label("", "cmux-heading");
    title.set_markup(&workspace_group_title_markup(row_model));
    configure_workspace_title_label(&title, sidebar_settings.wrap_workspace_titles);
    row.append(&title);
    if !sidebar_settings.hide_all_details {
        row.append(&workspace_detail_label(
            &row_model.subtitle,
            "cmux-workspace-detail",
        ));
    }

    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("cmux-workspace-select");
    button.set_focusable(false);
    button.set_hexpand(true);
    let group_target = row_model.target.clone();
    let focus_app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(
            &focus_app_state,
            "workspace.group.focus",
            json!({"group_id": group_target}),
        );
    });
    attach_workspace_group_context_menu_for(&button, app_state, row_model);
    attach_workspace_drag_source(&button, row_model, drag_state, app_state);
    attach_workspace_drop_target(&button, row_model, drag_state);
    row_container.append(&button);

    let add = gtk::Button::with_label("+");
    add.add_css_class("cmux-group-add");
    add.set_focusable(false);
    add.set_tooltip_text(Some("New Workspace in Group"));
    let group_target = row_model.target.clone();
    let add_app_state = Arc::clone(app_state);
    add.connect_clicked(move |_| {
        call_app(
            &add_app_state,
            "workspace.group.new_workspace",
            json!({"group_id": group_target}),
        );
    });
    attach_workspace_group_add_context_menu_for(&add, app_state, row_model);
    row_container.append(&add);

    row_container
}

fn configure_workspace_bounded_label(label: &gtk::Label) {
    label.set_hexpand(true);
    label.set_width_chars(1);
    label.set_max_width_chars(1);
}

fn configure_workspace_title_label(title: &gtk::Label, wrap: bool) {
    configure_workspace_bounded_label(title);
    title.set_wrap(wrap);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.set_single_line_mode(!wrap);
    title.set_ellipsize(if wrap {
        gtk::pango::EllipsizeMode::None
    } else {
        gtk::pango::EllipsizeMode::End
    });
}

fn workspace_sidebar_details(
    row: &GtkWorkspaceSidebarRow,
    settings: &config::SidebarSettings,
) -> GtkWorkspaceSidebarDetails {
    if settings.hide_all_details {
        return GtkWorkspaceSidebarDetails::default();
    }
    let description = settings
        .show_workspace_description
        .then(|| row.description.as_deref().and_then(sidebar_compact_text))
        .flatten();
    let mut branch_directory = Vec::new();
    if settings.show_branch_directory {
        let branch = settings
            .watch_git_status
            .then(|| row.git_branch.as_deref())
            .flatten()
            .and_then(sidebar_compact_text)
            .map(|branch| {
                if row.git_dirty {
                    format!("{branch} *")
                } else {
                    branch
                }
            });
        let directory = row
            .cwd
            .as_deref()
            .and_then(|path| sidebar_display_path(path, settings.path_last_segment_only));
        if !settings.stack_branch_directory
            && settings.branch_layout == config::SidebarBranchLayout::Inline
        {
            match (branch, directory) {
                (Some(branch), Some(directory)) => {
                    branch_directory.push(format!("{branch} · {directory}"));
                }
                (Some(branch), None) => branch_directory.push(branch),
                (None, Some(directory)) => branch_directory.push(directory),
                (None, None) => {}
            }
        } else {
            branch_directory.extend(branch);
            branch_directory.extend(directory);
        }
    }
    let notification = settings
        .show_notification_message
        .then(|| {
            row.latest_notification_text
                .as_deref()
                .and_then(sidebar_compact_text)
        })
        .flatten();
    let ssh_target = settings
        .show_ssh
        .then(|| row.ssh_target.as_deref().and_then(sidebar_compact_text))
        .flatten();
    let ports = if settings.show_ports {
        row.listening_ports.iter().copied().take(12).collect()
    } else {
        Vec::new()
    };
    let pull_requests = if settings.show_pull_requests {
        row.pull_request_urls.iter().take(4).cloned().collect()
    } else {
        Vec::new()
    };
    let metadata = if settings.show_custom_metadata {
        row.status_entries.iter().take(6).cloned().collect()
    } else {
        Vec::new()
    };
    let metadata_blocks = if settings.show_custom_metadata {
        row.metadata_blocks
            .iter()
            .filter_map(|value| sidebar_compact_text(value))
            .take(3)
            .collect()
    } else {
        Vec::new()
    };
    let progress = settings
        .show_progress
        .then(|| row.progress.clone())
        .flatten();
    let log = settings
        .show_log
        .then(|| row.latest_log.as_deref().and_then(sidebar_compact_text))
        .flatten();
    GtkWorkspaceSidebarDetails {
        description,
        branch_directory,
        notification,
        ssh_target,
        ports,
        pull_requests,
        metadata,
        metadata_blocks,
        progress,
        log,
    }
}

fn append_workspace_sidebar_details(
    row: &gtk::Box,
    row_model: &GtkWorkspaceSidebarRow,
    settings: &config::SidebarSettings,
    app_state: &Arc<Mutex<AppState>>,
) {
    let details = workspace_sidebar_details(row_model, settings);
    if let Some(description) = details.description {
        row.append(&workspace_detail_label(
            &description,
            "cmux-workspace-description",
        ));
    }
    for detail in details.branch_directory {
        row.append(&workspace_detail_label(&detail, "cmux-workspace-detail"));
    }
    if let Some(notification) = details.notification {
        row.append(&workspace_detail_label(
            &notification,
            "cmux-workspace-detail",
        ));
    }
    if let Some(target) = details.ssh_target {
        row.append(&workspace_detail_label(
            &format!("SSH {target}"),
            "cmux-workspace-detail",
        ));
    }
    if !details.ports.is_empty() {
        let ports = gtk::FlowBox::new();
        ports.set_selection_mode(gtk::SelectionMode::None);
        ports.set_max_children_per_line(4);
        ports.set_column_spacing(4);
        ports.set_row_spacing(4);
        for port in details.ports {
            let url = format!("http://localhost:{port}");
            let link = workspace_sidebar_link_label(
                &format!(":{port}"),
                &url,
                settings.open_port_links_in_cmux_browser,
                &row_model.target,
                app_state,
            );
            ports.insert(&link, -1);
        }
        row.append(&ports);
    }
    for url in details.pull_requests {
        if settings.make_pull_requests_clickable {
            let link = workspace_sidebar_link_label(
                &sidebar_link_label(&url),
                &url,
                settings.open_pull_request_links_in_cmux_browser,
                &row_model.target,
                app_state,
            );
            row.append(&link);
        } else {
            row.append(&workspace_detail_label(
                &sidebar_link_label(&url),
                "cmux-workspace-detail",
            ));
        }
    }
    if let Some(progress) = details.progress {
        let bar = gtk::ProgressBar::new();
        bar.add_css_class("cmux-workspace-progress");
        bar.set_fraction(progress.value.clamp(0.0, 1.0));
        if let Some(label) = progress.label.as_deref().and_then(sidebar_compact_text) {
            bar.set_show_text(true);
            bar.set_text(Some(&label));
        }
        row.append(&bar);
    }
    if !details.metadata.is_empty() {
        let metadata = gtk::FlowBox::new();
        metadata.set_selection_mode(gtk::SelectionMode::None);
        metadata.set_max_children_per_line(3);
        metadata.set_column_spacing(4);
        metadata.set_row_spacing(4);
        for entry in details.metadata {
            let pill = label(&entry.value, "cmux-workspace-metadata");
            configure_workspace_bounded_label(&pill);
            pill.set_ellipsize(gtk::pango::EllipsizeMode::End);
            if let Some(color) = entry
                .color
                .as_deref()
                .filter(|color| valid_hex_color(color))
            {
                install_custom_sidebar_style(
                    pill.upcast_ref(),
                    &format!("color: {color}; border: 1px solid {color};"),
                );
            }
            if let Some(url) = entry.url.as_deref() {
                pill.set_tooltip_text(Some(url));
            }
            metadata.insert(&pill, -1);
        }
        row.append(&metadata);
    }
    for block in details.metadata_blocks {
        row.append(&workspace_detail_label(&block, "cmux-workspace-detail"));
    }
    if let Some(log) = details.log {
        row.append(&workspace_detail_label(
            &format!("Log: {log}"),
            "cmux-workspace-detail",
        ));
    }
}

fn workspace_detail_label(text: &str, css_class: &str) -> gtk::Label {
    let detail = label(text, css_class);
    configure_workspace_bounded_label(&detail);
    detail.set_wrap(true);
    detail.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    detail.set_lines(2);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail
}

fn workspace_sidebar_link_label(
    text: &str,
    url: &str,
    embedded: bool,
    workspace_target: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Label {
    let link = label(text, "cmux-workspace-metadata");
    configure_workspace_bounded_label(&link);
    link.set_ellipsize(gtk::pango::EllipsizeMode::End);
    link.add_css_class("cmux-workspace-port");
    link.set_tooltip_text(Some(url));
    let url = url.to_string();
    let workspace_target = workspace_target.to_string();
    let app_state = Arc::clone(app_state);
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        if embedded {
            call_app(
                &app_state,
                "browser.open",
                json!({"url": url.clone(), "workspace_id": workspace_target.clone()}),
            );
        } else {
            let _ = gio::AppInfo::launch_default_for_uri(&url, None::<&gio::AppLaunchContext>);
        }
    });
    link.add_controller(gesture);
    link
}

fn sidebar_display_path(path: &str, last_segment_only: bool) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if last_segment_only {
        Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .map(ToString::to_string)
            .or_else(|| Some(path.to_string()))
    } else {
        Some(path.to_string())
    }
}

fn sidebar_compact_text(value: &str) -> Option<String> {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        None
    } else {
        Some(compact.chars().take(240).collect())
    }
}

fn sidebar_link_label(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .find(|value| !value.is_empty())
        .unwrap_or(url)
        .to_string()
}

fn apply_workspace_color_style(
    row: &gtk::Box,
    row_model: &GtkWorkspaceSidebarRow,
    settings: &config::WorkspaceColorSettings,
) {
    let tint = row_model
        .tint_hex
        .as_deref()
        .filter(|color| valid_hex_color(color));
    let mut declarations = Vec::new();
    if settings.indicator_style == "solidFill" {
        if let Some(tint) = tint {
            declarations.push(format!(
                "background: {};",
                hex_color_with_alpha(tint, if row_model.selected { 0.52 } else { 0.20 })
                    .unwrap_or_else(|| tint.to_string())
            ));
        } else if row_model.selected {
            if let Some(selection) = settings.selection_color.as_deref() {
                declarations.push(format!("background: {selection};"));
            }
        }
    } else {
        if let Some(tint) = tint {
            declarations.push(format!("border-left: 4px solid {tint}; padding-left: 6px;"));
        }
        if row_model.selected {
            if let Some(selection) = settings.selection_color.as_deref() {
                declarations.push(format!("background: {selection};"));
            }
        }
    }
    if !declarations.is_empty() {
        install_custom_sidebar_style(row.upcast_ref(), &declarations.join(" "));
    }
}

fn hex_color_with_alpha(color: &str, alpha: f64) -> Option<String> {
    let color = color.strip_prefix('#')?;
    if color.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&color[0..2], 16).ok()?;
    let green = u8::from_str_radix(&color[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&color[4..6], 16).ok()?;
    Some(format!(
        "rgba({red}, {green}, {blue}, {:.3})",
        alpha.clamp(0.0, 1.0)
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GtkWorkspaceSidebarRowKind {
    Workspace,
    GroupHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GtkWorkspaceGroupConfiguredMenuEntry {
    Separator,
    Action(GtkWorkspaceGroupConfiguredMenuAction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkWorkspaceGroupConfiguredMenuAction {
    title: String,
    tooltip: Option<String>,
    icon_symbol: Option<String>,
    action_id: String,
    action_kind: String,
    builtin: Option<String>,
    command: Option<String>,
    command_name: Option<String>,
    agent: Option<String>,
    args: Option<String>,
    workspace: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkWorkspaceGroupMenuTarget {
    target: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkWorkspaceSidebarRow {
    kind: GtkWorkspaceSidebarRowKind,
    target: String,
    anchor_workspace_target: String,
    group_target: Option<String>,
    available_group_targets: Vec<GtkWorkspaceGroupMenuTarget>,
    title: String,
    subtitle: String,
    selected: bool,
    multi_selected: bool,
    close_visible: bool,
    indented: bool,
    collapsed: bool,
    custom_title: bool,
    is_pinned: bool,
    unread: bool,
    icon_symbol: String,
    tint_hex: Option<String>,
    description: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    git_dirty: bool,
    latest_notification_text: Option<String>,
    ssh_target: Option<String>,
    listening_ports: Vec<u16>,
    pull_request_urls: Vec<String>,
    status_entries: Vec<GtkWorkspaceStatusEntry>,
    metadata_blocks: Vec<String>,
    progress: Option<GtkWorkspaceProgress>,
    latest_log: Option<String>,
    configured_context_menu_entries: Vec<GtkWorkspaceGroupConfiguredMenuEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkWorkspaceStatusEntry {
    value: String,
    color: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct GtkWorkspaceProgress {
    value: f64,
    label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct GtkWorkspaceSidebarDetails {
    description: Option<String>,
    branch_directory: Vec<String>,
    notification: Option<String>,
    ssh_target: Option<String>,
    ports: Vec<u16>,
    pull_requests: Vec<String>,
    metadata: Vec<GtkWorkspaceStatusEntry>,
    metadata_blocks: Vec<String>,
    progress: Option<GtkWorkspaceProgress>,
    log: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GtkWorkspaceDragPayload {
    Workspace {
        workspace_target: String,
        group_target: Option<String>,
        pinned: bool,
    },
    Group {
        group_target: String,
        pinned: bool,
    },
}

#[derive(Default)]
struct GtkWorkspaceDragState {
    payload: Option<GtkWorkspaceDragPayload>,
    hover: Option<GtkWorkspaceDropHover>,
}

struct GtkWorkspaceDropHover {
    target: GtkWorkspaceSidebarRow,
    y: f64,
    height: f64,
    widget: glib::WeakRef<gtk::Button>,
}

type GtkWorkspaceDragStateRef = Rc<RefCell<GtkWorkspaceDragState>>;

fn workspace_drag_payload(row: &GtkWorkspaceSidebarRow) -> Option<GtkWorkspaceDragPayload> {
    if row.target.trim().is_empty() {
        return None;
    }
    Some(match row.kind {
        GtkWorkspaceSidebarRowKind::Workspace => GtkWorkspaceDragPayload::Workspace {
            workspace_target: row.target.clone(),
            group_target: row.group_target.clone(),
            pinned: row.is_pinned,
        },
        GtkWorkspaceSidebarRowKind::GroupHeader => GtkWorkspaceDragPayload::Group {
            group_target: row.target.clone(),
            pinned: row.is_pinned,
        },
    })
}

fn workspace_drag_same_partition(
    payload: &GtkWorkspaceDragPayload,
    target: &GtkWorkspaceSidebarRow,
) -> bool {
    let GtkWorkspaceDragPayload::Workspace {
        group_target,
        pinned,
        ..
    } = payload
    else {
        return false;
    };
    match (group_target, &target.group_target) {
        (Some(source), Some(destination)) => source == destination,
        (None, None) => *pinned == target.is_pinned,
        _ => false,
    }
}

fn workspace_drop_request(
    payload: &GtkWorkspaceDragPayload,
    target: &GtkWorkspaceSidebarRow,
    y: f64,
    height: f64,
) -> Option<(&'static str, Value)> {
    let height = height.max(1.0);
    let before = y < height / 2.0;
    match payload {
        GtkWorkspaceDragPayload::Group {
            group_target,
            pinned,
        } => {
            if target.kind == GtkWorkspaceSidebarRowKind::GroupHeader {
                if group_target == &target.target || *pinned != target.is_pinned {
                    return None;
                }
                let params = if before {
                    json!({"group_id": group_target, "before_group_id": target.target})
                } else {
                    json!({"group_id": group_target, "after_group_id": target.target})
                };
                return Some(("workspace.group.move", params));
            }
            if target.group_target.is_some() || *pinned != target.is_pinned {
                return None;
            }
            let params = if before {
                json!({"group_id": group_target, "before_workspace_id": target.target})
            } else {
                json!({"group_id": group_target, "after_workspace_id": target.target})
            };
            return Some(("workspace.group.move", params));
        }
        GtkWorkspaceDragPayload::Workspace {
            workspace_target,
            group_target,
            pinned,
        } => {
            if workspace_target == &target.target {
                return None;
            }
            if target.kind == GtkWorkspaceSidebarRowKind::GroupHeader {
                let center = y > height * 0.25 && y < height * 0.75;
                if center && !*pinned && group_target.as_deref() != Some(target.target.as_str()) {
                    return Some((
                        "workspace.group.add",
                        json!({
                            "workspace_id": workspace_target,
                            "group_id": target.target
                        }),
                    ));
                }
                return None;
            }
        }
    }
    if !workspace_drag_same_partition(payload, target) {
        return None;
    }

    let GtkWorkspaceDragPayload::Workspace {
        workspace_target, ..
    } = payload
    else {
        return None;
    };
    let params = if before {
        json!({
            "workspace_id": workspace_target,
            "before_workspace_id": target.target
        })
    } else {
        json!({
            "workspace_id": workspace_target,
            "after_workspace_id": target.target
        })
    };
    Some(("workspace.reorder", params))
}

fn attach_workspace_drag_source(
    widget: &gtk::Button,
    row_model: &GtkWorkspaceSidebarRow,
    drag_state: &GtkWorkspaceDragStateRef,
    app_state: &Arc<Mutex<AppState>>,
) {
    let Some(payload) = workspace_drag_payload(row_model) else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let begin_state = Rc::clone(drag_state);
    let begin_widget = widget.downgrade();
    gesture.connect_drag_begin(move |_, _, _| {
        let Some(begin_widget) = begin_widget.upgrade() else {
            return;
        };
        let mut state = begin_state.borrow_mut();
        state.payload = Some(payload.clone());
        state.hover = None;
        begin_widget.add_css_class("cmux-drag-source");
    });
    let end_state = Rc::clone(drag_state);
    let end_widget = widget.downgrade();
    let app_state = Arc::clone(app_state);
    gesture.connect_drag_end(move |_, _, _| {
        if let Some(end_widget) = end_widget.upgrade() {
            end_widget.remove_css_class("cmux-drag-source");
        }
        let (payload, hover) = {
            let mut state = end_state.borrow_mut();
            (state.payload.take(), state.hover.take())
        };
        let (Some(payload), Some(hover)) = (payload, hover) else {
            return;
        };
        if let Some(widget) = hover.widget.upgrade() {
            widget.remove_css_class("cmux-drop-target");
        }
        if let Some((method, params)) =
            workspace_drop_request(&payload, &hover.target, hover.y, hover.height)
        {
            call_app(&app_state, method, params);
        }
    });
    widget.add_controller(gesture);
}

fn attach_workspace_drop_target(
    row: &gtk::Button,
    row_model: &GtkWorkspaceSidebarRow,
    drag_state: &GtkWorkspaceDragStateRef,
) {
    let motion = gtk::EventControllerMotion::new();
    let row_enter = row.downgrade();
    let enter_model = row_model.clone();
    let enter_state = Rc::clone(drag_state);
    motion.connect_enter(move |_, _, y| {
        if enter_state.borrow().payload.is_none() {
            return;
        }
        let Some(row_enter) = row_enter.upgrade() else {
            return;
        };
        row_enter.add_css_class("cmux-drop-target");
        enter_state.borrow_mut().hover = Some(GtkWorkspaceDropHover {
            target: enter_model.clone(),
            y,
            height: row_enter.allocated_height() as f64,
            widget: row_enter.downgrade(),
        });
    });
    let motion_model = row_model.clone();
    let motion_state = Rc::clone(drag_state);
    let motion_row = row.downgrade();
    motion.connect_motion(move |_, _, y| {
        if motion_state.borrow().payload.is_none() {
            return;
        }
        let Some(motion_row) = motion_row.upgrade() else {
            return;
        };
        motion_state.borrow_mut().hover = Some(GtkWorkspaceDropHover {
            target: motion_model.clone(),
            y,
            height: motion_row.allocated_height() as f64,
            widget: motion_row.downgrade(),
        });
    });
    let row_leave = row.downgrade();
    let leave_target = row_model.target.clone();
    let leave_state = Rc::clone(drag_state);
    motion.connect_leave(move |_| {
        let Some(row_leave) = row_leave.upgrade() else {
            return;
        };
        row_leave.remove_css_class("cmux-drop-target");
        let mut state = leave_state.borrow_mut();
        if state
            .hover
            .as_ref()
            .is_some_and(|hover| hover.target.target == leave_target)
        {
            state.hover = None;
        }
    });
    row.add_controller(motion);
}

fn workspace_sidebar_rows(snapshot: &Value) -> Vec<GtkWorkspaceSidebarRow> {
    let workspaces = snapshot
        .get("workspaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let groups = snapshot
        .get("workspace_groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let workspace_count = workspaces.len();
    let mut group_by_anchor = HashMap::<String, Value>::new();
    let mut group_collapsed = HashMap::<String, bool>::new();
    let mut group_ref_by_id = HashMap::<String, String>::new();
    let mut group_menu_targets = Vec::<GtkWorkspaceGroupMenuTarget>::new();
    let mut selected_group_ids = HashSet::<String>::new();

    for group in &groups {
        if let Some(anchor_id) = value_string(group, "anchor_workspace_id") {
            group_by_anchor.insert(anchor_id, group.clone());
        }
        if let Some(group_id) =
            value_string(group, "group_id").or_else(|| value_string(group, "id"))
        {
            group_collapsed.insert(group_id.clone(), value_bool(group, "is_collapsed"));
            let group_ref = value_string(group, "group_ref")
                .or_else(|| value_string(group, "ref"))
                .or_else(|| value_string(group, "group_id"))
                .unwrap_or_default();
            if !group_ref.trim().is_empty() {
                group_ref_by_id.insert(group_id.to_string(), group_ref.clone());
                group_menu_targets.push(GtkWorkspaceGroupMenuTarget {
                    target: group_ref,
                    title: value_str(group, "name", "Group").to_string(),
                });
            }
        }
    }

    for workspace in &workspaces {
        if workspace_selected(workspace) {
            if let Some(group_id) = value_string(workspace, "group_id") {
                selected_group_ids.insert(group_id);
            }
        }
    }

    let mut rows = Vec::new();
    for workspace in workspaces {
        let Some(id) = workspace_id(&workspace) else {
            continue;
        };
        if let Some(group) = group_by_anchor.get(&id) {
            let group_id = value_string(group, "group_id")
                .or_else(|| value_string(group, "id"))
                .unwrap_or_default();
            rows.push(workspace_group_sidebar_model(
                group,
                &workspace,
                selected_group_ids.contains(group_id.as_str()),
            ));
            continue;
        }

        let group_id = value_string(&workspace, "group_id");
        if group_id
            .as_deref()
            .and_then(|group_id| group_collapsed.get(group_id))
            .copied()
            .unwrap_or(false)
        {
            continue;
        }

        rows.push(workspace_sidebar_model(
            &workspace,
            workspace_count,
            group_id.is_some(),
            group_id
                .as_deref()
                .and_then(|group_id| group_ref_by_id.get(group_id))
                .cloned(),
            group_menu_targets.clone(),
        ));
    }

    rows.into_iter()
        .filter(|row| !row.target.trim().is_empty())
        .collect()
}

fn workspace_sidebar_model(
    workspace: &Value,
    workspace_count: usize,
    indented: bool,
    group_target: Option<String>,
    available_group_targets: Vec<GtkWorkspaceGroupMenuTarget>,
) -> GtkWorkspaceSidebarRow {
    let target = workspace_id_or_ref(workspace).unwrap_or_default();
    GtkWorkspaceSidebarRow {
        kind: GtkWorkspaceSidebarRowKind::Workspace,
        target,
        anchor_workspace_target: String::new(),
        group_target,
        available_group_targets,
        title: value_str(workspace, "title", "Workspace").to_string(),
        subtitle: value_str(workspace, "workspace_ref", "").to_string(),
        selected: workspace_selected(workspace),
        multi_selected: value_bool(workspace, "multi_selected"),
        close_visible: workspace_close_button_visible(workspace, workspace_count),
        indented,
        collapsed: false,
        custom_title: value_bool(workspace, "custom_title"),
        is_pinned: value_bool(workspace, "pinned"),
        unread: value_bool(workspace, "unread"),
        icon_symbol: String::new(),
        tint_hex: value_string(workspace, "effective_color")
            .or_else(|| value_string(workspace, "custom_color"))
            .or_else(|| value_string(workspace, "color")),
        description: value_string(workspace, "description"),
        cwd: value_string(workspace, "cwd"),
        git_branch: value_string(workspace, "git_branch"),
        git_dirty: value_bool(workspace, "git_dirty"),
        latest_notification_text: value_string(workspace, "latest_notification_text"),
        ssh_target: value_string(workspace, "ssh_target"),
        listening_ports: workspace
            .get("listening_ports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .filter_map(|port| u16::try_from(port).ok())
            .collect(),
        pull_request_urls: workspace
            .get("pull_request_urls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        status_entries: workspace
            .get("status_entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                Some(GtkWorkspaceStatusEntry {
                    value: value_string(entry, "value")?,
                    color: value_string(entry, "color"),
                    url: value_string(entry, "url"),
                })
            })
            .collect(),
        metadata_blocks: workspace
            .get("metadata_blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| value_string(block, "markdown"))
            .collect(),
        progress: workspace.get("progress").and_then(|progress| {
            Some(GtkWorkspaceProgress {
                value: progress.get("value")?.as_f64()?,
                label: value_string(progress, "label"),
            })
        }),
        latest_log: workspace.get("latest_log").and_then(|entry| {
            let message = value_string(entry, "message")?;
            Some(
                value_string(entry, "source")
                    .map(|source| format!("{source}: {message}"))
                    .unwrap_or(message),
            )
        }),
        configured_context_menu_entries: Vec::new(),
    }
}

fn workspace_group_sidebar_model(
    group: &Value,
    anchor: &Value,
    selected_group_member: bool,
) -> GtkWorkspaceSidebarRow {
    let group_ref = value_string(group, "group_ref")
        .or_else(|| value_string(group, "ref"))
        .or_else(|| value_string(group, "group_id"))
        .unwrap_or_default();
    let member_count = group
        .get("member_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            group
                .get("member_workspace_ids")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(1) as u64
        }) as usize;
    let collapsed = value_bool(group, "is_collapsed");
    let selected = workspace_selected(anchor) || (collapsed && selected_group_member);
    GtkWorkspaceSidebarRow {
        kind: GtkWorkspaceSidebarRowKind::GroupHeader,
        target: group_ref.clone(),
        anchor_workspace_target: workspace_id_or_ref(anchor)
            .or_else(|| value_string(group, "anchor_workspace_id"))
            .or_else(|| value_string(group, "anchor_workspace_ref"))
            .unwrap_or_default(),
        group_target: None,
        available_group_targets: Vec::new(),
        title: value_str(group, "name", "Group").to_string(),
        subtitle: format!("{} · {}", group_ref, workspace_count_label(member_count)),
        selected,
        multi_selected: value_bool(anchor, "multi_selected"),
        close_visible: false,
        indented: false,
        collapsed,
        custom_title: false,
        is_pinned: value_bool(group, "is_pinned"),
        unread: false,
        icon_symbol: value_string(group, "effective_icon_symbol")
            .or_else(|| value_string(group, "icon_symbol"))
            .unwrap_or_else(|| "folder.fill".to_string()),
        tint_hex: value_string(group, "effective_color")
            .or_else(|| value_string(group, "custom_color")),
        description: None,
        cwd: None,
        git_branch: None,
        git_dirty: false,
        latest_notification_text: None,
        ssh_target: None,
        listening_ports: Vec::new(),
        pull_request_urls: Vec::new(),
        status_entries: Vec::new(),
        metadata_blocks: Vec::new(),
        progress: None,
        latest_log: None,
        configured_context_menu_entries: workspace_group_configured_menu_entries(group),
    }
}

fn workspace_group_title_markup(row: &GtkWorkspaceSidebarRow) -> String {
    let title = glib::markup_escape_text(&format!("{}  {}", row.icon_symbol, row.title));
    if let Some(color) = row
        .tint_hex
        .as_deref()
        .filter(|color| valid_hex_color(color))
    {
        format!("<span foreground=\"{color}\">{title}</span>")
    } else {
        title.to_string()
    }
}

fn workspace_group_configured_menu_entries(
    group: &Value,
) -> Vec<GtkWorkspaceGroupConfiguredMenuEntry> {
    group
        .get("configured_context_menu_items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let item_type = value_str(item, "type", "");
            if item_type == "separator" {
                return Some(GtkWorkspaceGroupConfiguredMenuEntry::Separator);
            }
            if item_type != "action" {
                return None;
            }
            let action = item.get("action").unwrap_or(&Value::Null);
            Some(GtkWorkspaceGroupConfiguredMenuEntry::Action(
                GtkWorkspaceGroupConfiguredMenuAction {
                    title: value_string(item, "title")
                        .or_else(|| value_string(item, "action_id"))
                        .unwrap_or_else(|| "Action".to_string()),
                    tooltip: value_string(item, "tooltip"),
                    icon_symbol: value_string(item, "icon_symbol"),
                    action_id: value_string(item, "action_id").unwrap_or_default(),
                    action_kind: value_string(action, "kind")
                        .unwrap_or_else(|| "unknown".to_string()),
                    builtin: value_string(action, "builtin"),
                    command: value_string(action, "command"),
                    command_name: value_string(action, "command_name"),
                    agent: value_string(action, "agent"),
                    args: value_string(action, "args"),
                    workspace: action.get("workspace").cloned(),
                },
            ))
        })
        .collect()
}

fn workspace_group_configured_action_request(
    row: &GtkWorkspaceSidebarRow,
    action: &GtkWorkspaceGroupConfiguredMenuAction,
) -> Option<(&'static str, Value)> {
    if row.target.trim().is_empty() {
        return None;
    }
    match (action.action_kind.as_str(), action.builtin.as_deref()) {
        ("builtin", Some("cmux.newWorkspace")) => Some((
            "workspace.group.new_workspace",
            json!({"group_id": row.target.as_str()}),
        )),
        ("builtin", Some("cmux.cloudvm")) => Some((
            "vm.create",
            json!({
                "idempotency_key": format!(
                    "gtk-workspace-group-{}",
                    Uuid::new_v4()
                ),
                "source": "workspace_group_context_menu",
                "group_id": row.target.as_str(),
                "action_id": action.action_id.as_str()
            }),
        )),
        ("builtin", Some("cmux.newTerminal")) if !row.anchor_workspace_target.trim().is_empty() => {
            Some((
                "surface.create",
                json!({
                    "workspace_id": row.anchor_workspace_target.as_str(),
                    "type": "terminal",
                    "focus": true
                }),
            ))
        }
        ("builtin", Some("cmux.newBrowser")) if !row.anchor_workspace_target.trim().is_empty() => {
            Some((
                "surface.create",
                json!({
                    "workspace_id": row.anchor_workspace_target.as_str(),
                    "type": "browser",
                    "url": "about:blank",
                    "focus": true
                }),
            ))
        }
        ("builtin", Some("cmux.splitRight")) if !row.anchor_workspace_target.trim().is_empty() => {
            Some((
                "surface.split",
                json!({
                    "workspace_id": row.anchor_workspace_target.as_str(),
                    "type": "terminal",
                    "direction": "right",
                    "focus": true
                }),
            ))
        }
        ("builtin", Some("cmux.splitDown")) if !row.anchor_workspace_target.trim().is_empty() => {
            Some((
                "surface.split",
                json!({
                    "workspace_id": row.anchor_workspace_target.as_str(),
                    "type": "terminal",
                    "direction": "down",
                    "focus": true
                }),
            ))
        }
        ("command", _) if !row.anchor_workspace_target.trim().is_empty() => action
            .command
            .as_deref()
            .and_then(|command| workspace_group_terminal_action_request(row, action, command)),
        ("agent", _) if !row.anchor_workspace_target.trim().is_empty() => {
            workspace_group_agent_command(action)
                .as_deref()
                .and_then(|command| workspace_group_terminal_action_request(row, action, command))
        }
        ("workspace_command", _) => workspace_group_workspace_command_action_request(row, action),
        _ => None,
    }
}

fn workspace_group_terminal_action_request(
    row: &GtkWorkspaceSidebarRow,
    action: &GtkWorkspaceGroupConfiguredMenuAction,
    command: &str,
) -> Option<(&'static str, Value)> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    Some((
        "surface.create",
        json!({
            "workspace_id": row.anchor_workspace_target.as_str(),
            "type": "terminal",
            "title": action.title.as_str(),
            "command": command,
            "focus": true,
            "configured_action_id": action.action_id.as_str()
        }),
    ))
}

fn workspace_group_agent_command(action: &GtkWorkspaceGroupConfiguredMenuAction) -> Option<String> {
    let agent = action.agent.as_deref()?.trim();
    if agent.is_empty() {
        return None;
    }
    let command = match agent {
        "claude" | "claudeCode" | "claude-code" => "claude",
        "codex" => "codex",
        other => other,
    };
    let args = action.args.as_deref().map(str::trim).unwrap_or_default();
    if args.is_empty() {
        Some(command.to_string())
    } else {
        Some(format!("{command} {args}"))
    }
}

fn workspace_group_workspace_command_action_request(
    row: &GtkWorkspaceSidebarRow,
    action: &GtkWorkspaceGroupConfiguredMenuAction,
) -> Option<(&'static str, Value)> {
    if row.target.trim().is_empty() {
        return None;
    }
    let workspace = action.workspace.as_ref()?;
    let mut params = json!({
        "group_id": row.target.as_str(),
        "focus": true,
        "title": action
            .command_name
            .as_deref()
            .unwrap_or(action.title.as_str()),
        "configured_action_id": action.action_id.as_str()
    });
    if let Some(title) =
        value_string(workspace, "name").or_else(|| value_string(workspace, "title"))
    {
        params["title"] = json!(title);
    }
    if let Some(cwd) =
        value_string(workspace, "cwd").or_else(|| value_string(workspace, "working_directory"))
    {
        params["cwd"] = json!(cwd);
    }
    if let Some(color) =
        value_string(workspace, "color").or_else(|| value_string(workspace, "custom_color"))
    {
        params["color"] = json!(color);
    }
    if let Some(env) = workspace.get("env").filter(|value| value.is_object()) {
        params["workspace_env"] = env.clone();
    }
    if let Some(layout) = workspace.get("layout").filter(|value| value.is_object()) {
        params["layout"] = layout.clone();
    }
    Some(("workspace.group.new_workspace", params))
}

fn workspace_count_label(count: usize) -> String {
    if count == 1 {
        "1 workspace".to_string()
    } else {
        format!("{count} workspaces")
    }
}

fn workspace_id(value: &Value) -> Option<String> {
    value
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(ToString::to_string)
}

fn value_non_empty_raw_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn value_f32(value: &Value, key: &str) -> Option<f32> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number > 0.0)
        .map(|number| number.clamp(1.0, 255.0) as f32)
}

fn value_string_pairs(value: &Value, key: &str) -> Vec<(String, String)> {
    let mut pairs = value
        .get(key)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();
    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    pairs
}

fn value_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn value_bool_or(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn ghostty_surface_options(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    config_reload_generation: u64,
) -> crate::gtk_ghostty::GhosttySurfaceOptions {
    let terminal_settings = app_state
        .lock()
        .map(|app| app.terminal_interaction_settings())
        .unwrap_or_default();
    crate::gtk_ghostty::GhosttySurfaceOptions {
        working_directory: value_string(view, "current_directory")
            .or_else(|| value_string(view, "cwd")),
        command: value_string(view, "terminal_command"),
        initial_input: value_non_empty_raw_string(view, "terminal_initial_input"),
        initial_output: value_non_empty_raw_string(view, "terminal_restore_output"),
        font_size: value_f32(view, "terminal_font_size"),
        wait_after_command: value_bool(view, "terminal_wait_after_command"),
        env: value_string_pairs(view, "terminal_env"),
        manual_io: value_bool(view, "remote_tmux_manual_io"),
        focused: value_bool(view, "focused"),
        occluded: !value_bool_or(view, "visible", true),
        copy_mode_active: value_bool(view, "terminal_copy_mode_active"),
        show_scroll_bar: terminal_settings.show_scroll_bar,
        scrollbar: ghostty_scrollbar_state(view),
        config_reload_generation,
        close_surface_id: surface_id_or_ref(view),
        app_state: Some(Arc::clone(app_state)),
    }
}

fn ghostty_scrollbar_state(view: &Value) -> Option<crate::gtk_ghostty::GhosttyScrollbarState> {
    let scrollbar = view.get("terminal_scrollbar")?.as_object()?;
    Some(crate::gtk_ghostty::GhosttyScrollbarState {
        total: scrollbar.get("total")?.as_u64()?,
        offset: scrollbar.get("offset")?.as_u64()?,
        len: scrollbar
            .get("len")
            .or_else(|| scrollbar.get("visible"))?
            .as_u64()?,
    })
}

fn config_reload_generation(snapshot: &Value) -> u64 {
    snapshot
        .get("config")
        .and_then(|config| {
            config
                .get("reload_generation")
                .or_else(|| config.get("config_reload_generation"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn ghostty_surface_cache_key(view: &Value) -> Option<String> {
    let kind = view
        .get("kind")
        .or_else(|| view.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("terminal");
    if kind != "terminal" {
        return None;
    }
    if value_bool(view, "hibernated")
        || view
            .get("agent_hibernation")
            .is_some_and(|value| value.is_object())
    {
        return None;
    }
    surface_id_or_ref(view)
}

fn prune_ghostty_surface_widgets(ghostty_widgets: &GhosttySurfaceWidgets, views: &[Value]) {
    let active = views
        .iter()
        .filter_map(ghostty_surface_cache_key)
        .collect::<HashSet<_>>();
    let stale = ghostty_widgets
        .borrow()
        .keys()
        .filter(|surface_id| !active.contains(*surface_id))
        .cloned()
        .collect::<Vec<_>>();
    for surface_id in stale {
        let Some(widget) = ghostty_widgets.borrow_mut().remove(&surface_id) else {
            continue;
        };
        detach_widget(widget.root());
        widget.shutdown();
    }
}

fn prune_browser_surface_controls(browser_controls: &BrowserSurfaceControlsCache, views: &[Value]) {
    let active = views
        .iter()
        .filter(|view| {
            view.get("kind")
                .or_else(|| view.get("type"))
                .and_then(Value::as_str)
                == Some("browser")
        })
        .filter_map(surface_id_or_ref)
        .collect::<HashSet<_>>();
    browser_controls
        .borrow_mut()
        .retain(|surface_id, _| active.contains(surface_id));
}

fn prune_diff_surface_controls(diff_controls: &DiffSurfaceControlsCache, views: &[Value]) {
    let active = views
        .iter()
        .filter(|view| {
            view.get("kind")
                .or_else(|| view.get("type"))
                .and_then(Value::as_str)
                == Some("diff")
        })
        .filter_map(surface_id_or_ref)
        .collect::<HashSet<_>>();
    diff_controls
        .borrow_mut()
        .retain(|surface_id, _| active.contains(surface_id));
}

fn browser_profile_choices(value: &Value) -> Vec<(String, String)> {
    value
        .get("profiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|profile| {
            let id = profile.get("id").and_then(Value::as_str)?.trim();
            let name = profile.get("name").and_then(Value::as_str)?.trim();
            (!id.is_empty() && !name.is_empty()).then(|| (id.to_string(), name.to_string()))
        })
        .collect()
}

fn browser_omnibar_suggestions(value: &Value) -> Vec<BrowserOmnibarSuggestion> {
    value
        .get("suggestions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|suggestion| {
            let url = suggestion.get("url").and_then(Value::as_str)?.trim();
            if url.is_empty() {
                return None;
            }
            Some(BrowserOmnibarSuggestion {
                kind: suggestion
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("history")
                    .to_string(),
                completion: suggestion
                    .get("completion")
                    .and_then(Value::as_str)
                    .unwrap_or(url)
                    .to_string(),
                url: url.to_string(),
                title: suggestion
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(url)
                    .to_string(),
                badge: suggestion
                    .get("badge")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                surface_id: suggestion
                    .get("surface_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn replace_browser_omnibar_suggestions(
    list: &gtk::ListBox,
    popover: &gtk::Popover,
    suggestions: &Rc<RefCell<Vec<BrowserOmnibarSuggestion>>>,
    next: Vec<BrowserOmnibarSuggestion>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for suggestion in &next {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("cmux-browser-suggestion");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(Some(&suggestion.title));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(54);
        title_row.append(&title);
        if let Some(badge) = suggestion.badge.as_deref() {
            let badge = gtk::Label::new(Some(badge));
            badge.add_css_class("cmux-muted");
            badge.set_xalign(1.0);
            title_row.append(&badge);
        }
        let detail = gtk::Label::new(Some(&suggestion.url));
        detail.add_css_class("cmux-muted");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        detail.set_max_width_chars(62);
        content.append(&title_row);
        content.append(&detail);
        row.set_child(Some(&content));
        list.append(&row);
    }
    list.unselect_all();
    suggestions.replace(next);
    if suggestions.borrow().is_empty() {
        popover.popdown();
    } else {
        popover.popup();
    }
}

fn browser_omnibar_display_text(url: &str) -> &str {
    if url.trim().eq_ignore_ascii_case("about:blank") {
        ""
    } else {
        url
    }
}

fn commit_browser_omnibar_suggestion(
    app_state: &Arc<Mutex<AppState>>,
    current_surface_id: &str,
    web_view: Option<&crate::gtk_webkit::GtkWebKitView>,
    model_url: &Rc<RefCell<String>>,
    suggestion: &BrowserOmnibarSuggestion,
) -> bool {
    if suggestion.kind == "switch_tab" {
        return suggestion.surface_id.as_deref().is_some_and(|surface_id| {
            call_app(
                app_state,
                "surface.focus",
                json!({"surface_id": surface_id}),
            )
        });
    }
    navigate_browser_omnibar(
        app_state,
        current_surface_id,
        web_view,
        model_url,
        &suggestion.completion,
    )
}

fn navigate_browser_omnibar(
    app_state: &Arc<Mutex<AppState>>,
    surface_id: &str,
    web_view: Option<&crate::gtk_webkit::GtkWebKitView>,
    model_url: &Rc<RefCell<String>>,
    input: &str,
) -> bool {
    let Some(resolved) = call_app_value(
        app_state,
        "browser.omnibar.resolve",
        json!({"surface_id": surface_id, "input": input}),
    ) else {
        return false;
    };
    let Some(url) = resolved
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    else {
        return false;
    };
    model_url.replace(url.to_string());
    if !call_app(
        app_state,
        "browser.navigate",
        json!({"surface_id": surface_id, "url": url}),
    ) {
        return false;
    }
    if let Some(view) = web_view {
        let _ = view.load_uri(url);
        view.widget().grab_focus();
    }
    true
}

fn browser_surface_requires_recreation(
    profile_id: &str,
    profile_data_generation: u64,
    state: &ui::BrowserNavigationState,
) -> bool {
    profile_id != state.profile_id || profile_data_generation != state.profile_data_generation
}

fn ensure_browser_surface_controls(
    state: &ui::BrowserNavigationState,
    global_search_needle: Option<&str>,
    app_state: &Arc<Mutex<AppState>>,
    browser_controls: &BrowserSurfaceControlsCache,
    ghostty_widgets: &GhosttySurfaceWidgets,
) -> BrowserSurfaceControls {
    let controls = {
        let mut controls = browser_controls.borrow_mut();
        if controls.get(&state.surface_id).is_some_and(|controls| {
            browser_surface_requires_recreation(
                &controls.profile_id,
                controls.profile_data_generation,
                state,
            )
        }) {
            controls.remove(&state.surface_id);
        }
        controls
            .entry(state.surface_id.clone())
            .or_insert_with(|| {
                let root = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                root.add_css_class("cmux-browser-toolbar");
                root.set_overflow(gtk::Overflow::Hidden);

                let model_url = Rc::new(RefCell::new(state.url.clone()));
                let global_search_needle = Rc::new(RefCell::new(None));
                let page_zoom = Rc::new(Cell::new(1.0));
                let user_agent = Rc::new(RefCell::new(String::new()));
                let applied_offline = Rc::new(Cell::new(None));
                let request_configuration_generation = Rc::new(Cell::new(0));
                let applied_init_scripts = Rc::new(RefCell::new(Vec::new()));
                let applied_storage = Rc::new(RefCell::new(None));
                let developer_tools_visible = Rc::new(Cell::new(false));
                let last_runtime_action_sequence = Rc::new(Cell::new(0));
                let model_focused = Rc::new(Cell::new(false));
                let browser_chrome_focused = Rc::new(Cell::new(false));
                let web_view = match crate::gtk_webkit::GtkWebKitView::new(
                    &state.profile_id,
                    state.profile_data_generation,
                ) {
                    Ok(view) => Some(view),
                    Err(err) => {
                        eprintln!("cmux: native WebKit browser unavailable: {err}");
                        None
                    }
                };
                if let Some(view) = web_view.as_ref() {
                    let map_focused = Rc::clone(&model_focused);
                    let map_browser_chrome_focused = Rc::clone(&browser_chrome_focused);
                    view.widget().connect_map(move |widget| {
                        if map_focused.get() && !map_browser_chrome_focused.get() {
                            widget.grab_focus();
                        }
                    });
                    let app_state_for_focus = Arc::clone(app_state);
                    let surface_id_for_focus = state.surface_id.clone();
                    let browser_chrome_for_webview = Rc::clone(&browser_chrome_focused);
                    view.widget()
                        .connect_notify_local(Some("has-focus"), move |widget, _| {
                            if widget.has_focus() {
                                browser_chrome_for_webview.set(false);
                            }
                            call_app(
                                &app_state_for_focus,
                                "browser.runtime.sync",
                                json!({
                                    "surface_id": surface_id_for_focus,
                                    "webview_focused": widget.has_focus()
                                }),
                            );
                        });
                    let view_for_uri = view.downgrade();
                    let model_url_for_uri = Rc::clone(&model_url);
                    let app_state_for_uri = Arc::clone(app_state);
                    let surface_id_for_uri = state.surface_id.clone();
                    view.widget()
                        .connect_notify_local(Some("uri"), move |_, _| {
                            let Some(view) = view_for_uri.upgrade() else {
                                return;
                            };
                            let Some(uri) = view.uri().filter(|uri| !uri.is_empty()) else {
                                return;
                            };
                            if model_url_for_uri.borrow().as_str() == uri {
                                return;
                            }
                            *model_url_for_uri.borrow_mut() = uri.clone();
                            call_app(
                                &app_state_for_uri,
                                "browser.navigate",
                                json!({"surface_id": surface_id_for_uri, "url": uri}),
                            );
                        });
                }

                let back = browser_icon_button("go-previous-symbolic", "Back");
                let back_app_state = Arc::clone(app_state);
                let back_surface_id = state.surface_id.clone();
                let back_web_view = web_view.as_ref().map(|view| view.downgrade());
                back.connect_clicked(move |_| {
                    call_app(
                        &back_app_state,
                        "browser.back",
                        json!({"surface_id": back_surface_id}),
                    );
                    if let Some(view) = back_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.go_back();
                    }
                });

                let forward = browser_icon_button("go-next-symbolic", "Forward");
                let forward_app_state = Arc::clone(app_state);
                let forward_surface_id = state.surface_id.clone();
                let forward_web_view = web_view.as_ref().map(|view| view.downgrade());
                forward.connect_clicked(move |_| {
                    call_app(
                        &forward_app_state,
                        "browser.forward",
                        json!({"surface_id": forward_surface_id}),
                    );
                    if let Some(view) = forward_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.go_forward();
                    }
                });

                let reload = browser_icon_button("view-refresh-symbolic", "Reload");
                let reload_app_state = Arc::clone(app_state);
                let reload_surface_id = state.surface_id.clone();
                let reload_web_view = web_view.as_ref().map(|view| view.downgrade());
                reload.connect_clicked(move |_| {
                    call_app(
                        &reload_app_state,
                        "browser.reload",
                        json!({"surface_id": reload_surface_id}),
                    );
                    if let Some(view) = reload_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.reload();
                    }
                });
                root.append(&back);
                root.append(&reload);

                let location = gtk::Entry::new();
                location.add_css_class("cmux-browser-location");
                location.set_hexpand(true);
                location.set_width_chars(1);
                location.set_max_width_chars(1);
                location.set_placeholder_text(Some("Search or enter address"));
                let location_viewport = gtk::ScrolledWindow::new();
                location_viewport.set_hexpand(true);
                location_viewport.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
                location_viewport.set_min_content_width(40);
                location_viewport.set_propagate_natural_width(false);
                location_viewport.set_propagate_natural_height(true);
                location_viewport.set_child(Some(&location));
                if let Some(view) = web_view.as_ref() {
                    let view_for_title = view.downgrade();
                    let location_for_title = location.clone();
                    let app_state_for_title = Arc::clone(app_state);
                    let surface_id_for_title = state.surface_id.clone();
                    view.widget()
                        .connect_notify_local(Some("title"), move |_, _| {
                            let Some(view) = view_for_title.upgrade() else {
                                return;
                            };
                            let title = view.title();
                            location_for_title.set_tooltip_text(title.as_deref());
                            if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
                                call_app(
                                    &app_state_for_title,
                                    "browser.runtime.sync",
                                    json!({"surface_id": surface_id_for_title, "title": title}),
                                );
                            }
                        });
                }

                let omnibar_popover = gtk::Popover::new();
                omnibar_popover.set_autohide(true);
                omnibar_popover.set_has_arrow(false);
                parent_context_popover(&omnibar_popover, &location);
                let omnibar_scroll = gtk::ScrolledWindow::new();
                omnibar_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
                omnibar_scroll.set_min_content_width(340);
                omnibar_scroll.set_max_content_height(320);
                let omnibar_list = gtk::ListBox::new();
                omnibar_list.add_css_class("cmux-browser-suggestions");
                omnibar_list.set_selection_mode(gtk::SelectionMode::Single);
                omnibar_list.set_activate_on_single_click(true);
                omnibar_scroll.set_child(Some(&omnibar_list));
                omnibar_popover.set_child(Some(&omnibar_scroll));
                let omnibar_suggestions =
                    Rc::new(RefCell::new(Vec::<BrowserOmnibarSuggestion>::new()));
                let omnibar_refresh_suppressed = Rc::new(Cell::new(false));
                let omnibar_remote_generation = Arc::new(AtomicU64::new(0));

                {
                    let app_state = Arc::clone(app_state);
                    let surface_id = state.surface_id.clone();
                    let web_view = web_view.as_ref().map(|view| view.downgrade());
                    let model_url = Rc::clone(&model_url);
                    let suggestions = Rc::clone(&omnibar_suggestions);
                    let popover = omnibar_popover.downgrade();
                    omnibar_list.connect_row_activated(move |_, row| {
                        let Some(suggestion) =
                            suggestions.borrow().get(row.index() as usize).cloned()
                        else {
                            return;
                        };
                        let web_view = web_view.as_ref().and_then(|view| view.upgrade());
                        if commit_browser_omnibar_suggestion(
                            &app_state,
                            &surface_id,
                            web_view.as_ref(),
                            &model_url,
                            &suggestion,
                        ) {
                            if let Some(popover) = popover.upgrade() {
                                popover.popdown();
                            }
                        }
                    });
                }

                let refresh_suggestions = {
                    let app_state = Arc::clone(app_state);
                    let surface_id = state.surface_id.clone();
                    let location = location.downgrade();
                    let list = omnibar_list.downgrade();
                    let popover = omnibar_popover.downgrade();
                    let suggestions = Rc::clone(&omnibar_suggestions);
                    let suppressed = Rc::clone(&omnibar_refresh_suppressed);
                    let remote_generation = Arc::clone(&omnibar_remote_generation);
                    Rc::new(move || {
                        let generation = remote_generation.fetch_add(1, Ordering::Relaxed) + 1;
                        let Some(location) = location.upgrade() else {
                            return;
                        };
                        let Some(list) = list.upgrade() else {
                            return;
                        };
                        let Some(popover) = popover.upgrade() else {
                            return;
                        };
                        if suppressed.get() || !location.has_focus() {
                            popover.popdown();
                            return;
                        }
                        let query = location.text().to_string();
                        let next = call_app_value(
                            &app_state,
                            "browser.omnibar.suggestions",
                            json!({
                                "surface_id": surface_id,
                                "query": query,
                                "limit": 8
                            }),
                        )
                        .as_ref()
                        .map(browser_omnibar_suggestions)
                        .unwrap_or_default();
                        replace_browser_omnibar_suggestions(&list, &popover, &suggestions, next);

                        let settings = config::browser_search_settings();
                        if !browser_omnibar::should_fetch_remote_search_suggestions(
                            &settings, &query,
                        ) {
                            return;
                        }
                        let result = Arc::new(Mutex::new(None::<Vec<String>>));
                        let worker_result = Arc::clone(&result);
                        let worker_generation = Arc::clone(&remote_generation);
                        let worker_query = query.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(100));
                            if worker_generation.load(Ordering::Relaxed) != generation {
                                return;
                            }
                            let remote = browser_omnibar::fetch_remote_search_suggestions(
                                &settings,
                                &worker_query,
                            );
                            if let Ok(mut slot) = worker_result.lock() {
                                *slot = Some(remote);
                            }
                        });

                        let result = Arc::clone(&result);
                        let app_state = Arc::clone(&app_state);
                        let surface_id = surface_id.clone();
                        let location = location.clone();
                        let list = list.clone();
                        let popover = popover.clone();
                        let suggestions = Rc::clone(&suggestions);
                        let remote_generation = Arc::clone(&remote_generation);
                        glib::timeout_add_local(Duration::from_millis(50), move || {
                            if remote_generation.load(Ordering::Relaxed) != generation
                                || !location.has_focus()
                                || location.text().as_str() != query
                            {
                                return glib::ControlFlow::Break;
                            }
                            let remote = result.lock().ok().and_then(|mut slot| slot.take());
                            let Some(remote) = remote else {
                                return glib::ControlFlow::Continue;
                            };
                            if remote.is_empty() {
                                return glib::ControlFlow::Break;
                            }
                            let next = call_app_value(
                                &app_state,
                                "browser.omnibar.suggestions",
                                json!({
                                    "surface_id": surface_id,
                                    "query": query,
                                    "limit": 8,
                                    "remote_suggestions": remote
                                }),
                            )
                            .as_ref()
                            .map(browser_omnibar_suggestions)
                            .unwrap_or_default();
                            replace_browser_omnibar_suggestions(
                                &list,
                                &popover,
                                &suggestions,
                                next,
                            );
                            glib::ControlFlow::Break
                        });
                    })
                };
                {
                    let refresh = Rc::clone(&refresh_suggestions);
                    location.connect_changed(move |_| refresh());
                }
                {
                    let refresh = Rc::clone(&refresh_suggestions);
                    let popover = omnibar_popover.clone();
                    let browser_chrome_focused = Rc::clone(&browser_chrome_focused);
                    let location_focus_acquired = Rc::new(Cell::new(false));
                    location.connect_notify_local(Some("has-focus"), move |entry, _| {
                        if entry.has_focus() {
                            location_focus_acquired.set(true);
                            browser_chrome_focused.set(true);
                            refresh();
                        } else {
                            if location_focus_acquired.replace(false) {
                                browser_chrome_focused.set(false);
                            }
                            popover.popdown();
                        }
                    });
                }

                {
                    let app_state = Arc::clone(app_state);
                    let surface_id = state.surface_id.clone();
                    let web_view = web_view.as_ref().map(|view| view.downgrade());
                    let model_url = Rc::clone(&model_url);
                    let popover = omnibar_popover.clone();
                    location.connect_activate(move |entry| {
                        let web_view = web_view.as_ref().and_then(|view| view.upgrade());
                        if navigate_browser_omnibar(
                            &app_state,
                            &surface_id,
                            web_view.as_ref(),
                            &model_url,
                            entry.text().as_str(),
                        ) {
                            popover.popdown();
                        }
                    });
                }

                let omnibar_keys = gtk::EventControllerKey::new();
                omnibar_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
                {
                    let list = omnibar_list.clone();
                    let suggestions = Rc::clone(&omnibar_suggestions);
                    let popover = omnibar_popover.clone();
                    let app_state = Arc::clone(app_state);
                    let surface_id = state.surface_id.clone();
                    let web_view = web_view.as_ref().map(|view| view.downgrade());
                    let model_url_for_navigation = Rc::clone(&model_url);
                    let model_url_for_escape = Rc::clone(&model_url);
                    let location_for_escape = location.downgrade();
                    let suppressed = Rc::clone(&omnibar_refresh_suppressed);
                    let ghostty_widgets = Rc::clone(ghostty_widgets);
                    let browser_controls = Rc::clone(browser_controls);
                    omnibar_keys.connect_key_pressed(move |_, keyval, _, modifiers| {
                        if let Some(combo) = omnibar_pane_focus_combo(keyval, modifiers) {
                            let previous_surface_id = app_state
                                .lock()
                                .ok()
                                .and_then(|app| app.current_input_surface_id());
                            let handled = call_app_value(
                                &app_state,
                                "debug.shortcut.simulate",
                                json!({
                                    "combo": combo,
                                    "context": shortcut_focus_context_from_flags(false, true, false)
                                }),
                            )
                            .is_some_and(|result| {
                                result.get("handled").and_then(Value::as_bool) != Some(false)
                            });
                            if handled {
                                focus_changed_model_surface(
                                    &app_state,
                                    &ghostty_widgets,
                                    &browser_controls,
                                    previous_surface_id.as_deref(),
                                );
                                return glib::Propagation::Stop;
                            }
                        }
                        if modifiers.intersects(
                            gdk::ModifierType::CONTROL_MASK
                                | gdk::ModifierType::ALT_MASK
                                | gdk::ModifierType::SUPER_MASK
                                | gdk::ModifierType::META_MASK,
                        ) {
                            return glib::Propagation::Proceed;
                        }
                        if keyval == gdk::Key::Down || keyval == gdk::Key::Up {
                            let count = suggestions.borrow().len() as i32;
                            if count == 0 {
                                return glib::Propagation::Proceed;
                            }
                            let current = list.selected_row().map(|row| row.index());
                            let next = if keyval == gdk::Key::Down {
                                current.map(|index| (index + 1).min(count - 1)).unwrap_or(0)
                            } else {
                                current.map(|index| (index - 1).max(0)).unwrap_or(count - 1)
                            };
                            if let Some(row) = list.row_at_index(next) {
                                list.select_row(Some(&row));
                            }
                            popover.popup();
                            return glib::Propagation::Stop;
                        }
                        if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                            let Some(row) = list.selected_row() else {
                                return glib::Propagation::Proceed;
                            };
                            let Some(suggestion) =
                                suggestions.borrow().get(row.index() as usize).cloned()
                            else {
                                return glib::Propagation::Proceed;
                            };
                            let web_view = web_view.as_ref().and_then(|view| view.upgrade());
                            if commit_browser_omnibar_suggestion(
                                &app_state,
                                &surface_id,
                                web_view.as_ref(),
                                &model_url_for_navigation,
                                &suggestion,
                            ) {
                                popover.popdown();
                                return glib::Propagation::Stop;
                            }
                        }
                        if keyval == gdk::Key::Escape {
                            let Some(location_for_escape) = location_for_escape.upgrade() else {
                                return glib::Propagation::Proceed;
                            };
                            popover.popdown();
                            suppressed.set(true);
                            let current_url = model_url_for_escape.borrow();
                            location_for_escape
                                .set_text(browser_omnibar_display_text(current_url.as_str()));
                            suppressed.set(false);
                            if let Some(view) = web_view.as_ref().and_then(|view| view.upgrade()) {
                                view.widget().grab_focus();
                            }
                            return glib::Propagation::Stop;
                        }
                        glib::Propagation::Proceed
                    });
                }
                location.add_controller(omnibar_keys);

                let profile_selector = gtk::ComboBoxText::new();
                profile_selector.set_tooltip_text(Some("Browser Profile"));
                profile_selector.set_focusable(false);
                let profile_choices = Rc::new(RefCell::new(Vec::new()));
                let profile_change_suppressed = Rc::new(Cell::new(false));
                let profile_select_app_state = Arc::clone(app_state);
                let profile_select_surface_id = state.surface_id.clone();
                let profile_change_suppressed_for_signal = Rc::clone(&profile_change_suppressed);
                profile_selector.connect_changed(move |selector| {
                    if profile_change_suppressed_for_signal.get() {
                        return;
                    }
                    let Some(profile_id) = selector.active_id() else {
                        return;
                    };
                    call_app(
                        &profile_select_app_state,
                        "browser.profiles.select",
                        json!({
                            "profile": profile_id.as_str(),
                            "surface_id": profile_select_surface_id
                        }),
                    );
                });

                let browser_tools = gtk::MenuButton::new();
                browser_tools.add_css_class("cmux-action");
                browser_tools.add_css_class("cmux-icon-action");
                browser_tools.set_icon_name("view-more-symbolic");
                browser_tools.set_tooltip_text(Some("Browser Options"));
                browser_tools.set_focusable(false);
                let browser_tools_popover = gtk::Popover::new();
                let browser_tools_content = gtk::Box::new(gtk::Orientation::Vertical, 8);
                browser_tools_content.add_css_class("cmux-browser-tools");
                browser_tools_content.set_margin_top(10);
                browser_tools_content.set_margin_bottom(10);
                browser_tools_content.set_margin_start(10);
                browser_tools_content.set_margin_end(10);

                let forward_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let forward_label = label("Forward", "cmux-browser-tools-label");
                forward_label.set_hexpand(true);
                forward_label.set_xalign(0.0);
                forward_row.append(&forward_label);
                forward_row.append(&forward);
                browser_tools_content.append(&forward_row);

                let profile_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let profile_label = label("Profile", "cmux-browser-tools-label");
                profile_label.set_hexpand(true);
                profile_label.set_xalign(0.0);
                profile_row.append(&profile_label);
                profile_row.append(&profile_selector);
                browser_tools_content.append(&profile_row);

                let profile_form = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let profile_name = gtk::Entry::new();
                profile_name.set_placeholder_text(Some("Profile name"));
                profile_name.set_width_chars(18);
                let profile_submit = browser_icon_button("list-add-symbolic", "Create Profile");
                profile_form.append(&profile_name);
                profile_form.append(&profile_submit);
                browser_tools_content.append(&profile_form);
                let create_profile = {
                    let app_state = Arc::clone(app_state);
                    let surface_id = state.surface_id.clone();
                    let profile_name = profile_name.downgrade();
                    let browser_tools_popover = browser_tools_popover.downgrade();
                    Rc::new(move || {
                        let Some(profile_name) = profile_name.upgrade() else {
                            return;
                        };
                        let name = profile_name.text().trim().to_string();
                        if name.is_empty() {
                            profile_name.grab_focus();
                            return;
                        }
                        let Some(result) = call_app_value(
                            &app_state,
                            "browser.profiles.create",
                            json!({"name": name}),
                        ) else {
                            return;
                        };
                        let Some(profile_id) = result
                            .get("profile")
                            .and_then(|profile| profile.get("id"))
                            .and_then(Value::as_str)
                        else {
                            return;
                        };
                        call_app(
                            &app_state,
                            "browser.profiles.select",
                            json!({
                                "profile": profile_id,
                                "surface_id": surface_id
                            }),
                        );
                        profile_name.set_text("");
                        if let Some(popover) = browser_tools_popover.upgrade() {
                            popover.popdown();
                        }
                    })
                };
                let create_profile_for_entry = Rc::clone(&create_profile);
                profile_name.connect_activate(move |_| create_profile_for_entry());
                profile_submit.connect_clicked(move |_| create_profile());

                let focus_mode = gtk::ToggleButton::builder()
                    .child(&gtk::Image::from_icon_name("view-fullscreen-symbolic"))
                    .build();
                focus_mode.add_css_class("cmux-action");
                focus_mode.add_css_class("cmux-icon-action");
                focus_mode.set_focusable(false);
                focus_mode.set_tooltip_text(Some("Enter Browser Focus Mode"));
                let focus_mode_active = Rc::new(Cell::new(state.focus_mode_active));
                let focus_mode_app_state = Arc::clone(app_state);
                let focus_mode_surface_id = state.surface_id.clone();
                let focus_mode_web_view = web_view.as_ref().map(|view| view.downgrade());
                let focus_mode_active_for_click = Rc::clone(&focus_mode_active);
                focus_mode.connect_clicked(move |button| {
                    let result = call_app_value(
                        &focus_mode_app_state,
                        "browser.focus_mode.set",
                        json!({"surface_id": focus_mode_surface_id, "mode": "toggle"}),
                    );
                    let active = result
                        .as_ref()
                        .and_then(|value| value.get("focus_mode_active"))
                        .and_then(Value::as_bool)
                        .unwrap_or_else(|| !focus_mode_active_for_click.get());
                    focus_mode_active_for_click.set(active);
                    button.set_active(active);
                    button.set_tooltip_text(Some(if active {
                        "Exit Browser Focus Mode"
                    } else {
                        "Enter Browser Focus Mode"
                    }));
                    if active {
                        if let Some(view) =
                            focus_mode_web_view.as_ref().and_then(|view| view.upgrade())
                        {
                            view.widget().grab_focus();
                        }
                    }
                });
                let focus_mode_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let focus_mode_label = label("Browser focus mode", "cmux-browser-tools-label");
                focus_mode_label.set_hexpand(true);
                focus_mode_label.set_xalign(0.0);
                focus_mode_row.append(&focus_mode_label);
                focus_mode_row.append(&focus_mode);
                browser_tools_content.append(&focus_mode_row);
                browser_tools_popover.set_child(Some(&browser_tools_content));
                browser_tools.set_popover(Some(&browser_tools_popover));
                root.append(&browser_tools);
                root.append(&location_viewport);

                let find_bar = gtk::Box::new(gtk::Orientation::Horizontal, 5);
                find_bar.add_css_class("cmux-browser-find-bar");
                find_bar.set_visible(false);
                let find_entry = gtk::SearchEntry::new();
                find_entry.add_css_class("cmux-browser-find");
                find_entry.set_hexpand(true);
                find_entry.set_placeholder_text(Some("Find in page"));
                let find_web_view = web_view.as_ref().map(|view| view.downgrade());
                find_entry.connect_search_changed(move |entry| {
                    if let Some(view) = find_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.find_text(entry.text().as_str());
                    }
                });
                find_bar.append(&find_entry);

                let find_previous = browser_icon_button("go-up-symbolic", "Previous Match");
                let previous_web_view = web_view.as_ref().map(|view| view.downgrade());
                find_previous.connect_clicked(move |_| {
                    if let Some(view) = previous_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.find_previous();
                    }
                });
                find_bar.append(&find_previous);

                let find_next = browser_icon_button("go-down-symbolic", "Next Match");
                let next_web_view = web_view.as_ref().map(|view| view.downgrade());
                find_next.connect_clicked(move |_| {
                    if let Some(view) = next_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.find_next();
                    }
                });
                find_bar.append(&find_next);

                let find_close = browser_icon_button("window-close-symbolic", "Close Find");
                let close_find_bar = find_bar.downgrade();
                let close_web_view = web_view.as_ref().map(|view| view.downgrade());
                let close_browser_chrome = Rc::clone(&browser_chrome_focused);
                find_close.connect_clicked(move |_| {
                    let Some(close_find_bar) = close_find_bar.upgrade() else {
                        return;
                    };
                    close_find_bar.set_visible(false);
                    close_browser_chrome.set(false);
                    if let Some(view) = close_web_view.as_ref().and_then(|view| view.upgrade()) {
                        view.finish_find();
                        view.widget().grab_focus();
                    }
                });
                find_bar.append(&find_close);

                let find_keys = gtk::EventControllerKey::new();
                find_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
                let key_find_bar = find_bar.downgrade();
                let key_web_view = web_view.as_ref().map(|view| view.downgrade());
                let key_browser_chrome = Rc::clone(&browser_chrome_focused);
                find_keys.connect_key_pressed(move |_, keyval, _, modifiers| {
                    let Some(view) = key_web_view.as_ref().and_then(|view| view.upgrade()) else {
                        return glib::Propagation::Proceed;
                    };
                    if keyval == gdk::Key::Escape {
                        if let Some(key_find_bar) = key_find_bar.upgrade() {
                            key_find_bar.set_visible(false);
                        }
                        key_browser_chrome.set(false);
                        view.finish_find();
                        view.widget().grab_focus();
                        return glib::Propagation::Stop;
                    }
                    if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter) {
                        if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
                            view.find_previous();
                        } else {
                            view.find_next();
                        }
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                });
                find_entry.add_controller(find_keys);

                BrowserSurfaceControls {
                    root,
                    find_bar,
                    find_entry,
                    profile_id: state.profile_id.clone(),
                    profile_data_generation: state.profile_data_generation,
                    profile_selector,
                    profile_choices,
                    profile_change_suppressed,
                    location,
                    back,
                    forward,
                    focus_mode,
                    focus_mode_active,
                    web_view,
                    model_url,
                    global_search_needle,
                    page_zoom,
                    user_agent,
                    applied_offline,
                    request_configuration_generation,
                    applied_init_scripts,
                    applied_storage,
                    developer_tools_visible,
                    last_runtime_action_sequence,
                    model_focused,
                    browser_chrome_focused,
                }
            })
            .clone()
    };
    if let Some(profile_payload) = call_app_value(app_state, "browser.profiles.list", json!({})) {
        let choices = browser_profile_choices(&profile_payload);
        if controls.profile_choices.borrow().as_slice() != choices.as_slice() {
            controls.profile_change_suppressed.set(true);
            controls.profile_selector.remove_all();
            for (id, name) in &choices {
                controls.profile_selector.append(Some(id), name);
            }
            controls
                .profile_selector
                .set_active_id(Some(&state.profile_id));
            controls.profile_change_suppressed.set(false);
            controls.profile_choices.replace(choices);
        } else if controls.profile_selector.active_id().as_deref()
            != Some(state.profile_id.as_str())
        {
            controls.profile_change_suppressed.set(true);
            controls
                .profile_selector
                .set_active_id(Some(&state.profile_id));
            controls.profile_change_suppressed.set(false);
        }
    }
    controls.model_url.replace(state.url.clone());
    controls.back.set_sensitive(
        state.can_go_back
            || controls
                .web_view
                .as_ref()
                .is_some_and(crate::gtk_webkit::GtkWebKitView::can_go_back),
    );
    controls.forward.set_sensitive(
        state.can_go_forward
            || controls
                .web_view
                .as_ref()
                .is_some_and(crate::gtk_webkit::GtkWebKitView::can_go_forward),
    );
    let display_url = browser_omnibar_display_text(&state.url);
    if !controls.location.has_focus() && controls.location.text().as_str() != display_url {
        controls.location.set_text(display_url);
    }
    if let Some(view) = &controls.web_view {
        if controls.request_configuration_generation.get() != state.request_configuration_generation
        {
            let request_configuration = app_state
                .lock()
                .ok()
                .and_then(|app| app.browser_native_request_configuration(&state.surface_id));
            if let Some((headers, credentials)) = request_configuration {
                let credentials = credentials
                    .as_ref()
                    .map(|(username, password)| (username.as_str(), password.as_str()));
                if let Err(err) = view.set_request_configuration(&headers, credentials) {
                    eprintln!("cmux: native WebKit request configuration update failed: {err}");
                } else {
                    controls
                        .request_configuration_generation
                        .set(state.request_configuration_generation);
                }
            }
        }
        if controls.applied_offline.get() != Some(state.environment.offline) {
            if let Err(err) = view.set_offline(state.environment.offline) {
                eprintln!("cmux: native WebKit offline update failed: {err}");
            } else {
                controls
                    .applied_offline
                    .set(Some(state.environment.offline));
            }
        }
        if controls.user_agent.borrow().as_str() != state.user_agent {
            if let Err(err) = view.set_user_agent(&state.user_agent) {
                eprintln!("cmux: native WebKit user-agent update failed: {err}");
            } else {
                controls.user_agent.replace(state.user_agent.clone());
            }
        }
        match state.environment.bootstrap_script() {
            Ok(environment_script) => {
                let mut desired_scripts = Vec::with_capacity(state.init_scripts.len() + 1);
                desired_scripts.push(environment_script.clone());
                desired_scripts.extend(state.init_scripts.iter().cloned());
                if controls.applied_init_scripts.borrow().as_slice() != desired_scripts.as_slice() {
                    match view.replace_init_scripts(&desired_scripts) {
                        Ok(()) => {
                            view.evaluate_javascript(&environment_script);
                            controls.applied_init_scripts.replace(desired_scripts);
                        }
                        Err(err) => {
                            eprintln!("cmux: native WebKit document-script update failed: {err}");
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!("cmux: native WebKit environment serialization failed: {err}");
            }
        }
        if view.uri().as_deref() != Some(state.url.as_str()) {
            let _ = view.load_uri(&state.url);
        }
        if (controls.page_zoom.get() - state.page_zoom).abs() > f64::EPSILON {
            view.set_zoom_level(state.page_zoom);
            controls.page_zoom.set(state.page_zoom);
        }
        if controls.developer_tools_visible.get() != state.developer_tools_visible {
            view.set_inspector_visible(state.developer_tools_visible);
            controls
                .developer_tools_visible
                .set(state.developer_tools_visible);
        }
        if !view.is_loading() {
            if state.storage.generation > 0 {
                let document_uri = view.uri().unwrap_or_else(|| state.url.clone());
                let storage_key = (document_uri, state.storage.generation);
                if controls.applied_storage.borrow().as_ref() != Some(&storage_key) {
                    match view.replace_storage(&state.storage.local, &state.storage.session) {
                        Ok(()) => {
                            controls.applied_storage.replace(Some(storage_key));
                        }
                        Err(err) => {
                            eprintln!("cmux: native WebKit storage synchronization failed: {err}");
                        }
                    }
                }
            }
            let mut applied_sequence = controls.last_runtime_action_sequence.get();
            for action in &state.runtime_actions {
                if action.sequence <= applied_sequence {
                    continue;
                }
                let upload_ready = action.upload.as_ref().is_none_or(|upload| {
                    if let Err(err) = view.prepare_file_selection(&upload.files) {
                        eprintln!("cmux: native WebKit file selection failed: {err}");
                        false
                    } else {
                        true
                    }
                });
                if upload_ready && !action.script.is_empty() {
                    view.evaluate_javascript(&action.script);
                }
                if let Some(cookie) = &action.cookie {
                    let result = match cookie.operation.as_str() {
                        "set" => {
                            let name = cookie.name.as_deref().unwrap_or_default();
                            view.set_cookie(
                                &cookie.url,
                                name,
                                cookie.value.as_deref().unwrap_or_default(),
                                cookie.domain.as_deref(),
                                cookie.path.as_deref(),
                                cookie.max_age,
                            )
                        }
                        "get" => {
                            let app_state = Arc::clone(app_state);
                            let surface_id = state.surface_id.clone();
                            view.get_cookies(&cookie.url, move |result| match result {
                                Ok(cookies) => {
                                    let cookies = cookies
                                        .into_iter()
                                        .map(|cookie| {
                                            json!({"name": cookie.name, "value": cookie.value})
                                        })
                                        .collect::<Vec<_>>();
                                    call_app(
                                        &app_state,
                                        "browser.runtime.sync",
                                        json!({"surface_id": surface_id, "cookies": cookies}),
                                    );
                                }
                                Err(err) => {
                                    eprintln!("cmux: native WebKit cookie read failed: {err}");
                                }
                            })
                        }
                        "clear" => view.clear_cookies(&cookie.url, cookie.name.as_deref()),
                        operation => Err(format!("unsupported WebKit cookie action {operation}")),
                    };
                    if let Err(err) = result {
                        eprintln!("cmux: native WebKit cookie action failed: {err}");
                    }
                }
                if action.focus_webview {
                    view.widget().grab_focus();
                }
                applied_sequence = action.sequence;
            }
            controls.last_runtime_action_sequence.set(applied_sequence);
        }
    }
    let next_needle = global_search_needle.map(ToString::to_string);
    if controls.global_search_needle.borrow().as_ref() != next_needle.as_ref() {
        controls.global_search_needle.replace(next_needle.clone());
        if let (Some(view), Some(needle)) = (&controls.web_view, next_needle.as_deref()) {
            view.evaluate_javascript(&browser_global_search_script(needle));
        }
    }
    let was_focus_mode_active = controls.focus_mode_active.replace(state.focus_mode_active);
    controls.focus_mode.set_active(state.focus_mode_active);
    controls
        .focus_mode
        .set_tooltip_text(Some(if state.focus_mode_active {
            "Exit Browser Focus Mode"
        } else {
            "Enter Browser Focus Mode"
        }));
    if state.focus_mode_active && !was_focus_mode_active {
        if let Some(view) = &controls.web_view {
            view.widget().grab_focus();
        }
    }
    if !state.focused {
        controls.browser_chrome_focused.set(false);
    }
    let became_focused = state.focused && !controls.model_focused.replace(state.focused);
    if became_focused {
        if let Some(view) = controls.web_view.as_ref() {
            focus_browser_widget(
                view.widget().clone(),
                Rc::clone(&controls.model_focused),
                Rc::clone(&controls.browser_chrome_focused),
            );
        }
    }
    controls
}

fn focus_browser_widget(
    widget: gtk::Widget,
    still_focused: Rc<Cell<bool>>,
    browser_chrome_focused: Rc<Cell<bool>>,
) {
    let immediate = widget.clone();
    let immediate_focus = Rc::clone(&still_focused);
    let immediate_chrome_focus = Rc::clone(&browser_chrome_focused);
    glib::idle_add_local_once(move || {
        if immediate_focus.get() && !immediate_chrome_focus.get() {
            immediate.grab_focus();
        }
    });
    let attempts = Rc::new(Cell::new(0_u8));
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if !still_focused.get() || browser_chrome_focused.get() || widget.has_focus() {
            return glib::ControlFlow::Break;
        }
        let attempt = attempts.get().saturating_add(1);
        attempts.set(attempt);
        widget.grab_focus();
        if attempt >= BROWSER_FOCUS_RETRY_ATTEMPTS {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn terminal_search_state(view: &Value) -> Option<GtkTerminalSearchState> {
    let search = view.get("terminal_search")?.as_object()?;
    if search.get("active").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(GtkTerminalSearchState {
        surface_id: surface_id_or_ref(view)?,
        query: search
            .get("query")
            .or_else(|| search.get("needle"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        total: search.get("total").and_then(Value::as_u64),
        selected: search.get("selected").and_then(Value::as_u64),
    })
}

fn terminal_search_count_text(state: &GtkTerminalSearchState) -> String {
    let Some(total) = state.total else {
        return "Searching...".to_string();
    };
    let selected = state
        .selected
        .map(|selected| selected.saturating_add(1).min(total))
        .unwrap_or(0);
    format!("{selected}/{total}")
}

fn sync_terminal_search_controls(snapshot: &Value, cache: &TerminalSearchControlsCache) {
    let states = snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(terminal_search_state)
        .map(|state| (state.surface_id.clone(), state))
        .collect::<HashMap<_, _>>();
    let mut controls = cache.borrow_mut();
    controls.retain(|surface_id, _| states.contains_key(surface_id));
    for (surface_id, controls) in controls.iter() {
        let Some(state) = states.get(surface_id) else {
            continue;
        };
        if !widget_contains_focus(&controls.entry) && controls.entry.text().as_str() != state.query
        {
            controls.entry.set_text(&state.query);
        }
        controls.count.set_text(&terminal_search_count_text(state));
    }
}

fn update_terminal_search_query(app_state: &Arc<Mutex<AppState>>, surface_id: &str, query: &str) {
    if let Ok(mut app) = app_state.lock() {
        let _ = app.start_embedded_terminal_search(surface_id, query);
    }
}

fn ensure_terminal_search_controls(
    state: &GtkTerminalSearchState,
    app_state: &Arc<Mutex<AppState>>,
    ghostty: &crate::gtk_ghostty::GhosttySurfaceWidget,
    cache: &TerminalSearchControlsCache,
) -> TerminalSearchControls {
    let controls = {
        let mut controls = cache.borrow_mut();
        controls
            .entry(state.surface_id.clone())
            .or_insert_with(|| {
                let root = gtk::Box::new(gtk::Orientation::Horizontal, 5);
                root.add_css_class("cmux-terminal-search-bar");

                let entry = gtk::SearchEntry::new();
                entry.add_css_class("cmux-terminal-search");
                entry.set_hexpand(true);
                entry.set_placeholder_text(Some("Find in terminal"));
                entry.set_text(&state.query);

                let search_ghostty = ghostty.clone();
                let search_app_state = Arc::clone(app_state);
                let search_surface_id = state.surface_id.clone();
                entry.connect_search_changed(move |entry| {
                    let query = entry.text().to_string();
                    let _ = search_ghostty.perform_binding_action(&format!("search:{query}"));
                    update_terminal_search_query(&search_app_state, &search_surface_id, &query);
                });
                root.append(&entry);

                let count = label(
                    &terminal_search_count_text(state),
                    "cmux-terminal-search-count",
                );
                count.set_xalign(0.5);
                root.append(&count);

                let previous = browser_icon_button("go-up-symbolic", "Previous Match");
                let previous_ghostty = ghostty.clone();
                previous.connect_clicked(move |_| {
                    let _ = previous_ghostty.perform_binding_action("navigate_search:previous");
                });
                root.append(&previous);

                let next = browser_icon_button("go-down-symbolic", "Next Match");
                let next_ghostty = ghostty.clone();
                next.connect_clicked(move |_| {
                    let _ = next_ghostty.perform_binding_action("navigate_search:next");
                });
                root.append(&next);

                let close = browser_icon_button("window-close-symbolic", "Close Find");
                let close_ghostty = ghostty.clone();
                close.connect_clicked(move |_| {
                    let _ = close_ghostty.perform_binding_action("end_search");
                    close_ghostty.grab_focus();
                });
                root.append(&close);

                let key = gtk::EventControllerKey::new();
                key.set_propagation_phase(gtk::PropagationPhase::Capture);
                let key_ghostty = ghostty.clone();
                key.connect_key_pressed(move |_, keyval, _, modifiers| {
                    let action = if keyval == gdk::Key::Escape {
                        Some("end_search")
                    } else if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                        Some(if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
                            "navigate_search:previous"
                        } else {
                            "navigate_search:next"
                        })
                    } else {
                        None
                    };
                    let Some(action) = action else {
                        return glib::Propagation::Proceed;
                    };
                    let _ = key_ghostty.perform_binding_action(action);
                    if action == "end_search" {
                        key_ghostty.grab_focus();
                    }
                    glib::Propagation::Stop
                });
                entry.add_controller(key);

                TerminalSearchControls { root, entry, count }
            })
            .clone()
    };
    if !widget_contains_focus(&controls.entry) && controls.entry.text().as_str() != state.query {
        controls.entry.set_text(&state.query);
    }
    controls.count.set_text(&terminal_search_count_text(state));
    controls
}

fn browser_icon_button(icon_name: &'static str, tooltip: &'static str) -> gtk::Button {
    let image = gtk::Image::from_icon_name(icon_name);
    let button = gtk::Button::builder().child(&image).build();
    button.add_css_class("cmux-action");
    button.add_css_class("cmux-icon-action");
    button.set_focusable(false);
    button.set_tooltip_text(Some(tooltip));
    button
}

fn ensure_ghostty_surface_widget(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    ghostty_widgets: &GhosttySurfaceWidgets,
    config_reload_generation: u64,
) -> Option<crate::gtk_ghostty::GhosttySurfaceWidget> {
    let cache_key = ghostty_surface_cache_key(view)?;
    let options = ghostty_surface_options(view, app_state, config_reload_generation);
    let ghostty = {
        let mut widgets = ghostty_widgets.borrow_mut();
        widgets
            .entry(cache_key)
            .or_insert_with(|| crate::gtk_ghostty::ghostty_surface_widget(options.clone()))
            .clone()
    };
    ghostty.update_options(options);
    Some(ghostty)
}

fn sync_cached_ghostty_presentation(
    ghostty_widgets: &GhosttySurfaceWidgets,
    views: &[Value],
    canvas_occlusion_states: &GtkCanvasOcclusionStates,
) {
    let active = views
        .iter()
        .filter_map(|view| {
            Some((
                ghostty_surface_cache_key(view)?,
                value_bool(view, "focused"),
            ))
        })
        .collect::<HashMap<_, _>>();
    for (surface_id, widget) in ghostty_widgets.borrow().iter() {
        let focused = active.get(surface_id).copied().unwrap_or(false);
        let canvas_occluded = canvas_occlusion_states
            .borrow()
            .get(surface_id)
            .copied()
            .unwrap_or(false);
        widget.update_presentation(focused, !active.contains_key(surface_id) || canvas_occluded);
    }
}

fn sync_ghostty_surface_widgets(
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
    ghostty_widgets: &GhosttySurfaceWidgets,
    canvas_occlusion_states: &GtkCanvasOcclusionStates,
    renderer_mode: GtkRendererMode,
) {
    if renderer_mode != GtkRendererMode::Ghostty {
        return;
    }
    let views = snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let surfaces = snapshot
        .get("window_surfaces")
        .or_else(|| snapshot.get("surfaces"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    prune_ghostty_surface_widgets(ghostty_widgets, &surfaces);
    let config_reload_generation = config_reload_generation(snapshot);
    for view in &views {
        let _ = ensure_ghostty_surface_widget(
            view,
            app_state,
            ghostty_widgets,
            config_reload_generation,
        );
    }
    sync_cached_ghostty_presentation(ghostty_widgets, &views, canvas_occlusion_states);
}

fn detach_widget<W: IsA<gtk::Widget>>(widget: &W) {
    let widget = widget.as_ref();
    let Some(parent) = widget.parent() else {
        return;
    };
    if let Ok(parent_box) = parent.clone().downcast::<gtk::Box>() {
        parent_box.remove(widget);
    } else if let Ok(parent_grid) = parent.downcast::<gtk::Grid>() {
        parent_grid.remove(widget);
    } else {
        widget.unparent();
    }
}

fn valid_hex_color(color: &str) -> bool {
    let Some(rest) = color.strip_prefix('#') else {
        return false;
    };
    matches!(rest.len(), 6 | 8) && rest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn surface_area(
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
    local_refresh: &GtkLocalRefresh,
) -> gtk::Box {
    let main = gtk::Box::new(
        gtk::Orientation::Vertical,
        if ui_mode.is_next() { 0 } else { 10 },
    );
    main.add_css_class("cmux-main");
    main.set_hexpand(true);
    main.set_vexpand(true);

    if !ui_mode.is_next() {
        let focused = snapshot.get("focused").unwrap_or(&Value::Null);
        let title = format!(
            "{} · {} · {}",
            value_str(focused, "workspace_ref", "workspace:-"),
            value_str(focused, "pane_ref", "pane:-"),
            value_str(focused, "surface_ref", "surface:-")
        );
        main.append(&label(&title, "cmux-heading"));
        main.append(&toolbar(snapshot, app_state));
        if let Some(palette) = command_palette_panel(snapshot) {
            main.append(&palette);
        }
        if let Some(shortcuts) = shortcut_help_panel(snapshot, None) {
            main.append(&shortcuts);
        }
    }

    let views = snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let surfaces = snapshot
        .get("window_surfaces")
        .or_else(|| snapshot.get("surfaces"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let canvas_layout = gtk_canvas_layout(snapshot, &views);
    if canvas_layout.is_none() {
        canvas_occlusion_states.borrow_mut().clear();
        for controls in browser_controls.borrow().values() {
            controls.root.set_visible(true);
        }
    }
    let config_reload_generation = config_reload_generation(snapshot);
    if renderer_mode == GtkRendererMode::Ghostty {
        prune_ghostty_surface_widgets(ghostty_widgets, &surfaces);
        sync_cached_ghostty_presentation(ghostty_widgets, &views, canvas_occlusion_states);
    }
    prune_browser_surface_controls(browser_controls, &surfaces);
    prune_diff_surface_controls(diff_controls, &surfaces);
    sync_terminal_search_controls(snapshot, terminal_search_controls);
    prune_terminal_text_box_controls(terminal_text_box_controls, &surfaces);
    let split_layout = canvas_layout
        .is_none()
        .then(|| surface_split_layout(&views))
        .flatten();
    let mut cards = HashMap::new();
    for (view_index, view) in views.iter().enumerate() {
        if !value_bool_or(view, "visible", true) {
            if renderer_mode == GtkRendererMode::Ghostty {
                if let Some(ghostty) = ensure_ghostty_surface_widget(
                    view,
                    app_state,
                    ghostty_widgets,
                    config_reload_generation,
                ) {
                    detach_widget(ghostty.root());
                }
            }
            continue;
        }
        let card = surface_card(
            view,
            app_state,
            pane_allocations,
            ghostty_widgets,
            browser_controls,
            diff_controls,
            terminal_search_controls,
            terminal_text_box_controls,
            renderer_mode,
            config_reload_generation,
            ui_mode,
            local_refresh,
        );
        cards.insert(view_index, card);
    }
    if renderer_mode == GtkRendererMode::Ghostty {
        sync_cached_ghostty_presentation(ghostty_widgets, &views, canvas_occlusion_states);
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_width(1)
        .min_content_height(1)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .build();
    let minimap = canvas_layout.as_ref().map(|layout| {
        let state = canvas_minimap_state(canvas_minimap_states, &layout.workspace_target);
        canvas_minimap_widget(&scroll, layout, state, app_state)
    });
    let content = if let Some(layout) = canvas_layout.as_ref() {
        build_canvas_layout_widget(layout, &mut cards, app_state, minimap.as_ref())
            .upcast::<gtk::Widget>()
    } else {
        split_layout
            .as_ref()
            .and_then(|layout| {
                let workspace_id = snapshot
                    .get("focused")
                    .and_then(|focused| focused.get("workspace_id"))
                    .and_then(Value::as_str)?;
                build_split_layout_widget(layout, &mut cards, app_state, workspace_id)
            })
            .unwrap_or_else(|| fallback_surface_grid(cards).upcast())
    };
    scroll.set_child(Some(&content));
    if let Some(layout) = canvas_layout.as_ref() {
        configure_canvas_viewport(
            &scroll,
            layout,
            app_state,
            minimap.as_ref(),
            ghostty_widgets,
            browser_controls,
            canvas_occlusion_states,
        );
    }
    if let Some(minimap) = minimap {
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scroll));
        overlay.add_overlay(&minimap.area);
        if let Some(layout) = canvas_layout.as_ref() {
            attach_canvas_pointer_navigation(&overlay, &scroll, layout, app_state, Some(&minimap));
        }
        main.append(&overlay);
    } else {
        main.append(&scroll);
    }
    main
}

fn gtk_canvas_layout(snapshot: &Value, views: &[Value]) -> Option<GtkCanvasLayout> {
    let canvas = snapshot.get("canvas")?;
    if canvas.get("mode").and_then(Value::as_str) != Some("canvas") {
        return None;
    }
    let workspace_target = canvas
        .get("workspace_ref")
        .or_else(|| canvas.get("workspace_id"))
        .and_then(Value::as_str)?
        .to_string();
    let scale = canvas
        .get("magnification")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
        .clamp(0.1, 8.0);
    let default_metrics = GtkCanvasMetrics::default();
    let metrics = GtkCanvasMetrics {
        gap: canvas
            .pointer("/metrics/pane_gap")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(default_metrics.gap)
            .clamp(0.0, 64.0),
        snap_threshold: canvas
            .pointer("/metrics/snap_threshold")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(default_metrics.snap_threshold)
            .max(0.0),
        min_width: canvas
            .pointer("/metrics/min_pane_width")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(default_metrics.min_width),
        min_height: canvas
            .pointer("/metrics/min_pane_height")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(default_metrics.min_height),
        snapping_enabled: canvas
            .pointer("/metrics/snapping_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(default_metrics.snapping_enabled),
    };
    let center_x = canvas
        .pointer("/viewport_center/x")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(400.0);
    let center_y = canvas
        .pointer("/viewport_center/y")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(300.0);
    let pane_frames = canvas
        .get("panes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|pane| {
            let pane_id = pane.get("pane_id").and_then(Value::as_str)?;
            let x = finite_frame_number(pane, "x")?;
            let y = finite_frame_number(pane, "y")?;
            let width = finite_frame_number(pane, "width")?;
            let height = finite_frame_number(pane, "height")?;
            let focused = pane
                .get("focused")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let z_index = pane
                .get("z_index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0);
            (width > 0.0 && height > 0.0)
                .then_some((pane_id, (x, y, width, height, focused, z_index)))
        })
        .collect::<HashMap<_, _>>();
    let mut raw = views
        .iter()
        .enumerate()
        .filter(|(_, view)| value_bool_or(view, "visible", true))
        .filter_map(|(view_index, view)| {
            let pane_id = view.get("pane_id").and_then(Value::as_str)?;
            let (x, y, width, height, focused, z_index) = *pane_frames.get(pane_id)?;
            let surface_target = surface_id_or_ref(view)?;
            Some((
                view_index,
                pane_id.to_string(),
                surface_target,
                focused,
                z_index,
                x,
                y,
                width,
                height,
            ))
        })
        .collect::<Vec<_>>();
    raw.sort_by_key(|(view_index, _, _, _, z_index, _, _, _, _)| (*z_index, *view_index));
    if raw.is_empty() {
        return None;
    }

    let logical_half_width = 600.0 / scale;
    let logical_half_height = 400.0 / scale;
    let min_x = raw
        .iter()
        .map(|(_, _, _, _, _, x, _, _, _)| *x)
        .fold(center_x - logical_half_width, f64::min);
    let min_y = raw
        .iter()
        .map(|(_, _, _, _, _, _, y, _, _)| *y)
        .fold(center_y - logical_half_height, f64::min);
    let max_x = raw
        .iter()
        .map(|(_, _, _, _, _, x, _, width, _)| x + width)
        .fold(center_x + logical_half_width, f64::max);
    let max_y = raw
        .iter()
        .map(|(_, _, _, _, _, _, y, _, height)| y + height)
        .fold(center_y + logical_half_height, f64::max);
    let padding = 48.0;
    let placements = raw
        .into_iter()
        .map(
            |(view_index, pane_id, surface_target, focused, _, x, y, width, height)| {
                GtkCanvasPlacement {
                    view_index,
                    pane_id,
                    surface_target,
                    focused,
                    logical_x: x,
                    logical_y: y,
                    logical_width: width,
                    logical_height: height,
                    scale,
                    x: (x - min_x) * scale + padding,
                    y: (y - min_y) * scale + padding,
                    width: width * scale,
                    height: height * scale,
                }
            },
        )
        .collect();
    Some(GtkCanvasLayout {
        workspace_target,
        scale,
        logical_origin_x: min_x,
        logical_origin_y: min_y,
        padding,
        metrics,
        width: (max_x - min_x) * scale + padding * 2.0,
        height: (max_y - min_y) * scale + padding * 2.0,
        viewport_x: (center_x - min_x) * scale + padding,
        viewport_y: (center_y - min_y) * scale + padding,
        placements,
    })
}

fn build_canvas_layout_widget(
    layout: &GtkCanvasLayout,
    cards: &mut HashMap<usize, gtk::Box>,
    app_state: &Arc<Mutex<AppState>>,
    minimap: Option<&GtkCanvasMinimapControls>,
) -> gtk::Fixed {
    let fixed = gtk::Fixed::new();
    fixed.add_css_class("cmux-canvas");
    fixed.set_overflow(gtk::Overflow::Hidden);
    fixed.set_size_request(
        layout.width.ceil().max(1.0) as i32,
        layout.height.ceil().max(1.0) as i32,
    );
    let guides = Rc::new(RefCell::new(Vec::<GtkCanvasGuide>::new()));
    let guide_overlay = canvas_guide_overlay(layout, Rc::clone(&guides));
    for placement in &layout.placements {
        let Some(card) = cards.remove(&placement.view_index) else {
            continue;
        };
        card.set_hexpand(false);
        card.set_vexpand(false);
        card.set_overflow(gtk::Overflow::Hidden);
        card.set_size_request(
            placement.width.round().max(1.0) as i32,
            placement.height.round().max(1.0) as i32,
        );
        fixed.put(&card, placement.x, placement.y);
        let neighbors = layout
            .placements
            .iter()
            .filter(|candidate| candidate.surface_target != placement.surface_target)
            .map(canvas_placement_frame)
            .collect::<Vec<_>>();
        attach_canvas_pane_gestures(
            &fixed,
            &card,
            placement,
            &neighbors,
            layout.metrics,
            &guides,
            &guide_overlay,
            app_state,
            minimap,
        );
    }
    fixed.put(&guide_overlay, 0.0, 0.0);
    fixed
}

fn canvas_placement_frame(placement: &GtkCanvasPlacement) -> GtkCanvasFrame {
    GtkCanvasFrame {
        x: placement.logical_x,
        y: placement.logical_y,
        width: placement.logical_width,
        height: placement.logical_height,
    }
}

fn canvas_guide_overlay(
    layout: &GtkCanvasLayout,
    guides: Rc<RefCell<Vec<GtkCanvasGuide>>>,
) -> gtk::DrawingArea {
    let overlay = gtk::DrawingArea::new();
    overlay.set_can_target(false);
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.set_size_request(
        layout.width.ceil().max(1.0) as i32,
        layout.height.ceil().max(1.0) as i32,
    );
    let origin_x = layout.logical_origin_x;
    let origin_y = layout.logical_origin_y;
    let padding = layout.padding;
    let scale = layout.scale;
    overlay.set_draw_func(move |_, context, _, _| {
        context.set_source_rgba(0.29, 0.72, 0.88, 0.88);
        context.set_line_width(1.0);
        context.set_dash(&[4.0, 3.0], 0.0);
        for guide in guides.borrow().iter() {
            match guide.axis {
                GtkCanvasGuideAxis::Vertical => {
                    let x = (guide.position - origin_x) * scale + padding;
                    let start = (guide.span_start - origin_y) * scale + padding;
                    let end = (guide.span_end - origin_y) * scale + padding;
                    context.move_to(x, start);
                    context.line_to(x, end);
                }
                GtkCanvasGuideAxis::Horizontal => {
                    let y = (guide.position - origin_y) * scale + padding;
                    let start = (guide.span_start - origin_x) * scale + padding;
                    let end = (guide.span_end - origin_x) * scale + padding;
                    context.move_to(start, y);
                    context.line_to(end, y);
                }
            }
        }
        let _ = context.stroke();
    });
    overlay
}

fn attach_canvas_pane_gestures(
    fixed: &gtk::Fixed,
    card: &gtk::Box,
    placement: &GtkCanvasPlacement,
    neighbors: &[GtkCanvasFrame],
    metrics: GtkCanvasMetrics,
    guides: &Rc<RefCell<Vec<GtkCanvasGuide>>>,
    guide_overlay: &gtk::DrawingArea,
    app_state: &Arc<Mutex<AppState>>,
    minimap: Option<&GtkCanvasMinimapControls>,
) {
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(gdk::BUTTON_PRIMARY);
    let active_region = Rc::new(Cell::new(None::<GtkCanvasDragRegion>));
    let begin_region = Rc::clone(&active_region);
    let rendered_width = placement.width;
    let rendered_height = placement.height;
    let begin_minimap = minimap.cloned();
    gesture.connect_drag_begin(move |gesture, x, y| {
        let region = canvas_drag_region(x, y, rendered_width, rendered_height);
        begin_region.set(region);
        if region.is_none() {
            let _ = gesture.set_state(gtk::EventSequenceState::Denied);
        } else if let Some(minimap) = begin_minimap.as_ref() {
            canvas_minimap_hold(minimap);
        }
    });

    let update_region = Rc::clone(&active_region);
    let update_fixed = fixed.clone();
    let update_card = card.clone();
    let update_placement = placement.clone();
    let update_neighbors = neighbors.to_vec();
    let update_guides = Rc::clone(guides);
    let update_overlay = guide_overlay.clone();
    let update_minimap = minimap.cloned();
    gesture.connect_drag_update(move |gesture, offset_x, offset_y| {
        let Some(region) = update_region.get() else {
            return;
        };
        let proposed = canvas_raw_interaction_frame(&update_placement, region, offset_x, offset_y);
        let snapping = metrics.snapping_enabled
            && !gesture
                .current_event_state()
                .intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK);
        let result = canvas_snap_result(proposed, region, &update_neighbors, metrics, snapping);
        let frame = result.frame;
        let (x, y, width, height) = canvas_rendered_frame(&update_placement, frame);
        update_fixed.move_(&update_card, x, y);
        update_card.set_size_request(width, height);
        *update_guides.borrow_mut() = result.guides;
        update_overlay.queue_draw();
        if let Some(minimap) = update_minimap.as_ref() {
            minimap.area.queue_draw();
        }
    });

    let end_region = Rc::clone(&active_region);
    let end_app_state = Arc::clone(app_state);
    let end_placement = placement.clone();
    let end_neighbors = neighbors.to_vec();
    let end_guides = Rc::clone(guides);
    let end_overlay = guide_overlay.clone();
    let end_minimap = minimap.cloned();
    gesture.connect_drag_end(move |gesture, offset_x, offset_y| {
        if let Some(region) = end_region.replace(None) {
            let proposed = canvas_raw_interaction_frame(&end_placement, region, offset_x, offset_y);
            let snapping = metrics.snapping_enabled
                && !gesture
                    .current_event_state()
                    .intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK);
            let frame =
                canvas_snap_result(proposed, region, &end_neighbors, metrics, snapping).frame;
            end_guides.borrow_mut().clear();
            end_overlay.queue_draw();
            call_app(
                &end_app_state,
                "canvas.set_frame",
                canvas_frame_params(&end_placement.surface_target, frame),
            );
            if let Some(minimap) = end_minimap.as_ref() {
                canvas_minimap_release(minimap);
            }
        }
    });
    card.add_controller(gesture);

    let motion = gtk::EventControllerMotion::new();
    let motion_card = card.clone();
    let motion_width = placement.width;
    let motion_height = placement.height;
    motion.connect_motion(move |_, x, y| {
        motion_card.set_cursor_from_name(
            canvas_drag_region(x, y, motion_width, motion_height).and_then(canvas_drag_cursor_name),
        );
    });
    let leave_card = card.clone();
    motion.connect_leave(move |_| leave_card.set_cursor_from_name(None));
    card.add_controller(motion);
}

fn canvas_drag_region(x: f64, y: f64, width: f64, height: f64) -> Option<GtkCanvasDragRegion> {
    const EDGE: f64 = 6.0;
    const CORNER: f64 = 12.0;
    if !x.is_finite() || !y.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let mut edges = GtkCanvasResizeEdges {
        left: x <= EDGE,
        right: x >= width - EDGE,
        top: y <= EDGE,
        bottom: y >= height - EDGE,
    };
    if edges.left || edges.right {
        edges.top |= y <= CORNER;
        edges.bottom |= y >= height - CORNER;
    } else if edges.top || edges.bottom {
        edges.left |= x <= CORNER;
        edges.right |= x >= width - CORNER;
    }
    if edges != GtkCanvasResizeEdges::default() {
        Some(GtkCanvasDragRegion::Resize(edges))
    } else if y <= 96.0 {
        Some(GtkCanvasDragRegion::Move)
    } else {
        None
    }
}

fn canvas_drag_cursor_name(region: GtkCanvasDragRegion) -> Option<&'static str> {
    match region {
        GtkCanvasDragRegion::Move => Some("move"),
        GtkCanvasDragRegion::Resize(edges)
            if (edges.left && edges.top) || (edges.right && edges.bottom) =>
        {
            Some("nwse-resize")
        }
        GtkCanvasDragRegion::Resize(edges)
            if (edges.right && edges.top) || (edges.left && edges.bottom) =>
        {
            Some("nesw-resize")
        }
        GtkCanvasDragRegion::Resize(edges) if edges.left || edges.right => Some("ew-resize"),
        GtkCanvasDragRegion::Resize(edges) if edges.top || edges.bottom => Some("ns-resize"),
        GtkCanvasDragRegion::Resize(_) => None,
    }
}

fn canvas_raw_interaction_frame(
    placement: &GtkCanvasPlacement,
    region: GtkCanvasDragRegion,
    offset_x: f64,
    offset_y: f64,
) -> GtkCanvasFrame {
    let dx = offset_x / placement.scale;
    let dy = offset_y / placement.scale;
    let mut frame = GtkCanvasFrame {
        x: placement.logical_x,
        y: placement.logical_y,
        width: placement.logical_width,
        height: placement.logical_height,
    };
    match region {
        GtkCanvasDragRegion::Move => {
            frame.x += dx;
            frame.y += dy;
        }
        GtkCanvasDragRegion::Resize(edges) => {
            if edges.left {
                let right = frame.x + frame.width;
                frame.width = (right - (frame.x + dx)).max(1.0);
                frame.x = right - frame.width;
            } else if edges.right {
                frame.width = (frame.width + dx).max(1.0);
            }
            if edges.top {
                let bottom = frame.y + frame.height;
                frame.height = (bottom - (frame.y + dy)).max(1.0);
                frame.y = bottom - frame.height;
            } else if edges.bottom {
                frame.height = (frame.height + dy).max(1.0);
            }
        }
    }
    frame
}

fn canvas_snap_result(
    proposed: GtkCanvasFrame,
    region: GtkCanvasDragRegion,
    neighbors: &[GtkCanvasFrame],
    metrics: GtkCanvasMetrics,
    snapping: bool,
) -> GtkCanvasSnapResult {
    let mut frame = proposed;
    let mut guides = Vec::new();
    match region {
        GtkCanvasDragRegion::Move => {
            if snapping {
                if let Some(best) = best_canvas_snap_candidate(
                    canvas_move_candidates_x(proposed, neighbors, metrics.gap),
                    metrics.snap_threshold,
                ) {
                    frame.x += best.delta;
                    guides.push(canvas_vertical_guide(
                        best.guide_position,
                        frame,
                        best.neighbor,
                    ));
                }
                if let Some(best) = best_canvas_snap_candidate(
                    canvas_move_candidates_y(proposed, neighbors, metrics.gap),
                    metrics.snap_threshold,
                ) {
                    frame.y += best.delta;
                    guides.push(canvas_horizontal_guide(
                        best.guide_position,
                        frame,
                        best.neighbor,
                    ));
                }
            }
        }
        GtkCanvasDragRegion::Resize(edges) => {
            if edges.left {
                if snapping {
                    let candidates = canvas_edge_candidates(
                        proposed.x,
                        neighbors.iter().map(|frame| (frame.x, *frame)),
                        neighbors
                            .iter()
                            .map(|frame| (frame.x + frame.width + metrics.gap, *frame)),
                    );
                    if let Some(best) =
                        best_canvas_snap_candidate(candidates, metrics.snap_threshold)
                    {
                        frame.x = proposed.x + best.delta;
                        frame.width = proposed.x + proposed.width - frame.x;
                        guides.push(canvas_vertical_guide(
                            best.guide_position,
                            frame,
                            best.neighbor,
                        ));
                    }
                }
                if frame.width < metrics.min_width {
                    let right = frame.x + frame.width;
                    frame.x = right - metrics.min_width;
                    frame.width = metrics.min_width;
                    guides.retain(|guide| guide.axis != GtkCanvasGuideAxis::Vertical);
                }
            } else if edges.right {
                if snapping {
                    let proposed_right = proposed.x + proposed.width;
                    let candidates = canvas_edge_candidates(
                        proposed_right,
                        neighbors
                            .iter()
                            .map(|frame| (frame.x + frame.width, *frame)),
                        neighbors
                            .iter()
                            .map(|frame| (frame.x - metrics.gap, *frame)),
                    );
                    if let Some(best) =
                        best_canvas_snap_candidate(candidates, metrics.snap_threshold)
                    {
                        frame.width = proposed_right + best.delta - frame.x;
                        guides.push(canvas_vertical_guide(
                            best.guide_position,
                            frame,
                            best.neighbor,
                        ));
                    }
                }
                if frame.width < metrics.min_width {
                    frame.width = metrics.min_width;
                    guides.retain(|guide| guide.axis != GtkCanvasGuideAxis::Vertical);
                }
            }

            if edges.top {
                if snapping {
                    let candidates = canvas_edge_candidates(
                        proposed.y,
                        neighbors.iter().map(|frame| (frame.y, *frame)),
                        neighbors
                            .iter()
                            .map(|frame| (frame.y + frame.height + metrics.gap, *frame)),
                    );
                    if let Some(best) =
                        best_canvas_snap_candidate(candidates, metrics.snap_threshold)
                    {
                        frame.y = proposed.y + best.delta;
                        frame.height = proposed.y + proposed.height - frame.y;
                        guides.push(canvas_horizontal_guide(
                            best.guide_position,
                            frame,
                            best.neighbor,
                        ));
                    }
                }
                if frame.height < metrics.min_height {
                    let bottom = frame.y + frame.height;
                    frame.y = bottom - metrics.min_height;
                    frame.height = metrics.min_height;
                    guides.retain(|guide| guide.axis != GtkCanvasGuideAxis::Horizontal);
                }
            } else if edges.bottom {
                if snapping {
                    let proposed_bottom = proposed.y + proposed.height;
                    let candidates = canvas_edge_candidates(
                        proposed_bottom,
                        neighbors
                            .iter()
                            .map(|frame| (frame.y + frame.height, *frame)),
                        neighbors
                            .iter()
                            .map(|frame| (frame.y - metrics.gap, *frame)),
                    );
                    if let Some(best) =
                        best_canvas_snap_candidate(candidates, metrics.snap_threshold)
                    {
                        frame.height = proposed_bottom + best.delta - frame.y;
                        guides.push(canvas_horizontal_guide(
                            best.guide_position,
                            frame,
                            best.neighbor,
                        ));
                    }
                }
                if frame.height < metrics.min_height {
                    frame.height = metrics.min_height;
                    guides.retain(|guide| guide.axis != GtkCanvasGuideAxis::Horizontal);
                }
            }
        }
    }
    GtkCanvasSnapResult { frame, guides }
}

fn best_canvas_snap_candidate(
    candidates: Vec<GtkCanvasSnapCandidate>,
    threshold: f64,
) -> Option<GtkCanvasSnapCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| candidate.delta.abs() <= threshold)
        .fold(None, |best, candidate| match best {
            None => Some(candidate),
            Some(current)
                if candidate.delta.abs() < current.delta.abs()
                    || (candidate.delta.abs() == current.delta.abs()
                        && candidate.priority < current.priority) =>
            {
                Some(candidate)
            }
            Some(current) => Some(current),
        })
}

fn canvas_move_candidates_x(
    frame: GtkCanvasFrame,
    neighbors: &[GtkCanvasFrame],
    gap: f64,
) -> Vec<GtkCanvasSnapCandidate> {
    let mut candidates = Vec::with_capacity(neighbors.len() * 5);
    for neighbor in neighbors {
        for (delta, guide_position, priority) in [
            (neighbor.x - frame.x, neighbor.x, 0),
            (
                neighbor.x + neighbor.width - frame.x - frame.width,
                neighbor.x + neighbor.width,
                0,
            ),
            (
                neighbor.x + neighbor.width + gap - frame.x,
                neighbor.x + neighbor.width + gap,
                1,
            ),
            (
                neighbor.x - gap - frame.x - frame.width,
                neighbor.x - gap,
                1,
            ),
            (
                neighbor.x + neighbor.width / 2.0 - frame.x - frame.width / 2.0,
                neighbor.x + neighbor.width / 2.0,
                2,
            ),
        ] {
            candidates.push(GtkCanvasSnapCandidate {
                delta,
                guide_position,
                priority,
                neighbor: *neighbor,
            });
        }
    }
    candidates
}

fn canvas_move_candidates_y(
    frame: GtkCanvasFrame,
    neighbors: &[GtkCanvasFrame],
    gap: f64,
) -> Vec<GtkCanvasSnapCandidate> {
    let mut candidates = Vec::with_capacity(neighbors.len() * 5);
    for neighbor in neighbors {
        for (delta, guide_position, priority) in [
            (neighbor.y - frame.y, neighbor.y, 0),
            (
                neighbor.y + neighbor.height - frame.y - frame.height,
                neighbor.y + neighbor.height,
                0,
            ),
            (
                neighbor.y + neighbor.height + gap - frame.y,
                neighbor.y + neighbor.height + gap,
                1,
            ),
            (
                neighbor.y - gap - frame.y - frame.height,
                neighbor.y - gap,
                1,
            ),
            (
                neighbor.y + neighbor.height / 2.0 - frame.y - frame.height / 2.0,
                neighbor.y + neighbor.height / 2.0,
                2,
            ),
        ] {
            candidates.push(GtkCanvasSnapCandidate {
                delta,
                guide_position,
                priority,
                neighbor: *neighbor,
            });
        }
    }
    candidates
}

fn canvas_edge_candidates(
    edge: f64,
    align_targets: impl Iterator<Item = (f64, GtkCanvasFrame)>,
    gap_targets: impl Iterator<Item = (f64, GtkCanvasFrame)>,
) -> Vec<GtkCanvasSnapCandidate> {
    align_targets
        .map(|(target, neighbor)| GtkCanvasSnapCandidate {
            delta: target - edge,
            guide_position: target,
            priority: 0,
            neighbor,
        })
        .chain(
            gap_targets.map(|(target, neighbor)| GtkCanvasSnapCandidate {
                delta: target - edge,
                guide_position: target,
                priority: 1,
                neighbor,
            }),
        )
        .collect()
}

fn canvas_vertical_guide(
    position: f64,
    snapped: GtkCanvasFrame,
    neighbor: GtkCanvasFrame,
) -> GtkCanvasGuide {
    GtkCanvasGuide {
        axis: GtkCanvasGuideAxis::Vertical,
        position,
        span_start: snapped.y.min(neighbor.y),
        span_end: (snapped.y + snapped.height).max(neighbor.y + neighbor.height),
    }
}

fn canvas_horizontal_guide(
    position: f64,
    snapped: GtkCanvasFrame,
    neighbor: GtkCanvasFrame,
) -> GtkCanvasGuide {
    GtkCanvasGuide {
        axis: GtkCanvasGuideAxis::Horizontal,
        position,
        span_start: snapped.x.min(neighbor.x),
        span_end: (snapped.x + snapped.width).max(neighbor.x + neighbor.width),
    }
}

fn canvas_rendered_frame(
    placement: &GtkCanvasPlacement,
    frame: GtkCanvasFrame,
) -> (f64, f64, i32, i32) {
    (
        placement.x + (frame.x - placement.logical_x) * placement.scale,
        placement.y + (frame.y - placement.logical_y) * placement.scale,
        (frame.width * placement.scale).round().max(1.0) as i32,
        (frame.height * placement.scale).round().max(1.0) as i32,
    )
}

fn canvas_frame_params(surface_target: &str, frame: GtkCanvasFrame) -> Value {
    json!({
        "surface_id": surface_target,
        "x": frame.x,
        "y": frame.y,
        "width": frame.width,
        "height": frame.height
    })
}

#[cfg(test)]
fn canvas_test_interaction_frame(
    placement: &GtkCanvasPlacement,
    region: GtkCanvasDragRegion,
    offset_x: f64,
    offset_y: f64,
) -> GtkCanvasFrame {
    canvas_snap_result(
        canvas_raw_interaction_frame(placement, region, offset_x, offset_y),
        region,
        &[],
        GtkCanvasMetrics::default(),
        false,
    )
    .frame
}

#[cfg(test)]
fn canvas_test_interaction_frame_params(
    placement: &GtkCanvasPlacement,
    region: GtkCanvasDragRegion,
    offset_x: f64,
    offset_y: f64,
) -> Value {
    canvas_frame_params(
        &placement.surface_target,
        canvas_test_interaction_frame(placement, region, offset_x, offset_y),
    )
}

fn canvas_viewport_center(
    layout: &GtkCanvasLayout,
    horizontal_value: f64,
    vertical_value: f64,
    page_width: f64,
    page_height: f64,
) -> (f64, f64) {
    (
        layout.logical_origin_x
            + (horizontal_value + page_width / 2.0 - layout.padding) / layout.scale,
        layout.logical_origin_y
            + (vertical_value + page_height / 2.0 - layout.padding) / layout.scale,
    )
}

fn canvas_scroll_pixels(delta: f64) -> f64 {
    if delta.is_finite() {
        delta * 48.0
    } else {
        0.0
    }
}

fn canvas_adjustment_after_scroll(
    value: f64,
    delta: f64,
    lower: f64,
    upper: f64,
    page_size: f64,
) -> f64 {
    let maximum = (upper - page_size).max(lower);
    (value + canvas_scroll_pixels(delta)).clamp(lower, maximum)
}

fn canvas_zoom_toward_pointer(
    center: (f64, f64),
    old_zoom: f64,
    new_zoom: f64,
    viewport_size: (f64, f64),
    pointer: (f64, f64),
) -> (f64, f64) {
    if old_zoom <= 0.0 || new_zoom <= 0.0 || !old_zoom.is_finite() || !new_zoom.is_finite() {
        return center;
    }
    let old_left = center.0 - viewport_size.0 / (2.0 * old_zoom);
    let old_top = center.1 - viewport_size.1 / (2.0 * old_zoom);
    let anchor_x = old_left + pointer.0 / old_zoom;
    let anchor_y = old_top + pointer.1 / old_zoom;
    (
        anchor_x - pointer.0 / new_zoom + viewport_size.0 / (2.0 * new_zoom),
        anchor_y - pointer.1 / new_zoom + viewport_size.1 / (2.0 * new_zoom),
    )
}

#[derive(Clone)]
struct GtkCanvasMinimapControls {
    area: gtk::DrawingArea,
    scroll: gtk::ScrolledWindow,
    layout: GtkCanvasLayout,
    state: Rc<RefCell<GtkCanvasMinimapVisibility>>,
    interaction_active: Rc<Cell<bool>>,
}

fn canvas_minimap_state(
    states: &GtkCanvasMinimapStates,
    workspace_target: &str,
) -> Rc<RefCell<GtkCanvasMinimapVisibility>> {
    Rc::clone(
        states
            .borrow_mut()
            .entry(workspace_target.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(GtkCanvasMinimapVisibility::default()))),
    )
}

fn canvas_minimap_snapshot(
    layout: &GtkCanvasLayout,
    horizontal: &gtk::Adjustment,
    vertical: &gtk::Adjustment,
) -> GtkCanvasMinimapSnapshot {
    let mut seen = HashSet::new();
    let panes = layout
        .placements
        .iter()
        .filter(|placement| seen.insert(placement.pane_id.clone()))
        .map(|placement| {
            (
                placement.pane_id.clone(),
                canvas_placement_frame(placement),
                placement.focused,
            )
        })
        .collect();
    let visible = canvas_logical_viewport(
        layout,
        horizontal.value(),
        vertical.value(),
        horizontal.page_size(),
        vertical.page_size(),
    );
    GtkCanvasMinimapSnapshot::new(panes, visible)
}

fn canvas_logical_viewport(
    layout: &GtkCanvasLayout,
    horizontal_value: f64,
    vertical_value: f64,
    page_width: f64,
    page_height: f64,
) -> GtkCanvasFrame {
    GtkCanvasFrame {
        x: layout.logical_origin_x + (horizontal_value - layout.padding) / layout.scale,
        y: layout.logical_origin_y + (vertical_value - layout.padding) / layout.scale,
        width: page_width / layout.scale,
        height: page_height / layout.scale,
    }
}

fn canvas_minimap_widget(
    scroll: &gtk::ScrolledWindow,
    layout: &GtkCanvasLayout,
    state: Rc<RefCell<GtkCanvasMinimapVisibility>>,
    app_state: &Arc<Mutex<AppState>>,
) -> GtkCanvasMinimapControls {
    let area = gtk::DrawingArea::new();
    area.add_css_class("cmux-canvas-minimap");
    area.set_size_request(168, 112);
    area.set_halign(gtk::Align::End);
    area.set_valign(gtk::Align::End);
    area.set_margin_end(14);
    area.set_margin_bottom(14);
    area.set_opacity(0.92);
    area.set_visible(false);
    area.set_tooltip_text(Some("Canvas minimap"));

    let controls = GtkCanvasMinimapControls {
        area: area.clone(),
        scroll: scroll.clone(),
        layout: layout.clone(),
        state,
        interaction_active: Rc::new(Cell::new(false)),
    };

    let draw_scroll = scroll.clone();
    let draw_layout = layout.clone();
    area.set_draw_func(move |_, context, width, height| {
        let snapshot = canvas_minimap_snapshot(
            &draw_layout,
            &draw_scroll.hadjustment(),
            &draw_scroll.vadjustment(),
        );
        if !snapshot.should_show() {
            return;
        }
        let width = width as f64;
        let height = height as f64;
        context.set_source_rgba(0.09, 0.10, 0.11, 0.92);
        cairo_rounded_rectangle(context, 0.5, 0.5, width - 1.0, height - 1.0, 8.0);
        let _ = context.fill_preserve();
        context.set_source_rgba(0.42, 0.46, 0.49, 0.72);
        context.set_line_width(1.0);
        let _ = context.stroke();

        let drawing = GtkCanvasFrame {
            x: 10.0,
            y: 10.0,
            width: (width - 20.0).max(1.0),
            height: (height - 20.0).max(1.0),
        };
        for (_, pane, focused) in &snapshot.panes {
            let pane = canvas_minimap_display_frame(snapshot.minimap_frame(*pane, drawing));
            cairo_rounded_rectangle(context, pane.x, pane.y, pane.width, pane.height, 3.0);
            if *focused {
                context.set_source_rgba(0.29, 0.72, 0.88, 0.36);
                let _ = context.fill_preserve();
                context.set_source_rgba(0.29, 0.72, 0.88, 0.96);
                context.set_line_width(1.5);
            } else {
                context.set_source_rgba(0.88, 0.90, 0.90, 0.20);
                let _ = context.fill_preserve();
                context.set_source_rgba(0.88, 0.90, 0.90, 0.30);
                context.set_line_width(1.0);
            }
            let _ = context.stroke();
        }

        let viewport =
            canvas_minimap_display_frame(snapshot.minimap_frame(snapshot.visible, drawing));
        cairo_rounded_rectangle(
            context,
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            4.0,
        );
        context.set_source_rgba(0.29, 0.72, 0.88, 0.12);
        let _ = context.fill_preserve();
        context.set_source_rgba(0.29, 0.72, 0.88, 0.92);
        context.set_line_width(2.0);
        let _ = context.stroke();
    });

    let pointer_inside = Rc::new(Cell::new(false));
    let dragging = Rc::new(Cell::new(false));
    let motion = gtk::EventControllerMotion::new();
    let enter_controls = controls.clone();
    let enter_pointer = Rc::clone(&pointer_inside);
    motion.connect_enter(move |_, _, _| {
        enter_pointer.set(true);
        canvas_minimap_hold(&enter_controls);
    });
    let leave_controls = controls.clone();
    let leave_pointer = Rc::clone(&pointer_inside);
    let leave_dragging = Rc::clone(&dragging);
    motion.connect_leave(move |_| {
        leave_pointer.set(false);
        if !leave_dragging.get() {
            canvas_minimap_release(&leave_controls);
        }
    });
    area.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    drag.set_button(gdk::BUTTON_PRIMARY);
    let start = Rc::new(Cell::new((0.0, 0.0)));
    let begin_controls = controls.clone();
    let begin_start = Rc::clone(&start);
    let begin_dragging = Rc::clone(&dragging);
    drag.connect_drag_begin(move |_, x, y| {
        begin_start.set((x, y));
        begin_dragging.set(true);
        begin_controls.interaction_active.set(true);
        canvas_minimap_hold(&begin_controls);
        canvas_minimap_recenter(&begin_controls, x, y);
    });
    let update_controls = controls.clone();
    let update_start = Rc::clone(&start);
    drag.connect_drag_update(move |_, offset_x, offset_y| {
        let (start_x, start_y) = update_start.get();
        canvas_minimap_recenter(&update_controls, start_x + offset_x, start_y + offset_y);
    });
    let end_controls = controls.clone();
    let end_start = Rc::clone(&start);
    let end_dragging = Rc::clone(&dragging);
    let end_pointer = Rc::clone(&pointer_inside);
    let end_app_state = Arc::clone(app_state);
    drag.connect_drag_end(move |_, offset_x, offset_y| {
        let (start_x, start_y) = end_start.get();
        let center = canvas_minimap_recenter(&end_controls, start_x + offset_x, start_y + offset_y);
        end_dragging.set(false);
        end_controls.interaction_active.set(false);
        if let Some((x, y)) = center {
            call_app(
                &end_app_state,
                "canvas.set_viewport",
                json!({
                    "workspace_id": end_controls.layout.workspace_target,
                    "x": x,
                    "y": y
                }),
            );
        }
        if end_pointer.get() {
            canvas_minimap_hold(&end_controls);
        } else {
            canvas_minimap_release(&end_controls);
        }
    });
    area.add_controller(drag);

    let map_controls = controls.clone();
    area.connect_map(move |_| canvas_minimap_sync_visibility(&map_controls));
    controls
}

fn cairo_rounded_rectangle(
    context: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let right = x + width;
    let bottom = y + height;
    context.new_sub_path();
    context.arc(
        right - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    context.arc(
        right - radius,
        bottom - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    context.arc(
        x + radius,
        bottom - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    context.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    context.close_path();
}

fn canvas_minimap_display_frame(mut frame: GtkCanvasFrame) -> GtkCanvasFrame {
    if frame.width < 3.0 {
        frame.x -= (3.0 - frame.width) / 2.0;
        frame.width = 3.0;
    }
    if frame.height < 3.0 {
        frame.y -= (3.0 - frame.height) / 2.0;
        frame.height = 3.0;
    }
    frame
}

fn canvas_minimap_recenter(
    controls: &GtkCanvasMinimapControls,
    x: f64,
    y: f64,
) -> Option<(f64, f64)> {
    let horizontal = controls.scroll.hadjustment();
    let vertical = controls.scroll.vadjustment();
    let snapshot = canvas_minimap_snapshot(&controls.layout, &horizontal, &vertical);
    if !snapshot.should_show() {
        return None;
    }
    let drawing = GtkCanvasFrame {
        x: 10.0,
        y: 10.0,
        width: (controls.area.allocated_width() as f64 - 20.0).max(1.0),
        height: (controls.area.allocated_height() as f64 - 20.0).max(1.0),
    };
    let projected = snapshot.projected_navigation_bounds(drawing);
    let x = x.clamp(projected.x, projected.x + projected.width);
    let y = y.clamp(projected.y, projected.y + projected.height);
    let center = snapshot.canvas_point(x, y, drawing);
    for (adjustment, logical_center, logical_origin) in [
        (&horizontal, center.0, controls.layout.logical_origin_x),
        (&vertical, center.1, controls.layout.logical_origin_y),
    ] {
        let target = (logical_center - logical_origin) * controls.layout.scale
            + controls.layout.padding
            - adjustment.page_size() / 2.0;
        let maximum = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
        adjustment.set_value(target.clamp(adjustment.lower(), maximum));
    }
    controls.area.queue_draw();
    Some(center)
}

fn canvas_minimap_hold(controls: &GtkCanvasMinimapControls) {
    let mut state = controls.state.borrow_mut();
    state.held = true;
    state.visible_until = None;
    drop(state);
    canvas_minimap_sync_visibility(controls);
}

fn canvas_minimap_release(controls: &GtkCanvasMinimapControls) {
    {
        let mut state = controls.state.borrow_mut();
        state.held = false;
        state.visible_until = Some(Instant::now() + Duration::from_secs(3));
    }
    canvas_minimap_sync_visibility(controls);
    canvas_minimap_schedule_hide(controls.clone(), Duration::from_secs(3));
}

fn canvas_minimap_reveal(controls: &GtkCanvasMinimapControls) {
    if controls.state.borrow().held {
        canvas_minimap_sync_visibility(controls);
        return;
    }
    canvas_minimap_release(controls);
}

fn canvas_minimap_schedule_hide(controls: GtkCanvasMinimapControls, delay: Duration) {
    glib::timeout_add_local_once(delay, move || {
        let remaining = {
            let mut state = controls.state.borrow_mut();
            if state.held {
                return;
            }
            match state.visible_until {
                Some(deadline) if deadline > Instant::now() => Some(deadline - Instant::now()),
                _ => {
                    state.visible_until = None;
                    None
                }
            }
        };
        if let Some(remaining) = remaining {
            canvas_minimap_schedule_hide(controls, remaining);
        } else {
            canvas_minimap_sync_visibility(&controls);
        }
    });
}

fn canvas_minimap_sync_visibility(controls: &GtkCanvasMinimapControls) {
    let active = {
        let state = controls.state.borrow();
        state.held
            || state
                .visible_until
                .is_some_and(|deadline| deadline > Instant::now())
    };
    let snapshot = canvas_minimap_snapshot(
        &controls.layout,
        &controls.scroll.hadjustment(),
        &controls.scroll.vadjustment(),
    );
    controls.area.set_visible(active && snapshot.should_show());
    if active {
        controls.area.set_opacity(0.92);
        controls.area.queue_draw();
    }
}

fn configure_canvas_viewport(
    scroll: &gtk::ScrolledWindow,
    layout: &GtkCanvasLayout,
    app_state: &Arc<Mutex<AppState>>,
    minimap: Option<&GtkCanvasMinimapControls>,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    canvas_occlusion_states: &GtkCanvasOcclusionStates,
) {
    let enabled = Rc::new(Cell::new(false));
    let pending = Rc::new(Cell::new(false));
    let horizontal = scroll.hadjustment();
    let vertical = scroll.vadjustment();
    for adjustment in [horizontal.clone(), vertical.clone()] {
        let enabled = Rc::clone(&enabled);
        let pending = Rc::clone(&pending);
        let horizontal = horizontal.clone();
        let vertical = vertical.clone();
        let layout = layout.clone();
        let app_state = Arc::clone(app_state);
        let minimap = minimap.cloned();
        let lifecycle_scroll = scroll.clone();
        let lifecycle_layout = layout.clone();
        let lifecycle_ghostty = Rc::clone(ghostty_widgets);
        let lifecycle_browsers = Rc::clone(browser_controls);
        let lifecycle_states = Rc::clone(canvas_occlusion_states);
        adjustment.connect_value_changed(move |_| {
            if !enabled.get() {
                return;
            }
            sync_canvas_viewport_lifecycle(
                &lifecycle_scroll,
                &lifecycle_layout,
                &lifecycle_ghostty,
                &lifecycle_browsers,
                &lifecycle_states,
            );
            if let Some(minimap) = minimap.as_ref() {
                minimap.area.queue_draw();
                canvas_minimap_reveal(minimap);
                if minimap.interaction_active.get() {
                    return;
                }
            }
            if pending.replace(true) {
                return;
            }
            let pending = Rc::clone(&pending);
            let horizontal = horizontal.clone();
            let vertical = vertical.clone();
            let layout = layout.clone();
            let app_state = Arc::clone(&app_state);
            glib::timeout_add_local_once(Duration::from_millis(120), move || {
                pending.set(false);
                let (x, y) = canvas_viewport_center(
                    &layout,
                    horizontal.value(),
                    vertical.value(),
                    horizontal.page_size(),
                    vertical.page_size(),
                );
                if x.is_finite() && y.is_finite() {
                    call_app(
                        &app_state,
                        "canvas.set_viewport",
                        json!({
                            "workspace_id": layout.workspace_target,
                            "x": x,
                            "y": y
                        }),
                    );
                }
            });
        });
    }

    let viewport = (layout.viewport_x, layout.viewport_y);
    let map_enabled = Rc::clone(&enabled);
    let map_minimap = minimap.cloned();
    let map_layout = layout.clone();
    let map_ghostty = Rc::clone(ghostty_widgets);
    let map_browsers = Rc::clone(browser_controls);
    let map_states = Rc::clone(canvas_occlusion_states);
    scroll.connect_map(move |scroll| {
        let weak_scroll = scroll.downgrade();
        let map_enabled = Rc::clone(&map_enabled);
        let map_minimap = map_minimap.clone();
        let map_layout = map_layout.clone();
        let map_ghostty = Rc::clone(&map_ghostty);
        let map_browsers = Rc::clone(&map_browsers);
        let map_states = Rc::clone(&map_states);
        glib::idle_add_local_once(move || {
            let Some(scroll) = weak_scroll.upgrade() else {
                return;
            };
            for (adjustment, target) in [
                (scroll.hadjustment(), viewport.0),
                (scroll.vadjustment(), viewport.1),
            ] {
                let maximum = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
                adjustment.set_value(
                    (target - adjustment.page_size() / 2.0).clamp(adjustment.lower(), maximum),
                );
            }
            glib::idle_add_local_once(move || {
                map_enabled.set(true);
                sync_canvas_viewport_lifecycle(
                    &scroll,
                    &map_layout,
                    &map_ghostty,
                    &map_browsers,
                    &map_states,
                );
                if let Some(minimap) = map_minimap.as_ref() {
                    canvas_minimap_sync_visibility(minimap);
                }
            });
        });
    });
}

fn attach_canvas_pointer_navigation(
    overlay: &gtk::Overlay,
    scroll: &gtk::ScrolledWindow,
    layout: &GtkCanvasLayout,
    app_state: &Arc<Mutex<AppState>>,
    minimap: Option<&GtkCanvasMinimapControls>,
) {
    let pointer = Rc::new(Cell::new((f64::NAN, f64::NAN)));
    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let motion_pointer = Rc::clone(&pointer);
    motion.connect_motion(move |_, x, y| motion_pointer.set((x, y)));
    overlay.add_controller(motion);

    let initial_center = (
        layout.logical_origin_x + (layout.viewport_x - layout.padding) / layout.scale,
        layout.logical_origin_y + (layout.viewport_y - layout.padding) / layout.scale,
    );
    let live_zoom = Rc::new(Cell::new(layout.scale));
    let live_center = Rc::new(Cell::new(initial_center));
    let controller = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::KINETIC,
    );
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let scroll_widget = scroll.clone();
    let scroll_app_state = Arc::clone(app_state);
    let scroll_workspace = layout.workspace_target.clone();
    let scroll_pointer = Rc::clone(&pointer);
    let scroll_zoom = Rc::clone(&live_zoom);
    let scroll_center = Rc::clone(&live_center);
    let scroll_minimap = minimap.cloned();
    controller.connect_scroll(move |controller, dx, dy| {
        let modifiers = controller.current_event_state();
        if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
            for (adjustment, delta) in [
                (scroll_widget.hadjustment(), dx),
                (scroll_widget.vadjustment(), dy),
            ] {
                adjustment.set_value(canvas_adjustment_after_scroll(
                    adjustment.value(),
                    delta,
                    adjustment.lower(),
                    adjustment.upper(),
                    adjustment.page_size(),
                ));
            }
            if let Some(minimap) = scroll_minimap.as_ref() {
                canvas_minimap_reveal(minimap);
            }
            return glib::Propagation::Stop;
        }
        if modifiers.contains(gdk::ModifierType::ALT_MASK) && dy != 0.0 {
            let old_zoom = scroll_zoom.get();
            let factor = (-dy * 0.10).exp().clamp(0.5, 2.0);
            let new_zoom = (old_zoom * factor).clamp(0.1, 8.0);
            if (new_zoom - old_zoom).abs() <= f64::EPSILON {
                return glib::Propagation::Stop;
            }
            let horizontal = scroll_widget.hadjustment();
            let vertical = scroll_widget.vadjustment();
            let mut pointer = scroll_pointer.get();
            if !pointer.0.is_finite() || !pointer.1.is_finite() {
                pointer = (horizontal.page_size() / 2.0, vertical.page_size() / 2.0);
            }
            let center = canvas_zoom_toward_pointer(
                scroll_center.get(),
                old_zoom,
                new_zoom,
                (horizontal.page_size(), vertical.page_size()),
                pointer,
            );
            scroll_zoom.set(new_zoom);
            scroll_center.set(center);
            call_app(
                &scroll_app_state,
                "canvas.set_viewport",
                json!({
                    "workspace_id": scroll_workspace,
                    "x": center.0,
                    "y": center.1,
                    "zoom": new_zoom
                }),
            );
            if let Some(minimap) = scroll_minimap.as_ref() {
                canvas_minimap_reveal(minimap);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    overlay.add_controller(controller);

    let pinch = gtk::GestureZoom::new();
    pinch.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pinch_start_zoom = Rc::new(Cell::new(layout.scale));
    let pinch_start_center = Rc::new(Cell::new(initial_center));
    let begin_zoom = Rc::clone(&pinch_start_zoom);
    let begin_center = Rc::clone(&pinch_start_center);
    let begin_live_zoom = Rc::clone(&live_zoom);
    let begin_live_center = Rc::clone(&live_center);
    pinch.connect_begin(move |gesture, _| {
        begin_zoom.set(begin_live_zoom.get());
        begin_center.set(begin_live_center.get());
        let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    let pinch_scroll = scroll.clone();
    let pinch_app_state = Arc::clone(app_state);
    let pinch_workspace = layout.workspace_target.clone();
    let pinch_live_zoom = Rc::clone(&live_zoom);
    let pinch_live_center = Rc::clone(&live_center);
    let pinch_minimap = minimap.cloned();
    pinch.connect_scale_changed(move |gesture, scale| {
        let old_zoom = pinch_start_zoom.get();
        let new_zoom = (old_zoom * scale).clamp(0.1, 8.0);
        let horizontal = pinch_scroll.hadjustment();
        let vertical = pinch_scroll.vadjustment();
        let pointer = gesture
            .bounding_box_center()
            .unwrap_or((horizontal.page_size() / 2.0, vertical.page_size() / 2.0));
        let center = canvas_zoom_toward_pointer(
            pinch_start_center.get(),
            old_zoom,
            new_zoom,
            (horizontal.page_size(), vertical.page_size()),
            pointer,
        );
        pinch_live_zoom.set(new_zoom);
        pinch_live_center.set(center);
        call_app(
            &pinch_app_state,
            "canvas.set_viewport",
            json!({
                "workspace_id": pinch_workspace,
                "x": center.0,
                "y": center.1,
                "zoom": new_zoom
            }),
        );
        if let Some(minimap) = pinch_minimap.as_ref() {
            canvas_minimap_reveal(minimap);
        }
        let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    overlay.add_controller(pinch);
}

fn sync_canvas_viewport_lifecycle(
    scroll: &gtk::ScrolledWindow,
    layout: &GtkCanvasLayout,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    canvas_occlusion_states: &GtkCanvasOcclusionStates,
) {
    let horizontal = scroll.hadjustment();
    let vertical = scroll.vadjustment();
    let visible = canvas_logical_viewport(
        layout,
        horizontal.value(),
        vertical.value(),
        horizontal.page_size(),
        vertical.page_size(),
    );
    if visible.width <= 1.0 || visible.height <= 1.0 {
        return;
    }
    let rendering = canvas_rendering_targets(&layout.placements, visible, 0.5);
    let rows = layout
        .placements
        .iter()
        .map(|placement| {
            (
                placement.surface_target.clone(),
                placement.focused,
                !rendering.contains(&placement.surface_target),
            )
        })
        .collect::<Vec<_>>();
    {
        let mut states = canvas_occlusion_states.borrow_mut();
        states.retain(|surface, _| rows.iter().any(|(target, _, _)| target == surface));
        for (surface, _, occluded) in &rows {
            states.insert(surface.clone(), *occluded);
        }
    }
    {
        let widgets = ghostty_widgets.borrow();
        for (surface, focused, occluded) in &rows {
            if let Some(widget) = widgets.get(surface) {
                widget.update_presentation(*focused, *occluded);
            }
        }
    }
    {
        let browsers = browser_controls.borrow();
        for (surface, _, occluded) in &rows {
            if let Some(controls) = browsers.get(surface) {
                controls.root.set_visible(!occluded);
            }
        }
    }
}

fn surface_split_layout(views: &[Value]) -> Option<GtkSplitLayout> {
    let frames = views
        .iter()
        .enumerate()
        .filter(|(_, view)| value_bool_or(view, "visible", true))
        .map(|(index, view)| Some((index, surface_frame(view)?)))
        .collect::<Option<Vec<_>>>()?;
    split_layout_from_frames(&frames)
}

fn split_layout_from_frames(frames: &[(usize, GtkSurfaceFrame)]) -> Option<GtkSplitLayout> {
    let bounds = surface_frame_union(frames)?;
    if let [(view_index, frame)] = frames {
        return Some(GtkSplitLayout::Leaf {
            view_index: *view_index,
            bounds: *frame,
        });
    }

    for axis in [GtkSplitAxis::Horizontal, GtkSplitAxis::Vertical] {
        let mut dividers = frames
            .iter()
            .flat_map(|(_, frame)| match axis {
                GtkSplitAxis::Horizontal => [frame.left, frame.right],
                GtkSplitAxis::Vertical => [frame.top, frame.bottom],
            })
            .filter(|divider| match axis {
                GtkSplitAxis::Horizontal => *divider > bounds.left && *divider < bounds.right,
                GtkSplitAxis::Vertical => *divider > bounds.top && *divider < bounds.bottom,
            })
            .collect::<Vec<_>>();
        dividers.sort_unstable();
        dividers.dedup();
        for divider in dividers {
            let mut leading = Vec::new();
            let mut trailing = Vec::new();
            let mut crosses = false;
            for item @ (_, frame) in frames {
                let (start, end) = match axis {
                    GtkSplitAxis::Horizontal => (frame.left, frame.right),
                    GtkSplitAxis::Vertical => (frame.top, frame.bottom),
                };
                if end <= divider {
                    leading.push(*item);
                } else if start >= divider {
                    trailing.push(*item);
                } else {
                    crosses = true;
                    break;
                }
            }
            if crosses || leading.is_empty() || trailing.is_empty() {
                continue;
            }
            let Some(leading) = split_layout_from_frames(&leading) else {
                continue;
            };
            let Some(trailing) = split_layout_from_frames(&trailing) else {
                continue;
            };
            return Some(GtkSplitLayout::Split {
                axis,
                bounds,
                divider,
                leading: Box::new(leading),
                trailing: Box::new(trailing),
            });
        }
    }
    None
}

fn surface_frame_union(frames: &[(usize, GtkSurfaceFrame)]) -> Option<GtkSurfaceFrame> {
    Some(GtkSurfaceFrame {
        left: frames.iter().map(|(_, frame)| frame.left).min()?,
        top: frames.iter().map(|(_, frame)| frame.top).min()?,
        right: frames.iter().map(|(_, frame)| frame.right).max()?,
        bottom: frames.iter().map(|(_, frame)| frame.bottom).max()?,
    })
}

fn build_split_layout_widget(
    layout: &GtkSplitLayout,
    cards: &mut HashMap<usize, gtk::Box>,
    app_state: &Arc<Mutex<AppState>>,
    workspace_id: &str,
) -> Option<gtk::Widget> {
    match layout {
        GtkSplitLayout::Leaf { view_index, .. } => cards
            .remove(view_index)
            .map(|card| card.upcast::<gtk::Widget>()),
        GtkSplitLayout::Split {
            axis,
            bounds,
            divider,
            leading,
            trailing,
        } => {
            let leading = build_split_layout_widget(leading, cards, app_state, workspace_id)?;
            let trailing = build_split_layout_widget(trailing, cards, app_state, workspace_id)?;
            let orientation = match axis {
                GtkSplitAxis::Horizontal => gtk::Orientation::Horizontal,
                GtkSplitAxis::Vertical => gtk::Orientation::Vertical,
            };
            let paned = gtk::Paned::new(orientation);
            paned.add_css_class("cmux-split");
            paned.set_hexpand(true);
            paned.set_vexpand(true);
            paned.set_wide_handle(true);
            paned.set_resize_start_child(true);
            paned.set_resize_end_child(true);
            paned.set_shrink_start_child(true);
            paned.set_shrink_end_child(true);
            paned.set_start_child(Some(&leading));
            paned.set_end_child(Some(&trailing));

            let axis_start = match axis {
                GtkSplitAxis::Horizontal => bounds.left,
                GtkSplitAxis::Vertical => bounds.top,
            };
            let axis_end = match axis {
                GtkSplitAxis::Horizontal => bounds.right,
                GtkSplitAxis::Vertical => bounds.bottom,
            };
            let divider = *divider;
            let ratio = (divider - axis_start) as f64 / (axis_end - axis_start) as f64;
            let initialized = Rc::new(Cell::new(false));
            let pointer_down = Rc::new(Cell::new(false));
            let pointer_changed_position = Rc::new(Cell::new(false));
            let settle_initialized = Rc::clone(&initialized);
            let settle_pointer_down = Rc::clone(&pointer_down);
            let settle_started = Rc::new(Cell::new(None::<Instant>));
            let settle_stable_since = Rc::new(Cell::new(None::<Instant>));
            let settle_last_size = Rc::new(Cell::new(0_i32));
            paned.add_tick_callback(move |paned, _| {
                if settle_pointer_down.get() {
                    settle_initialized.set(true);
                    return glib::ControlFlow::Break;
                }

                let now = Instant::now();
                let started = settle_started.get().unwrap_or_else(|| {
                    settle_started.set(Some(now));
                    now
                });
                let size = match orientation {
                    gtk::Orientation::Horizontal => paned.width(),
                    gtk::Orientation::Vertical => paned.height(),
                    _ => 0,
                };
                if size <= 0 {
                    return glib::ControlFlow::Continue;
                }

                let position = (size as f64 * ratio).round() as i32;
                if paned.position() != position {
                    paned.set_position(position);
                }
                settle_initialized.set(true);

                if settle_last_size.replace(size) != size {
                    settle_stable_since.set(Some(now));
                    return glib::ControlFlow::Continue;
                }
                let stable_since = settle_stable_since.get().unwrap_or(now);
                if now.duration_since(started) >= GTK_SPLIT_INITIAL_SETTLE_INTERVAL
                    && now.duration_since(stable_since) >= GTK_SPLIT_STABLE_INTERVAL
                {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });

            let position_pointer_down = Rc::clone(&pointer_down);
            let position_changed = Rc::clone(&pointer_changed_position);
            paned.connect_position_notify(move |_| {
                if position_pointer_down.get() {
                    position_changed.set(true);
                }
            });

            let resize_app_state = Arc::clone(app_state);
            let resize_workspace_id = workspace_id.to_string();
            let resize_initialized = Rc::clone(&initialized);
            let event_pointer_down = Rc::clone(&pointer_down);
            let event_changed = Rc::clone(&pointer_changed_position);
            let weak_paned = paned.downgrade();
            let pointer_events = gtk::EventControllerLegacy::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .build();
            pointer_events.connect_event(move |_, event| {
                match event.event_type() {
                    gdk::EventType::ButtonPress => {
                        event_pointer_down.set(true);
                        event_changed.set(false);
                    }
                    gdk::EventType::ButtonRelease => {
                        event_pointer_down.set(false);
                        if !resize_initialized.get() || !event_changed.replace(false) {
                            return glib::Propagation::Proceed;
                        }
                        let weak_paned = weak_paned.clone();
                        let timeout_app_state = Arc::clone(&resize_app_state);
                        let timeout_workspace_id = resize_workspace_id.clone();
                        glib::idle_add_local_once(move || {
                            let Some(paned) = weak_paned.upgrade() else {
                                return;
                            };
                            let size = match orientation {
                                gtk::Orientation::Horizontal => paned.width(),
                                gtk::Orientation::Vertical => paned.height(),
                                _ => 0,
                            };
                            if size <= 0 {
                                return;
                            }
                            let fraction = (paned.position() as f64 / size as f64).clamp(0.0, 1.0);
                            call_app(
                                &timeout_app_state,
                                "debug.layout.resize_split",
                                json!({
                                    "workspace_id": timeout_workspace_id,
                                    "axis": match orientation {
                                        gtk::Orientation::Horizontal => "horizontal",
                                        gtk::Orientation::Vertical => "vertical",
                                        _ => "horizontal"
                                    },
                                    "start": axis_start,
                                    "end": axis_end,
                                    "divider": divider,
                                    "fraction": fraction
                                }),
                            );
                        });
                    }
                    _ => {}
                }
                glib::Propagation::Proceed
            });
            paned.add_controller(pointer_events);
            Some(paned.upcast())
        }
    }
}

fn fallback_surface_grid(mut cards: HashMap<usize, gtk::Box>) -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_hexpand(true);
    grid.set_vexpand(true);
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    let mut cards = cards.drain().collect::<Vec<_>>();
    cards.sort_by_key(|(index, _)| *index);
    let columns = if cards.len() <= 1 { 1 } else { 2 };
    for (visible_index, (_, card)) in cards.into_iter().enumerate() {
        grid.attach(
            &card,
            (visible_index % columns) as i32,
            (visible_index / columns) as i32,
            1,
            1,
        );
    }
    grid
}

fn surface_frame(view: &Value) -> Option<GtkSurfaceFrame> {
    let frame = view.get("frame")?;
    let x = finite_frame_number(frame, "x")?;
    let y = finite_frame_number(frame, "y")?;
    let width = finite_frame_number(frame, "width")?;
    let height = finite_frame_number(frame, "height")?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let left = x.round() as i64;
    let top = y.round() as i64;
    let right = (x + width).round() as i64;
    let bottom = (y + height).round() as i64;
    (right > left && bottom > top).then_some(GtkSurfaceFrame {
        left,
        top,
        right,
        bottom,
    })
}

fn finite_frame_number(frame: &Value, key: &str) -> Option<f64> {
    frame
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn toolbar(snapshot: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.add_css_class("cmux-toolbar");
    let [browser_icon, plus_label] = toolbar_primary_action_markers();
    toolbar.append(&icon_action_button(
        browser_icon,
        "Open Browser",
        app_state,
        "browser.open_split",
        json!({"url": "about:blank", "focus": true}),
    ));
    let (new_workspace_method, new_workspace_params) = new_workspace_request_for_snapshot(snapshot);
    let plus = action_button(
        plus_label,
        app_state,
        new_workspace_method,
        new_workspace_params,
    );
    plus.add_css_class("cmux-plus-action");
    plus.set_tooltip_text(Some("New Workspace"));
    toolbar.append(&plus);
    toolbar.append(&action_button(
        "Split Right",
        app_state,
        "surface.split",
        json!({"direction": "right"}),
    ));
    toolbar.append(&action_button(
        "Split Down",
        app_state,
        "surface.split",
        json!({"direction": "down"}),
    ));
    toolbar.append(&action_button(
        "Terminal",
        app_state,
        "surface.create",
        json!({"type": "terminal", "focus": true}),
    ));
    if canvas_mode(snapshot) {
        toolbar.append(&icon_action_button(
            "view-dual-symbolic",
            "Use Split Layout",
            app_state,
            "canvas.set_mode",
            json!({"mode": "splits"}),
        ));
        for (icon, tooltip, direction) in [
            ("zoom-in-symbolic", "Zoom In", "in"),
            ("zoom-out-symbolic", "Zoom Out", "out"),
        ] {
            toolbar.append(&icon_action_button(
                icon,
                tooltip,
                app_state,
                "canvas.zoom",
                json!({"direction": direction}),
            ));
        }
        toolbar.append(&icon_action_button(
            "zoom-fit-best-symbolic",
            "Show Canvas Overview",
            app_state,
            "canvas.overview",
            json!({}),
        ));
    } else {
        toolbar.append(&icon_action_button(
            "view-grid-symbolic",
            "Use Canvas Layout",
            app_state,
            "canvas.set_mode",
            json!({"mode": "canvas"}),
        ));
    }
    toolbar.append(&action_button(
        "Install Claude",
        app_state,
        "integration.claude.open_installer",
        json!({}),
    ));
    toolbar.append(&action_button(
        "Install Codex",
        app_state,
        "integration.codex.open_installer",
        json!({}),
    ));
    toolbar.append(&action_button(
        "Install OpenCode",
        app_state,
        "integration.opencode.open_installer",
        json!({}),
    ));
    toolbar.append(&action_button(
        "Palette",
        app_state,
        "debug.command_palette.toggle",
        json!({}),
    ));
    toolbar.append(&action_button(
        "?",
        app_state,
        "help.shortcuts.toggle",
        json!({}),
    ));
    toolbar
}

fn toolbar_primary_action_markers() -> [&'static str; 2] {
    [BROWSER_TOOLBAR_ICON, NEW_WORKSPACE_TOOLBAR_LABEL]
}

fn canvas_mode(snapshot: &Value) -> bool {
    snapshot.pointer("/canvas/mode").and_then(Value::as_str) == Some("canvas")
}

fn new_workspace_request_for_snapshot(snapshot: &Value) -> (&'static str, Value) {
    if let Some(group_target) = selected_workspace_group_target(snapshot) {
        (
            "workspace.group.new_workspace",
            json!({
                "group_id": group_target,
                "focus": true,
                "placement": "afterCurrent",
                "placement_reference": "current_workspace"
            }),
        )
    } else {
        let placement = snapshot
            .pointer("/config/app/newWorkspacePlacement")
            .and_then(Value::as_str)
            .unwrap_or("afterCurrent");
        let inherit_working_directory = snapshot
            .pointer("/config/app/workspaceInheritWorkingDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        (
            "workspace.create",
            json!({
                "title": "Workspace",
                "focus": true,
                "placement": placement,
                "inherit_working_directory": inherit_working_directory
            }),
        )
    }
}

fn selected_workspace_group_target(snapshot: &Value) -> Option<String> {
    snapshot
        .get("workspaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|workspace| workspace_selected(workspace))
        .and_then(|workspace| {
            value_string(workspace, "group_ref").or_else(|| value_string(workspace, "group_id"))
        })
}

fn app_chrome_sidebar(snapshot: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let chrome = gtk::Box::new(gtk::Orientation::Vertical, 10);
    chrome.add_css_class("cmux-chrome");
    chrome.set_focusable(true);
    let configured_width = config::sidebar_settings()
        .right_max_width
        .unwrap_or(300.0)
        .clamp(276.0, 4096.0);
    chrome.set_width_request(configured_width.round() as i32);
    chrome.set_vexpand(true);

    let mode = right_sidebar_mode(snapshot);
    let right_sidebar = snapshot.get("right_sidebar").unwrap_or(&Value::Null);
    chrome.append(&right_sidebar_mode_toolbar(&mode, right_sidebar, app_state));
    chrome.append(&label(right_sidebar_mode_label(&mode), "cmux-heading"));
    let sidebar = snapshot.get("sidebar").unwrap_or(&Value::Null);
    match mode.as_str() {
        "files" => append_right_sidebar_files(&chrome, sidebar, app_state, false),
        "find" => append_right_sidebar_files(&chrome, sidebar, app_state, true),
        "sessions" => append_right_sidebar_sessions(&chrome, snapshot, app_state),
        "feed" => append_right_sidebar_feed(&chrome, snapshot, app_state),
        "dock" => append_right_sidebar_dock(&chrome, snapshot, app_state),
        _ => append_right_sidebar_files(&chrome, sidebar, app_state, false),
    }
    chrome
}

fn right_sidebar_visible(snapshot: &Value) -> bool {
    snapshot
        .get("right_sidebar")
        .and_then(|sidebar| sidebar.get("visible"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn right_sidebar_mode(snapshot: &Value) -> String {
    match snapshot
        .get("right_sidebar")
        .and_then(|sidebar| sidebar.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("files")
    {
        "vault" => "sessions".to_string(),
        mode @ ("files" | "find" | "sessions" | "feed" | "dock") => mode.to_string(),
        _ => "files".to_string(),
    }
}

fn right_sidebar_mode_label(mode: &str) -> &'static str {
    match mode {
        "find" => "Find",
        "sessions" => "Vault",
        "feed" => "Feed",
        "dock" => "Dock",
        _ => "Files",
    }
}

fn right_sidebar_mode_toolbar(
    mode: &str,
    right_sidebar: &Value,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Box {
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    toolbar.add_css_class("cmux-sidebar-modes");
    let available_modes = right_sidebar
        .get("available_modes")
        .and_then(Value::as_array)
        .map(|modes| {
            modes
                .iter()
                .filter_map(Value::as_str)
                .collect::<HashSet<_>>()
        });
    for (target, icon, tooltip) in [
        ("files", "folder-symbolic", "Files"),
        ("find", "edit-find-symbolic", "Find"),
        ("sessions", "view-list-symbolic", "Vault"),
        ("feed", "mail-unread-symbolic", "Feed"),
        ("dock", "utilities-terminal-symbolic", "Dock"),
    ] {
        if available_modes
            .as_ref()
            .is_some_and(|modes| !modes.contains(target))
        {
            continue;
        }
        let button = icon_action_button(
            icon,
            tooltip,
            app_state,
            "sidebar.right",
            json!({"action": "set", "mode": target, "no_focus": true}),
        );
        if mode == target {
            button.add_css_class("cmux-sidebar-mode-active");
        }
        toolbar.append(&button);
    }
    toolbar
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GtkSidebarFileEntry {
    label: String,
    path: String,
    is_directory: bool,
}

fn append_right_sidebar_files(
    chrome: &gtk::Box,
    sidebar: &Value,
    app_state: &Arc<Mutex<AppState>>,
    searchable: bool,
) {
    let cwd = value_str(sidebar, "cwd", "");
    if !cwd.is_empty() {
        chrome.append(&row_label(cwd, "cmux-muted"));
    }
    let entries = Rc::new(sidebar_file_entries(Path::new(cwd), searchable, 240));
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
    rows.set_vexpand(true);

    if searchable {
        let search = gtk::SearchEntry::new();
        search.add_css_class("cmux-right-sidebar-input");
        search.set_placeholder_text(Some("Find files"));
        let rows_for_search = rows.clone();
        let entries_for_search = Rc::clone(&entries);
        let app_state_for_search = Arc::clone(app_state);
        search.connect_search_changed(move |search| {
            populate_sidebar_file_rows(
                &rows_for_search,
                &entries_for_search,
                search.text().as_str(),
                &app_state_for_search,
            );
        });
        chrome.append(&search);
    }
    populate_sidebar_file_rows(&rows, &entries, "", app_state);
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&rows)
        .build();
    chrome.append(&scroll);
}

fn populate_sidebar_file_rows(
    rows: &gtk::Box,
    entries: &[GtkSidebarFileEntry],
    query: &str,
    app_state: &Arc<Mutex<AppState>>,
) {
    while let Some(child) = rows.first_child() {
        rows.remove(&child);
    }
    let query = query.trim().to_ascii_lowercase();
    let matches = entries
        .iter()
        .filter(|entry| query.is_empty() || entry.label.to_ascii_lowercase().contains(&query))
        .take(40)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        rows.append(&row_label("No matching files", "cmux-muted"));
        return;
    }
    for entry in matches {
        rows.append(&sidebar_file_button(entry, app_state));
    }
}

fn sidebar_file_button(
    entry: &GtkSidebarFileEntry,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&gtk::Image::from_icon_name(if entry.is_directory {
        "folder-symbolic"
    } else {
        "text-x-generic-symbolic"
    }));
    let name = label(&entry.label, "cmux-muted");
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    content.append(&name);
    let button = gtk::Button::builder().child(&content).build();
    button.add_css_class("cmux-sidebar-file");
    button.set_tooltip_text(Some(&entry.path));
    let app_state = Arc::clone(app_state);
    let path = entry.path.clone();
    let kind = if entry.is_directory {
        "directory"
    } else {
        "file"
    };
    button.connect_clicked(move |_| {
        call_app(
            &app_state,
            "open.targets",
            json!({"targets": [{"kind": kind, "path": path}], "focus": true}),
        );
    });
    button
}

fn sidebar_file_entries(root: &Path, recursive: bool, limit: usize) -> Vec<GtkSidebarFileEntry> {
    if root.as_os_str().is_empty() || !root.is_dir() || limit == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        let Ok(read_dir) = fs::read_dir(&directory) else {
            continue;
        };
        let mut children = read_dir.flatten().collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for child in children {
            let path = child.path();
            let is_directory = path.is_dir();
            let relative = path.strip_prefix(root).unwrap_or(&path);
            result.push(GtkSidebarFileEntry {
                label: relative.to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_directory,
            });
            if result.len() >= limit {
                break;
            }
            if recursive
                && is_directory
                && depth < 4
                && !matches!(
                    child.file_name().to_string_lossy().as_ref(),
                    ".git" | "target" | "node_modules" | ".zig-cache"
                )
            {
                pending.push((path, depth + 1));
            }
        }
        if result.len() >= limit || !recursive {
            break;
        }
    }
    result.sort_by(|left, right| {
        right.is_directory.cmp(&left.is_directory).then_with(|| {
            left.label
                .to_ascii_lowercase()
                .cmp(&right.label.to_ascii_lowercase())
        })
    });
    result
}

fn append_right_sidebar_sessions(
    chrome: &gtk::Box,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let sessions = snapshot
        .get("surfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|surface| surface.get("resume_binding").is_some_and(Value::is_object))
        .collect::<Vec<_>>();
    if sessions.is_empty() {
        chrome.append(&row_label("No saved sessions", "cmux-muted"));
        return;
    }
    for surface in sessions {
        append_sidebar_surface_button(chrome, surface, app_state);
    }
}

fn append_right_sidebar_feed(
    chrome: &gtk::Box,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let sidebar = snapshot.get("sidebar").unwrap_or(&Value::Null);
    append_status_section(chrome, sidebar);
    append_log_section(chrome, sidebar);
    let right_sidebar = snapshot.get("right_sidebar").unwrap_or(&Value::Null);
    let items = right_sidebar
        .get("feed_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        chrome.append(&row_label("No feed items", "cmux-muted"));
    } else {
        for item in items.iter().rev().take(20) {
            let title = value_string(item, "title")
                .or_else(|| value_string(item, "message"))
                .or_else(|| value_string(item, "workstream_id"))
                .or_else(|| value_string(item, "id"))
                .or_else(|| value_string(item, "item_id"))
                .unwrap_or_else(|| "Feed item".to_string());
            chrome.append(&row_label(&title, "cmux-muted"));
        }
    }
    append_notification_section(chrome, snapshot, app_state);
}

fn append_right_sidebar_dock(
    chrome: &gtk::Box,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let terminals = snapshot
        .get("surfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|surface| value_str(surface, "type", "") == "terminal")
        .collect::<Vec<_>>();
    if terminals.is_empty() {
        chrome.append(&row_label("No terminal sessions", "cmux-muted"));
        return;
    }
    for surface in terminals {
        append_sidebar_surface_button(chrome, surface, app_state);
    }
}

fn append_sidebar_surface_button(
    chrome: &gtk::Box,
    surface: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    let Some(target) = surface_id_or_ref(surface) else {
        return;
    };
    let title = value_str(
        surface,
        "title",
        value_str(surface, "surface_ref", "Surface"),
    );
    let button = action_button(
        title,
        app_state,
        "surface.focus",
        json!({"surface_id": target}),
    );
    button.add_css_class("cmux-sidebar-file");
    chrome.append(&button);
}

fn append_status_section(chrome: &gtk::Box, sidebar: &Value) {
    chrome.append(&section_heading("Status"));
    let progress = value_str(sidebar, "progress", "none");
    if progress != "none" {
        chrome.append(&row_label(
            &format!("Progress: {progress}"),
            "cmux-status-row",
        ));
    }

    let statuses = sidebar
        .get("statuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if statuses.is_empty() && progress == "none" {
        chrome.append(&label("No status entries", "cmux-muted"));
        return;
    }

    for status in statuses {
        let text = status_text(&status);
        chrome.append(&row_label(&text, "cmux-status-row"));
    }
}

fn append_log_section(chrome: &gtk::Box, sidebar: &Value) {
    chrome.append(&section_heading("Recent Logs"));
    let logs = sidebar
        .get("logs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if logs.is_empty() {
        chrome.append(&label("No log entries", "cmux-muted"));
        return;
    }

    for entry in logs {
        let source = value_str(&entry, "source", "");
        let prefix = if source.is_empty() {
            value_str(&entry, "level", "info").to_string()
        } else {
            format!("{} / {}", value_str(&entry, "level", "info"), source)
        };
        let text = format!("{prefix}: {}", value_str(&entry, "message", ""));
        chrome.append(&row_label(&text, "cmux-log-row"));
    }
}

fn append_notification_section(
    chrome: &gtk::Box,
    snapshot: &Value,
    app_state: &Arc<Mutex<AppState>>,
) {
    chrome.append(&section_heading("Notifications"));
    let notifications = snapshot
        .get("notifications")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unread = notifications
        .into_iter()
        .filter(|notification| {
            !notification
                .get("read")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .take(5)
        .collect::<Vec<_>>();
    if unread.is_empty() {
        chrome.append(&label("No unread notifications", "cmux-muted"));
        return;
    }

    for notification in unread {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.add_css_class("cmux-notification");
        row.append(&label(
            value_str(&notification, "title", "cmux"),
            "cmux-heading",
        ));
        let detail = notification_detail(&notification);
        if !detail.is_empty() {
            row.append(&label(&detail, "cmux-muted"));
        }

        let button = gtk::Button::builder().child(&row).build();
        button.add_css_class("cmux-notification-button");
        button.set_focusable(true);
        let click_app_state = Arc::clone(app_state);
        let params = notification_focus_params(&notification);
        button.connect_clicked(move |_| {
            call_app(&click_app_state, "debug.notification.focus", params.clone());
        });
        attach_notification_context_menu(&button, app_state, &notification);
        chrome.append(&button);
    }
}

fn command_palette_panel(snapshot: &Value) -> Option<gtk::Box> {
    let palette = snapshot.get("command_palette")?;
    if !palette
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.add_css_class("cmux-palette");
    let query = value_str(palette, "query", "");
    let mode = value_str(palette, "mode", "commands");
    let heading = if mode == "global_search" {
        if query.is_empty() {
            "Search all windows, panels, browser tabs...".to_string()
        } else {
            format!("Search all windows: {query}")
        }
    } else {
        format!("{mode}: {query}")
    };
    panel.append(&label(&heading, "cmux-heading"));

    let selected = palette
        .get("selected_index")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    for (index, result) in palette
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let input_row = command_palette_input_mode(mode);
        let global_search_row = mode == "global_search";
        let row = gtk::Box::new(
            if input_row || global_search_row {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            },
            8,
        );
        row.add_css_class("cmux-palette-row");
        if index == selected {
            row.add_css_class("cmux-palette-selected");
        }
        let title = value_str(result, "title", "Command");
        let kind = value_str(result, "trailing_label", "");
        row.append(&label(
            &if global_search_row && !kind.is_empty() {
                format!("{title} · {kind}")
            } else {
                title.to_string()
            },
            "cmux-heading",
        ));
        if input_row {
            let draft = label(value_str(result, "label", ""), "cmux-palette-input");
            draft.set_wrap(true);
            draft.set_selectable(true);
            draft.set_xalign(0.0);
            row.append(&draft);
            let input_hint = value_str(result, "input_hint", "");
            if !input_hint.is_empty() {
                row.append(&label(input_hint, "cmux-muted"));
            }
        }
        if global_search_row {
            let snippet = value_str(result, "snippet", "");
            if !snippet.is_empty() {
                let snippet = label(snippet, "cmux-muted");
                snippet.set_wrap(true);
                snippet.set_xalign(0.0);
                row.append(&snippet);
            }
            let location = value_str(result, "location", "");
            if !location.is_empty() {
                row.append(&label(location, "cmux-muted"));
            }
        }
        let hint = linux_shortcut_label(result);
        if !hint.is_empty() {
            row.append(&label(&hint, "cmux-muted"));
        }
        row.set_hexpand(true);
        panel.append(&row);
    }

    Some(panel)
}

fn command_palette_input_mode(mode: &str) -> bool {
    matches!(mode, "rename_input" | "workspace_description_input")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutHelpDismissInteraction {
    CloseButton,
    BackdropPress,
    PanelPress,
    PlainEscape,
}

fn handle_shortcut_help_dismissal(
    app_state: &Arc<Mutex<AppState>>,
    window_id: &str,
    interaction: ShortcutHelpDismissInteraction,
) -> bool {
    if interaction == ShortcutHelpDismissInteraction::PanelPress {
        return false;
    }
    call_app(
        app_state,
        "help.shortcuts.toggle",
        json!({"window_id": window_id, "visible": false}),
    )
}

fn shortcut_help_is_visible(app_state: &Arc<Mutex<AppState>>, window_id: &str) -> bool {
    call_app_value(app_state, "help.shortcuts", json!({"window_id": window_id}))
        .and_then(|value| value.get("visible").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn configure_shortcut_help_overlay_panel(panel: &gtk::Box) {
    panel.set_valign(gtk::Align::Fill);
    panel.set_vexpand(true);
    panel.set_margin_top(24);
    panel.set_margin_bottom(24);
}

fn shortcut_help_panel(
    snapshot: &Value,
    dismissal: Option<(&Arc<Mutex<AppState>>, &str)>,
) -> Option<gtk::Box> {
    let help = snapshot.get("shortcut_help")?;
    if !shortcut_help_visible(snapshot) {
        return None;
    }

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.add_css_class("cmux-shortcut-help");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("cmux-shortcut-help-header");
    let title = label(
        value_str(help, "title", "Keyboard Shortcuts"),
        "cmux-heading",
    );
    title.set_hexpand(true);
    title.set_xalign(0.0);
    header.append(&title);
    if let Some((app_state, window_id)) = dismissal {
        let close_text = strings::text("shortcut_help.close");
        let close = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("window-close-symbolic"))
            .build();
        close.add_css_class("cmux-shortcut-help-close");
        close.set_tooltip_text(Some(&close_text));
        close.update_property(&[gtk::accessible::Property::Label(&close_text)]);
        let app_state = Arc::clone(app_state);
        let window_id = window_id.to_string();
        close.connect_clicked(move |_| {
            handle_shortcut_help_dismissal(
                &app_state,
                &window_id,
                ShortcutHelpDismissInteraction::CloseButton,
            );
        });
        header.append(&close);
    }
    panel.append(&header);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 6);
    rows.add_css_class("cmux-shortcut-help-rows");
    for row_value in help
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("cmux-palette-row");
        row.append(&label(
            value_str(row_value, "title", "Shortcut"),
            "cmux-heading",
        ));
        let hint = linux_shortcut_label(row_value);
        if !hint.is_empty() {
            row.append(&label(&hint, "cmux-muted"));
        }
        let description = value_str(row_value, "description", "");
        if !description.is_empty() {
            row.append(&label(description, "cmux-muted"));
        }
        row.set_hexpand(true);
        rows.append(&row);
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_height(120)
        .max_content_height(520)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&rows)
        .build();
    scroll.add_css_class("cmux-shortcut-help-scroll");
    panel.append(&scroll);

    Some(panel)
}

fn shortcut_help_visible(snapshot: &Value) -> bool {
    snapshot
        .get("shortcut_help")
        .and_then(|help| help.get("visible"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn linux_shortcut_hint_label(hint: &str) -> String {
    if hint.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    let mut key = String::new();
    for ch in hint.chars() {
        match ch {
            '⇧' => parts.push("Shift".to_string()),
            '⌃' => parts.push("Ctrl".to_string()),
            '⌥' => parts.push("Alt".to_string()),
            '⌘' => parts.push("Super".to_string()),
            other => key.push(other),
        }
    }
    if !key.is_empty() {
        parts.push(key);
    }
    if parts.is_empty() {
        hint.to_string()
    } else {
        parts.join("+")
    }
}

fn linux_shortcut_label(row: &Value) -> String {
    let label = value_str(row, "shortcut_label", "");
    if !label.is_empty() {
        label.to_string()
    } else {
        linux_shortcut_hint_label(&value_str(row, "shortcut_hint", ""))
    }
}

fn action_button(
    title: &str,
    app_state: &Arc<Mutex<AppState>>,
    method: &'static str,
    params: Value,
) -> gtk::Button {
    let button = gtk::Button::with_label(title);
    button.add_css_class("cmux-action");
    button.set_focusable(false);
    let app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(&app_state, method, params.clone());
    });
    button
}

fn icon_action_button(
    icon_name: &'static str,
    tooltip: &str,
    app_state: &Arc<Mutex<AppState>>,
    method: &'static str,
    params: Value,
) -> gtk::Button {
    let image = gtk::Image::from_icon_name(icon_name);
    let button = gtk::Button::builder().child(&image).build();
    button.add_css_class("cmux-action");
    button.add_css_class("cmux-icon-action");
    button.set_focusable(false);
    button.set_tooltip_text(Some(tooltip));
    let app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(&app_state, method, params.clone());
    });
    button
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GtkPaneTab {
    surface_id: String,
    title: String,
    kind: String,
    selected: bool,
    pinned: bool,
    unread: bool,
}

fn pane_tabs(view: &Value) -> Vec<GtkPaneTab> {
    view.get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tab| {
            let surface_id = surface_id_or_ref(tab)?;
            Some(GtkPaneTab {
                surface_id,
                title: value_str(tab, "title", "Surface").to_string(),
                kind: value_str(tab, "kind", "terminal").to_string(),
                selected: value_bool(tab, "selected"),
                pinned: value_bool(tab, "pinned"),
                unread: value_bool(tab, "unread"),
            })
        })
        .collect()
}

fn pane_tab_icon(kind: &str) -> &'static str {
    match kind {
        "browser" => "web-browser-symbolic",
        "project" | "file" | "filePreview" | "markdown" | "diff" => "text-x-generic-symbolic",
        "settings" => "preferences-system-symbolic",
        "agent-session" => "system-run-symbolic",
        _ => "utilities-terminal-symbolic",
    }
}

fn pane_new_terminal_params(view: &Value) -> Option<Value> {
    Some(json!({
        "workspace_id": workspace_id_or_ref(view)?,
        "pane_id": pane_id_or_ref(view)?,
        "type": "terminal",
        "focus": true
    }))
}

fn pane_tab_close_button_visible(tab_count: usize, hidden: bool) -> bool {
    tab_count > 0 && !hidden
}

fn pane_tab_strip(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: Option<&GtkLocalRefresh>,
) -> Option<gtk::Box> {
    let tabs = pane_tabs(view);
    if tabs.is_empty() {
        return None;
    }
    let pane_id = pane_id_or_ref(view)?;

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    root.add_css_class("cmux-pane-tabs");
    root.set_hexpand(true);
    root.set_widget_name(&pane_id);
    populate_pane_tab_strip(&root, view, app_state, local_refresh);
    Some(root)
}

fn configure_pane_tab_button(button: &gtk::Button, tab: &GtkPaneTab) {
    let current_title =
        widget_descendant_with_css_class(button.upcast_ref(), "cmux-pane-tab-title")
            .and_then(|widget| widget.downcast::<gtk::Label>().ok())
            .map(|label| label.text());
    if current_title.as_deref() != Some(tab.title.as_str()) {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        content.append(&gtk::Image::from_icon_name(pane_tab_icon(&tab.kind)));
        let title = label(&tab.title, "cmux-pane-tab-title");
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(18);
        content.append(&title);
        button.set_child(Some(&content));
    }
    button.set_focusable(false);
    button.set_widget_name(&tab.surface_id);
    let tooltip = if tab.pinned {
        format!("{} (Pinned)", tab.title)
    } else {
        tab.title.clone()
    };
    button.set_tooltip_text(Some(&tooltip));
    if tab.selected {
        button.add_css_class("cmux-pane-tab-selected");
    } else {
        button.remove_css_class("cmux-pane-tab-selected");
    }
    if tab.unread {
        button.add_css_class("cmux-pane-tab-unread");
    } else {
        button.remove_css_class("cmux-pane-tab-unread");
    }
}

fn pane_tab_scroller(root: &gtk::Box) -> (gtk::ScrolledWindow, gtk::Box) {
    if let Some(scroller) = root
        .first_child()
        .and_then(|child| child.downcast::<gtk::ScrolledWindow>().ok())
    {
        if let Some(row) =
            widget_descendant_with_css_class(scroller.upcast_ref(), "cmux-pane-tab-row")
                .and_then(|child| child.downcast::<gtk::Box>().ok())
        {
            return (scroller, row);
        }
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    row.add_css_class("cmux-pane-tab-row");
    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&row)
        .build();
    scroller.add_css_class("cmux-pane-tab-scroll");
    root.prepend(&scroller);
    (scroller, row)
}

fn populate_pane_tab_strip(
    root: &gtk::Box,
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: Option<&GtkLocalRefresh>,
) {
    let tabs = pane_tabs(view);
    let (scroller, tab_row) = pane_tab_scroller(root);
    let mut existing = HashMap::new();
    let mut child = tab_row.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.downcast::<gtk::Button>() {
            existing.insert(button.widget_name().to_string(), button);
        }
    }
    let mut previous = None;
    for tab in &tabs {
        let button = existing.remove(&tab.surface_id).unwrap_or_else(|| {
            let button = gtk::Button::new();
            button.add_css_class("cmux-pane-tab");
            let surface_id = tab.surface_id.clone();
            let app_state = Arc::clone(app_state);
            let local_refresh = local_refresh.cloned();
            button.connect_clicked(move |_| {
                if call_app(
                    &app_state,
                    "surface.focus",
                    json!({"surface_id": surface_id}),
                ) {
                    if let Some(local_refresh) = local_refresh.as_ref() {
                        local_refresh.schedule();
                    }
                }
            });
            tab_row.append(&button);
            button
        });
        configure_pane_tab_button(&button, tab);
        tab_row.reorder_child_after(&button, previous.as_ref());
        previous = Some(button);
    }
    for button in existing.into_values() {
        tab_row.remove(&button);
    }

    let mut child = scroller.next_sibling();
    while let Some(widget) = child {
        child = widget.next_sibling();
        root.remove(&widget);
    }
    let hide_close_button = app_state
        .lock()
        .ok()
        .map(|app| {
            app.app_workspace_settings_value()
                .get("hideTabCloseButton")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if pane_tab_close_button_visible(tabs.len(), hide_close_button) {
        if let Some(selected) = tabs.iter().find(|tab| tab.selected) {
            let close = gtk::Button::builder()
                .child(&gtk::Image::from_icon_name("window-close-symbolic"))
                .build();
            close.add_css_class("cmux-pane-tab-tool");
            close.set_focusable(false);
            close.set_tooltip_text(Some("Close Tab"));
            let surface_id = selected.surface_id.clone();
            let app_state = Arc::clone(app_state);
            let local_refresh = local_refresh.cloned();
            close.connect_clicked(move |_| {
                if call_app(
                    &app_state,
                    "surface.close",
                    json!({"surface_id": surface_id, "source": "tab_button"}),
                ) {
                    if let Some(local_refresh) = local_refresh.as_ref() {
                        local_refresh.schedule();
                    }
                }
            });
            root.append(&close);
        }
    }
    if let Some(params) = pane_new_terminal_params(view) {
        let add = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("list-add-symbolic"))
            .build();
        add.add_css_class("cmux-pane-tab-tool");
        add.set_focusable(false);
        add.set_tooltip_text(Some("New Terminal Tab"));
        let app_state = Arc::clone(app_state);
        let local_refresh = local_refresh.cloned();
        add.connect_clicked(move |_| {
            if call_app(&app_state, "surface.create", params.clone()) {
                if let Some(local_refresh) = local_refresh.as_ref() {
                    local_refresh.schedule();
                }
            }
        });
        root.append(&add);
    }
}

fn sync_pane_tab_strips(
    window: &gtk::ApplicationWindow,
    snapshot: &Value,
    previous_keys: &HashMap<String, Value>,
    next_keys: &HashMap<String, Value>,
    app_state: &Arc<Mutex<AppState>>,
    local_refresh: &GtkLocalRefresh,
) {
    let Some(root) = window.child() else {
        return;
    };
    for view in snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|view| value_bool_or(view, "visible", true))
    {
        let Some(pane_id) = pane_id_or_ref(view) else {
            continue;
        };
        if previous_keys.get(&pane_id) == next_keys.get(&pane_id) {
            continue;
        }
        if let Some(strip) = find_pane_tab_strip(&root, &pane_id) {
            populate_pane_tab_strip(&strip, view, app_state, Some(local_refresh));
        }
    }
}

fn find_named_box_with_css_class(
    root: &gtk::Widget,
    css_class: &str,
    widget_name: &str,
) -> Option<gtk::Box> {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.has_css_class(css_class) && widget.widget_name() == widget_name {
            return widget.downcast::<gtk::Box>().ok();
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    None
}

fn find_pane_tab_strip(root: &gtk::Widget, pane_id: &str) -> Option<gtk::Box> {
    find_named_box_with_css_class(root, "cmux-pane-tabs", pane_id)
}

fn find_pane_surface_card(root: &gtk::Widget, pane_id: &str) -> Option<gtk::Box> {
    find_named_box_with_css_class(root, "cmux-surface", pane_id)
}

fn replace_pane_surface_card(old: &gtk::Box, new: &gtk::Box) -> bool {
    let Some(parent) = old.parent() else {
        return false;
    };
    if let Ok(paned) = parent.clone().downcast::<gtk::Paned>() {
        if paned.start_child().as_ref() == Some(old.upcast_ref()) {
            paned.set_start_child(Some(new));
            return true;
        }
        if paned.end_child().as_ref() == Some(old.upcast_ref()) {
            paned.set_end_child(Some(new));
            return true;
        }
        return false;
    }
    if let Ok(grid) = parent.clone().downcast::<gtk::Grid>() {
        let (column, row, width, height) = grid.query_child(old);
        grid.remove(old);
        grid.attach(new, column, row, width, height);
        return true;
    }
    if let Ok(scroller) = parent.clone().downcast::<gtk::ScrolledWindow>() {
        scroller.set_child(Some(new));
        return true;
    }
    if let Ok(viewport) = parent.clone().downcast::<gtk::Viewport>() {
        viewport.set_child(Some(new));
        return true;
    }
    if let Ok(parent_box) = parent.downcast::<gtk::Box>() {
        let previous = old.prev_sibling();
        parent_box.remove(old);
        parent_box.insert_child_after(new, previous.as_ref());
        return true;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn sync_pane_surface_cards(
    window: &gtk::ApplicationWindow,
    snapshot: &Value,
    previous_keys: &HashMap<String, Value>,
    next_keys: &HashMap<String, Value>,
    app_state: &Arc<Mutex<AppState>>,
    pane_allocations: &PaneAllocations,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    diff_controls: &DiffSurfaceControlsCache,
    terminal_search_controls: &TerminalSearchControlsCache,
    terminal_text_box_controls: &TerminalTextBoxControlsCache,
    renderer_mode: GtkRendererMode,
    ui_mode: GtkUiMode,
    local_refresh: &GtkLocalRefresh,
) -> bool {
    let Some(root) = window.child() else {
        return false;
    };
    let config_reload_generation = config_reload_generation(snapshot);
    for view in snapshot
        .get("surface_views")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|view| value_bool_or(view, "visible", true))
    {
        let Some(pane_id) = pane_id_or_ref(view) else {
            return false;
        };
        if previous_keys.get(&pane_id) == next_keys.get(&pane_id) {
            continue;
        }
        let Some(old_card) = find_pane_surface_card(&root, &pane_id) else {
            return false;
        };
        let new_card = surface_card(
            view,
            app_state,
            pane_allocations,
            ghostty_widgets,
            browser_controls,
            diff_controls,
            terminal_search_controls,
            terminal_text_box_controls,
            renderer_mode,
            config_reload_generation,
            ui_mode,
            local_refresh,
        );
        if let (Some(old_strip), Some(new_strip)) = (
            find_pane_tab_strip(old_card.upcast_ref(), &pane_id),
            find_pane_tab_strip(new_card.upcast_ref(), &pane_id),
        ) {
            new_card.remove(&new_strip);
            old_card.remove(&old_strip);
            populate_pane_tab_strip(&old_strip, view, app_state, Some(local_refresh));
            new_card.prepend(&old_strip);
        }
        for widget in ghostty_widgets.borrow().values() {
            if widget_is_or_descendant_of(widget.root().upcast_ref(), old_card.upcast_ref()) {
                detach_widget(widget.root());
            }
        }
        if !replace_pane_surface_card(&old_card, &new_card) {
            return false;
        }
    }
    true
}

fn pango_escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

fn markdown_inline_pango(text: &str) -> String {
    let mut escaped = pango_escape(text);
    for (marker, open, close) in [
        ("**", "<b>", "</b>"),
        ("__", "<b>", "</b>"),
        ("`", "<tt>", "</tt>"),
    ] {
        let mut output = String::new();
        let mut remaining = escaped.as_str();
        let mut opened = false;
        while let Some(index) = remaining.find(marker) {
            output.push_str(&remaining[..index]);
            output.push_str(if opened { close } else { open });
            opened = !opened;
            remaining = &remaining[index + marker.len()..];
        }
        output.push_str(remaining);
        if opened {
            output.push_str(close);
        }
        escaped = output;
    }
    escaped
}

fn markdown_pango_markup(source: &str, font_size: f64) -> String {
    let body_size = (font_size * 1024.0).round().max(8.0 * 1024.0) as i32;
    let mut output = format!("<span size=\"{body_size}\">");
    let mut in_code = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code {
                output.push_str("</tt></span>\n");
            } else {
                output.push_str("<span background=\"#252a2e\"><tt>");
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            output.push_str(&pango_escape(line));
            output.push('\n');
            continue;
        }
        let hashes = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if hashes > 0 && hashes <= 6 && trimmed.as_bytes().get(hashes) == Some(&b' ') {
            let scale = match hashes {
                1 => "xx-large",
                2 => "x-large",
                3 => "large",
                _ => "medium",
            };
            output.push_str(&format!(
                "<span size=\"{scale}\" weight=\"bold\">{}</span>\n",
                markdown_inline_pango(trimmed[hashes + 1..].trim())
            ));
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            output.push_str("  •  ");
            output.push_str(&markdown_inline_pango(item));
            output.push('\n');
        } else if let Some(quote) = trimmed.strip_prefix('>') {
            output.push_str("<span foreground=\"#a7b0b8\">│ ");
            output.push_str(&markdown_inline_pango(quote.trim()));
            output.push_str("</span>\n");
        } else {
            output.push_str(&markdown_inline_pango(line));
            output.push('\n');
        }
    }
    if in_code {
        output.push_str("</tt></span>");
    }
    output.push_str("</span>");
    output
}

fn diff_pango_markup(source: &str, font_size: f64) -> String {
    let size = (font_size * 1024.0).round().max(8.0 * 1024.0) as i32;
    let mut output = format!("<span font_family=\"monospace\" size=\"{size}\">");
    for line in source.lines() {
        let (background, foreground, weight) =
            if line.starts_with("diff --git") || line.starts_with("+++") || line.starts_with("---")
            {
                ("#18263a", "#79c0ff", "bold")
            } else if line.starts_with("@@") {
                ("#123247", "#a5d6ff", "normal")
            } else if line.starts_with('+') {
                ("#173322", "#7ee787", "normal")
            } else if line.starts_with('-') {
                ("#3a1c20", "#ffa198", "normal")
            } else {
                ("#181b1e", "#e6edf3", "normal")
            };
        output.push_str(&format!(
            "<span background=\"{background}\" foreground=\"{foreground}\" weight=\"{weight}\">{}</span>\n",
            pango_escape(line)
        ));
    }
    output.push_str("</span>");
    output
}

fn split_diff_pango_column(
    rows: &[diff_viewer::SplitDiffRow],
    old_side: bool,
    font_size: f64,
) -> (String, String) {
    let size = (font_size * 1024.0).round().max(8.0 * 1024.0) as i32;
    let mut numbers = format!("<span font_family=\"monospace\" size=\"{size}\">");
    let mut content = format!("<span font_family=\"monospace\" size=\"{size}\">");
    for row in rows {
        let (background, foreground, weight) = match row.kind {
            diff_viewer::SplitDiffRowKind::Header => ("#18263a", "#79c0ff", "bold"),
            diff_viewer::SplitDiffRowKind::Hunk => ("#123247", "#a5d6ff", "normal"),
            diff_viewer::SplitDiffRowKind::Context => ("#181b1e", "#e6edf3", "normal"),
            diff_viewer::SplitDiffRowKind::Change if old_side => ("#3a1c20", "#ffa198", "normal"),
            diff_viewer::SplitDiffRowKind::Change => ("#173322", "#7ee787", "normal"),
            diff_viewer::SplitDiffRowKind::Meta => ("#181b1e", "#8b949e", "normal"),
        };
        let line_number = if old_side { row.old_line } else { row.new_line }
            .map(|line| line.to_string())
            .unwrap_or_default();
        let text = if old_side {
            row.old_text.as_str()
        } else {
            row.new_text.as_str()
        };
        numbers.push_str(&format!(
            "<span background=\"{background}\" foreground=\"#7d8790\">{}</span>\n",
            pango_escape(&line_number)
        ));
        content.push_str(&format!(
            "<span background=\"{background}\" foreground=\"{foreground}\" weight=\"{weight}\">{}</span>\n",
            pango_escape(if text.is_empty() { " " } else { text })
        ));
    }
    numbers.push_str("</span>");
    content.push_str("</span>");
    (numbers, content)
}

fn split_diff_section_widget(section: &diff_viewer::SplitDiffSection, font_size: f64) -> gtk::Box {
    let section_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    section_box.add_css_class("cmux-diff-section");
    let heading = label(&section.path, "cmux-heading");
    heading.set_xalign(0.0);
    section_box.append(&heading);

    let (old_numbers, old_content) = split_diff_pango_column(&section.rows, true, font_size);
    let (new_numbers, new_content) = split_diff_pango_column(&section.rows, false, font_size);
    let grid = gtk::Grid::new();
    grid.add_css_class("cmux-diff-split-grid");
    grid.set_column_spacing(6);
    grid.set_hexpand(true);

    let old_line_numbers = label("", "cmux-diff-split-line-numbers");
    old_line_numbers.set_markup(&old_numbers);
    old_line_numbers.set_xalign(1.0);
    old_line_numbers.set_yalign(0.0);
    let old_code = label("", "cmux-diff-split-code");
    old_code.set_markup(&old_content);
    old_code.set_selectable(true);
    old_code.set_xalign(0.0);
    old_code.set_yalign(0.0);
    old_code.set_hexpand(true);

    let divider = gtk::Separator::new(gtk::Orientation::Vertical);
    divider.add_css_class("cmux-diff-split-divider");

    let new_line_numbers = label("", "cmux-diff-split-line-numbers");
    new_line_numbers.set_markup(&new_numbers);
    new_line_numbers.set_xalign(1.0);
    new_line_numbers.set_yalign(0.0);
    let new_code = label("", "cmux-diff-split-code");
    new_code.set_markup(&new_content);
    new_code.set_selectable(true);
    new_code.set_xalign(0.0);
    new_code.set_yalign(0.0);
    new_code.set_hexpand(true);

    grid.attach(&old_line_numbers, 0, 0, 1, 1);
    grid.attach(&old_code, 1, 0, 1, 1);
    grid.attach(&divider, 2, 0, 1, 1);
    grid.attach(&new_line_numbers, 3, 0, 1, 1);
    grid.attach(&new_code, 4, 0, 1, 1);
    section_box.append(&grid);
    section_box
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeDiffSection {
    path: String,
    content: String,
}

fn native_diff_sections(source: &str) -> Vec<NativeDiffSection> {
    let mut sections = Vec::new();
    let mut path = "Overview".to_string();
    let mut lines = Vec::new();
    for line in source.lines() {
        if let Some(next_path) = native_diff_header_path(line) {
            if !lines.is_empty() {
                sections.push(NativeDiffSection {
                    path,
                    content: format!("{}\n", lines.join("\n")),
                });
            }
            path = next_path;
            lines.clear();
        }
        lines.push(line);
    }
    if !lines.is_empty() {
        sections.push(NativeDiffSection {
            path,
            content: format!("{}\n", lines.join("\n")),
        });
    }
    if sections.is_empty() {
        sections.push(NativeDiffSection {
            path: "Diff".to_string(),
            content: source.to_string(),
        });
    }
    sections
}

fn native_diff_header_path(line: &str) -> Option<String> {
    diff_viewer::diff_header_path(line)
}

fn native_diff_matching_sections(paths: &[String], query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    paths
        .iter()
        .enumerate()
        .filter_map(|(index, path)| path.to_ascii_lowercase().contains(&query).then_some(index))
        .collect()
}

fn scroll_diff_to_widget(scroll: &gtk::ScrolledWindow, widget: &gtk::Widget) {
    let scroll = scroll.clone();
    let widget = widget.clone();
    glib::idle_add_local_once(move || {
        let adjustment = scroll.vadjustment();
        let y = widget.allocation().y() as f64;
        let maximum = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
        adjustment.set_value(y.clamp(adjustment.lower(), maximum));
    });
}

fn update_diff_file_search(
    paths: &[String],
    widgets: &[gtk::Widget],
    scroll: &gtk::ScrolledWindow,
    count: &gtk::Label,
    query: &str,
    requested_match: usize,
) -> Option<usize> {
    let matches = native_diff_matching_sections(paths, query);
    if matches.is_empty() {
        count.set_text(if query.trim().is_empty() {
            ""
        } else {
            "No matches"
        });
        return None;
    }
    let selected = requested_match % matches.len();
    count.set_text(&format!("{} of {}", selected + 1, matches.len()));
    if let Some(widget) = widgets.get(matches[selected]) {
        scroll_diff_to_widget(scroll, widget);
    }
    Some(selected)
}

fn ensure_diff_surface_controls(
    surface_id: &str,
    document: &Value,
    cache: &DiffSurfaceControlsCache,
) -> DiffSurfaceControls {
    let content = document
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(controls) = cache
        .borrow()
        .get(surface_id)
        .filter(|controls| controls.document_key == *document)
        .cloned()
    {
        return controls;
    }

    let font_size = document
        .get("font_size")
        .and_then(Value::as_f64)
        .unwrap_or(10.0);
    let layout = document
        .get("display_mode")
        .and_then(Value::as_str)
        .unwrap_or("unified");
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    search_row.add_css_class("cmux-terminal-search-bar");
    search_row.set_visible(false);
    let search = gtk::SearchEntry::new();
    search.add_css_class("cmux-diff-file-search");
    search.set_hexpand(true);
    search.set_placeholder_text(Some("Search changed files"));
    let search_count = label("", "cmux-terminal-search-count");
    let close = browser_icon_button("window-close-symbolic", "Close File Search");
    search_row.append(&search);
    search_row.append(&search_count);
    search_row.append(&close);
    root.append(&search_row);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.add_css_class("cmux-document-content");
    for annotation in document
        .get("annotations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let file = annotation
            .get("file_path")
            .and_then(Value::as_str)
            .unwrap_or("Review comment");
        let line = annotation.get("line").and_then(Value::as_u64);
        let message = annotation
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let annotation_label = label(
            &format!(
                "{}{}\n{}",
                file,
                line.map(|line| format!(":{line}")).unwrap_or_default(),
                message
            ),
            "cmux-document-annotation",
        );
        annotation_label.set_wrap(true);
        annotation_label.set_xalign(0.0);
        body.append(&annotation_label);
    }

    let mut section_paths = Vec::new();
    let mut section_widgets = Vec::new();
    if layout == "split" {
        let sections = diff_viewer::split_diff_sections(content);
        section_widgets.reserve(sections.len());
        section_paths.reserve(sections.len());
        for section in sections {
            section_paths.push(section.path.clone());
            let section_box = split_diff_section_widget(&section, font_size);
            body.append(&section_box);
            section_widgets.push(section_box.upcast::<gtk::Widget>());
        }
    } else {
        let sections = native_diff_sections(content);
        section_widgets.reserve(sections.len());
        section_paths.reserve(sections.len());
        for section in sections {
            section_paths.push(section.path.clone());
            let section_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
            section_box.add_css_class("cmux-diff-section");
            let heading = label(&section.path, "cmux-heading");
            heading.set_xalign(0.0);
            section_box.append(&heading);
            let diff = label("", "cmux-project-value");
            diff.set_markup(&diff_pango_markup(&section.content, font_size));
            diff.set_selectable(true);
            diff.set_xalign(0.0);
            section_box.append(&diff);
            body.append(&section_box);
            section_widgets.push(section_box.upcast::<gtk::Widget>());
        }
    }
    let section_paths = Rc::new(section_paths);
    let section_widgets = Rc::new(section_widgets);
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&body)
        .build();
    scroll.add_css_class("cmux-diff-scroll");
    root.append(&scroll);

    let selected_match = Rc::new(Cell::new(0usize));
    let changed_paths = Rc::clone(&section_paths);
    let changed_widgets = Rc::clone(&section_widgets);
    let changed_scroll = scroll.clone();
    let changed_count = search_count.clone();
    let changed_selected = Rc::clone(&selected_match);
    search.connect_search_changed(move |entry| {
        changed_selected.set(0);
        update_diff_file_search(
            &changed_paths,
            &changed_widgets,
            &changed_scroll,
            &changed_count,
            entry.text().as_str(),
            0,
        );
    });
    let activate_paths = Rc::clone(&section_paths);
    let activate_widgets = Rc::clone(&section_widgets);
    let activate_scroll = scroll.clone();
    let activate_count = search_count.clone();
    let activate_selected = Rc::clone(&selected_match);
    search.connect_activate(move |entry| {
        let requested = activate_selected.get().saturating_add(1);
        if let Some(selected) = update_diff_file_search(
            &activate_paths,
            &activate_widgets,
            &activate_scroll,
            &activate_count,
            entry.text().as_str(),
            requested,
        ) {
            activate_selected.set(selected);
        }
    });
    let close_row = search_row.clone();
    let close_scroll = scroll.clone();
    close.connect_clicked(move |_| {
        close_row.set_visible(false);
        close_scroll.grab_focus();
    });
    let escape_row = search_row.clone();
    let escape_scroll = scroll.clone();
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gdk::Key::Escape {
            escape_row.set_visible(false);
            escape_scroll.grab_focus();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    search.add_controller(key);

    let controls = DiffSurfaceControls {
        root,
        search_row,
        search,
        scroll,
        document_key: document.clone(),
    };
    cache
        .borrow_mut()
        .insert(surface_id.to_string(), controls.clone());
    controls
}

fn document_toolbar(document: &Value) -> gtk::Box {
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.add_css_class("cmux-document-toolbar");
    let path = document
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| document.get("source_label").and_then(Value::as_str))
        .unwrap_or_default();
    let mode = document
        .get("display_mode")
        .and_then(Value::as_str)
        .unwrap_or("document");
    let title = label(if path.is_empty() { mode } else { path }, "cmux-muted");
    title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    toolbar.append(&title);
    if !path.is_empty() {
        let open = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("document-open-symbolic"))
            .build();
        open.set_tooltip_text(Some("Open Externally"));
        let uri = crate::file_url::file_url_for_path(path);
        open.connect_clicked(move |_| {
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        });
        toolbar.append(&open);
    }
    toolbar
}

fn submit_agent_session_prompt(
    app_state: &Arc<Mutex<AppState>>,
    surface_id: &str,
    prompt: &gtk::TextView,
    has_attachments: bool,
) {
    let buffer = prompt.buffer();
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    if text.trim().is_empty() && !has_attachments {
        return;
    }
    if call_app(
        app_state,
        "agent_session.send",
        json!({"surface_id": surface_id, "text": text}),
    ) {
        buffer.set_text("");
    }
}

fn open_agent_session_attachment_picker(
    root: &gtk::Box,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
    action: gtk::FileChooserAction,
) {
    let selecting_folder = action == gtk::FileChooserAction::SelectFolder;
    let dialog = gtk::FileChooserNative::builder()
        .title(if selecting_folder {
            "Attach Folders"
        } else {
            "Attach Files"
        })
        .action(action)
        .accept_label("Attach")
        .cancel_label("Cancel")
        .select_multiple(true)
        .build();
    if let Some(parent) = root
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
    {
        dialog.set_transient_for(Some(&parent));
    }
    let surface_id = surface_id.to_string();
    let app_state = Arc::clone(app_state);
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let files = dialog.files();
            let paths = (0..files.n_items())
                .filter_map(|index| files.item(index))
                .filter_map(|item| item.downcast::<gio::File>().ok())
                .filter_map(|file| file.path())
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                call_app(
                    &app_state,
                    "agent_session.attachment.add",
                    json!({"surface_id": surface_id, "paths": paths}),
                );
            }
        }
        dialog.destroy();
    });
    dialog.show();
}

fn agent_session_surface_view(view: &Value, app_state: &Arc<Mutex<AppState>>) -> Option<gtk::Box> {
    let state = view.get("agent_session")?;
    let surface_id = surface_id_or_ref(view)?;
    let provider_id = state
        .get("provider_id")
        .and_then(Value::as_str)
        .unwrap_or("codex");
    let status = state
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let running = status == "running";
    let active = matches!(status, "running" | "starting");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("cmux-agent-session");
    root.set_hexpand(true);
    root.set_vexpand(true);

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.add_css_class("cmux-agent-session-toolbar");
    let provider = gtk::ComboBoxText::new();
    provider.append(Some("codex"), "Codex");
    provider.append(Some("claude"), "Claude Code");
    provider.append(Some("opencode"), "OpenCode");
    provider.set_active_id(Some(provider_id));
    provider.set_sensitive(!active);
    provider.set_tooltip_text(Some("Agent Provider"));
    let provider_state = Arc::clone(app_state);
    let provider_surface = surface_id.clone();
    provider.connect_changed(move |provider| {
        if let Some(provider_id) = provider.active_id() {
            call_app(
                &provider_state,
                "agent_session.set_provider",
                json!({"surface_id": provider_surface, "provider": provider_id.as_str()}),
            );
        }
    });
    toolbar.append(&provider);

    let status_text = label(
        &format!(
            "{} · {}",
            status,
            state
                .get("renderer_kind")
                .and_then(Value::as_str)
                .unwrap_or("react")
        ),
        "cmux-muted",
    );
    status_text.set_hexpand(true);
    status_text.set_xalign(0.0);
    toolbar.append(&status_text);

    let lifecycle = gtk::Button::builder()
        .child(&gtk::Image::from_icon_name(if active {
            "media-playback-stop-symbolic"
        } else {
            "media-playback-start-symbolic"
        }))
        .build();
    lifecycle.set_tooltip_text(Some(if active {
        "Stop Agent Session"
    } else {
        "Start Agent Session"
    }));
    let lifecycle_state = Arc::clone(app_state);
    let lifecycle_surface = surface_id.clone();
    lifecycle.connect_clicked(move |_| {
        call_app(
            &lifecycle_state,
            if active {
                "agent_session.stop"
            } else {
                "agent_session.start"
            },
            json!({"surface_id": lifecycle_surface}),
        );
    });
    toolbar.append(&lifecycle);
    root.append(&toolbar);

    if let Some(error) = state
        .get("last_error")
        .and_then(Value::as_str)
        .filter(|error| !error.is_empty())
    {
        let error = label(error, "cmux-agent-session-error");
        error.set_wrap(true);
        error.set_xalign(0.0);
        root.append(&error);
    }

    let transcript_buffer = gtk::TextBuffer::new(None);
    let transcript = gtk::TextView::with_buffer(&transcript_buffer);
    transcript.add_css_class("cmux-agent-session-transcript");
    transcript.set_editable(false);
    transcript.set_cursor_visible(false);
    transcript.set_monospace(true);
    transcript.set_wrap_mode(gtk::WrapMode::WordChar);
    transcript.set_left_margin(14);
    transcript.set_right_margin(14);
    transcript.set_top_margin(12);
    transcript.set_bottom_margin(12);
    let transcript_scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&transcript)
        .build();
    root.append(&transcript_scroll);

    let refresh_transcript = {
        let app_state = Arc::clone(app_state);
        let surface_id = surface_id.clone();
        move |buffer: &gtk::TextBuffer| {
            let Some(value) = call_app_value(
                &app_state,
                "agent_session.output",
                json!({"surface_id": surface_id}),
            ) else {
                return;
            };
            let output = value
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let current = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            if current != output {
                buffer.set_text(output);
            }
        }
    };
    refresh_transcript(&transcript_buffer);
    let weak_transcript = transcript.downgrade();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let Some(transcript) = weak_transcript.upgrade() else {
            return glib::ControlFlow::Break;
        };
        refresh_transcript(&transcript.buffer());
        glib::ControlFlow::Continue
    });

    let pending_attachments = state
        .get("pending_attachments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_attachments = !pending_attachments.is_empty();
    if has_attachments {
        let tray = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tray.add_css_class("cmux-agent-session-attachments");
        for attachment in &pending_attachments {
            let attachment_id = value_str(attachment, "id", "").to_string();
            let attachment_path = value_str(attachment, "path", "").to_string();
            let attachment_kind = value_str(attachment, "kind", "file");
            let item = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            item.add_css_class("cmux-agent-session-attachment");
            item.set_tooltip_text(Some(&attachment_path));
            item.append(&gtk::Image::from_icon_name(
                if attachment_kind == "directory" {
                    "folder-symbolic"
                } else {
                    "text-x-generic-symbolic"
                },
            ));
            let name = label(
                value_str(attachment, "label", &attachment_path),
                "cmux-heading",
            );
            name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            name.set_max_width_chars(24);
            item.append(&name);
            let remove = gtk::Button::from_icon_name("window-close-symbolic");
            remove.set_tooltip_text(Some("Remove attachment"));
            let remove_state = Arc::clone(app_state);
            let remove_surface = surface_id.clone();
            remove.connect_clicked(move |_| {
                call_app(
                    &remove_state,
                    "agent_session.attachment.remove",
                    json!({
                        "surface_id": remove_surface,
                        "attachment_id": attachment_id
                    }),
                );
            });
            item.append(&remove);
            tray.append(&item);
        }
        if pending_attachments.len() > 1 {
            let clear = gtk::Button::from_icon_name("edit-clear-symbolic");
            clear.set_tooltip_text(Some("Clear attachments"));
            let clear_state = Arc::clone(app_state);
            let clear_surface = surface_id.clone();
            clear.connect_clicked(move |_| {
                call_app(
                    &clear_state,
                    "agent_session.attachment.clear",
                    json!({"surface_id": clear_surface}),
                );
            });
            tray.append(&clear);
        }
        let tray_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&tray)
            .build();
        root.append(&tray_scroll);
    }

    let composer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    composer.add_css_class("cmux-agent-session-composer");
    let attach_files = gtk::Button::from_icon_name("mail-attachment-symbolic");
    attach_files.set_tooltip_text(Some("Attach files"));
    let files_root = root.clone();
    let files_surface = surface_id.clone();
    let files_state = Arc::clone(app_state);
    attach_files.connect_clicked(move |_| {
        open_agent_session_attachment_picker(
            &files_root,
            &files_surface,
            &files_state,
            gtk::FileChooserAction::Open,
        );
    });
    composer.append(&attach_files);
    let attach_folders = gtk::Button::from_icon_name("folder-open-symbolic");
    attach_folders.set_tooltip_text(Some("Attach folders"));
    let folders_root = root.clone();
    let folders_surface = surface_id.clone();
    let folders_state = Arc::clone(app_state);
    attach_folders.connect_clicked(move |_| {
        open_agent_session_attachment_picker(
            &folders_root,
            &folders_surface,
            &folders_state,
            gtk::FileChooserAction::SelectFolder,
        );
    });
    composer.append(&attach_folders);
    if provider_id == "codex" {
        let permission_mode = state
            .get("permission_mode")
            .and_then(Value::as_str)
            .unwrap_or("default");
        let permissions = gtk::ComboBoxText::new();
        permissions.add_css_class("cmux-agent-session-permissions");
        permissions.append(Some("default"), "Default permissions");
        permissions.append(Some("auto-review"), "Auto-review");
        permissions.append(Some("full-access"), "Full access");
        permissions.append(Some("custom"), "Custom (config.toml)");
        permissions.set_active_id(Some(permission_mode));
        permissions.set_tooltip_text(Some("Permissions for the next Codex turn"));
        let permission_state = Arc::clone(app_state);
        let permission_surface = surface_id.clone();
        permissions.connect_changed(move |permissions| {
            if let Some(permission_mode) = permissions.active_id() {
                call_app(
                    &permission_state,
                    "agent_session.set_permission_mode",
                    json!({
                        "surface_id": permission_surface,
                        "permission_mode": permission_mode.as_str()
                    }),
                );
            }
        });
        composer.append(&permissions);
    }
    let prompt = gtk::TextView::new();
    prompt.add_css_class("cmux-agent-session-editor");
    prompt.set_hexpand(true);
    prompt.set_vexpand(false);
    prompt.set_accepts_tab(false);
    prompt.set_wrap_mode(gtk::WrapMode::WordChar);
    prompt.set_tooltip_text(Some("Ask anything"));
    prompt.set_sensitive(active);
    prompt.buffer().set_text(value_str(state, "draft_text", ""));
    let editor = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .min_content_height(38)
        .max_content_height(150)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&prompt)
        .build();
    let send = gtk::Button::builder()
        .child(&gtk::Image::from_icon_name("mail-send-symbolic"))
        .build();
    send.set_tooltip_text(Some("Send Prompt"));
    send.set_sensitive(active);
    let interrupt = gtk::Button::builder()
        .child(&gtk::Image::from_icon_name("process-stop-symbolic"))
        .build();
    interrupt.set_tooltip_text(Some("Interrupt"));
    interrupt.set_sensitive(running);

    let draft_state = Arc::clone(app_state);
    let draft_surface = surface_id.clone();
    prompt.buffer().connect_changed(move |buffer| {
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        call_app(
            &draft_state,
            "agent_session.draft.set",
            json!({"surface_id": draft_surface, "text": text}),
        );
    });
    let submit_key = gtk::EventControllerKey::new();
    let key_state = Arc::clone(app_state);
    let key_surface = surface_id.clone();
    let key_prompt = prompt.clone();
    submit_key.connect_key_pressed(move |_, key, _, modifiers| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter)
            && !modifiers.intersects(gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::ALT_MASK)
        {
            submit_agent_session_prompt(&key_state, &key_surface, &key_prompt, has_attachments);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    prompt.add_controller(submit_key);
    let clicked_state = Arc::clone(app_state);
    let clicked_surface = surface_id.clone();
    let clicked_prompt = prompt.clone();
    send.connect_clicked(move |_| {
        submit_agent_session_prompt(
            &clicked_state,
            &clicked_surface,
            &clicked_prompt,
            has_attachments,
        )
    });
    let interrupt_state = Arc::clone(app_state);
    let interrupt_surface = surface_id.clone();
    interrupt.connect_clicked(move |_| {
        call_app(
            &interrupt_state,
            "agent_session.interrupt",
            json!({"surface_id": interrupt_surface}),
        );
    });
    composer.append(&editor);
    composer.append(&interrupt);
    composer.append(&send);
    root.append(&composer);
    Some(root)
}

fn native_document_surface_view(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    diff_controls: &DiffSurfaceControlsCache,
) -> Option<gtk::Box> {
    let document = view.get("document")?;
    let surface_id = surface_id_or_ref(view)?;
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("cmux-document");
    root.set_hexpand(true);
    root.set_vexpand(true);
    let kind = document
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("filePreview");
    let mode = document
        .get("display_mode")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let toolbar = document_toolbar(document);
    let global_search_needle = view
        .get("global_search_needle")
        .and_then(Value::as_str)
        .filter(|needle| !needle.is_empty());
    if let Some(needle) = global_search_needle {
        toolbar.append(&label(&format!("Search: {needle}"), "cmux-muted"));
    }
    if kind == "diff" {
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        layout.add_css_class("linked");
        let unified = gtk::ToggleButton::with_label("Unified");
        unified.set_active(mode != "split");
        unified.set_tooltip_text(Some("Unified Diff"));
        let split = gtk::ToggleButton::with_label("Split");
        split.set_group(Some(&unified));
        split.set_active(mode == "split");
        split.set_tooltip_text(Some("Side-by-Side Diff"));
        for (button, target_mode) in [(&unified, "unified"), (&split, "split")] {
            let app_state = Arc::clone(app_state);
            let surface_id = surface_id.clone();
            button.connect_toggled(move |button| {
                if button.is_active() {
                    call_app(
                        &app_state,
                        "document.set_mode",
                        json!({"surface_id": surface_id, "mode": target_mode}),
                    );
                }
            });
        }
        layout.append(&unified);
        layout.append(&split);
        toolbar.append(&layout);
    }
    root.append(&toolbar);
    let content = document
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let font_size = document
        .get("font_size")
        .and_then(Value::as_f64)
        .unwrap_or(13.0);

    if kind == "markdown" {
        let preview = gtk::ToggleButton::with_label("Preview");
        preview.set_active(mode == "preview");
        preview.set_tooltip_text(Some("Rendered Markdown"));
        let text_mode = gtk::ToggleButton::with_label("Text");
        text_mode.set_group(Some(&preview));
        text_mode.set_active(mode == "text");
        text_mode.set_tooltip_text(Some("Edit Markdown Source"));
        for (button, target_mode) in [(&preview, "preview"), (&text_mode, "text")] {
            let app_state = Arc::clone(app_state);
            let surface_id = surface_id.clone();
            button.connect_clicked(move |button| {
                if button.is_active() {
                    call_app(
                        &app_state,
                        "document.set_mode",
                        json!({"surface_id": surface_id, "mode": target_mode}),
                    );
                }
            });
        }
        toolbar.append(&preview);
        toolbar.append(&text_mode);
    }

    if kind == "filePreview" && mode == "image" {
        let path = document.get("path").and_then(Value::as_str)?;
        let picture = gtk::Picture::for_filename(path);
        picture.set_can_shrink(true);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        root.append(&picture);
        return Some(root);
    }

    if kind == "diff" {
        let controls = ensure_diff_surface_controls(&surface_id, document, diff_controls);
        detach_widget(&controls.root);
        root.append(&controls.root);
        if view.get("focused").and_then(Value::as_bool) == Some(true)
            && !widget_contains_focus(&controls.search)
        {
            let scroll = controls.scroll.clone();
            glib::idle_add_local_once(move || {
                scroll.grab_focus();
            });
        }
        return Some(root);
    }

    if kind == "markdown" && mode == "preview" {
        let markdown = label("", "cmux-document-content");
        markdown.set_markup(&markdown_pango_markup(content, font_size));
        markdown.set_wrap(true);
        markdown.set_selectable(true);
        markdown.set_xalign(0.0);
        markdown.set_yalign(0.0);
        if let Some(needle) = global_search_needle {
            select_label_search_match(&markdown, needle);
        }
        root.append(
            &gtk::ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .child(&markdown)
                .build(),
        );
        return Some(root);
    }

    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(content);
    let text = gtk::TextView::with_buffer(&buffer);
    text.add_css_class("cmux-document-text");
    let editable = matches!(kind, "markdown" | "filePreview") && mode == "text";
    text.set_editable(editable);
    text.set_cursor_visible(true);
    text.set_monospace(true);
    text.set_wrap_mode(gtk::WrapMode::None);
    text.set_left_margin(12);
    text.set_top_margin(12);
    if let Some(needle) = global_search_needle {
        select_text_view_search_match(&text, &buffer, content, needle);
    }
    if editable {
        let dirty = label("", "cmux-muted");
        let revert = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("document-revert-symbolic"))
            .build();
        revert.set_tooltip_text(Some("Revert Unsaved Changes"));
        revert.set_sensitive(false);
        let save = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("document-save-symbolic"))
            .build();
        save.set_tooltip_text(Some("Save"));
        save.set_sensitive(false);

        let original = content.to_string();
        let changed_save = save.clone();
        let changed_revert = revert.clone();
        let changed_dirty = dirty.clone();
        buffer.connect_changed(move |buffer| {
            let value = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            let is_dirty = value != original;
            changed_save.set_sensitive(is_dirty);
            changed_revert.set_sensitive(is_dirty);
            changed_dirty.set_text(if is_dirty { "Modified" } else { "" });
        });

        let save_state = Arc::clone(app_state);
        let save_surface = surface_id.clone();
        let save_buffer = buffer.clone();
        let save_button = save.clone();
        save.connect_clicked(move |_| {
            let content = save_buffer
                .text(&save_buffer.start_iter(), &save_buffer.end_iter(), true)
                .to_string();
            call_app(
                &save_state,
                "document.save",
                json!({"surface_id": save_surface, "content": content}),
            );
            save_button.set_sensitive(false);
        });

        let reload_state = Arc::clone(app_state);
        let reload_surface = surface_id.clone();
        revert.connect_clicked(move |_| {
            call_app(
                &reload_state,
                "document.reload",
                json!({"surface_id": reload_surface}),
            );
        });

        toolbar.append(&dirty);
        toolbar.append(&revert);
        toolbar.append(&save);
    }
    root.append(
        &gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&text)
            .build(),
    );
    Some(root)
}

fn case_insensitive_match_character_offsets(text: &str, needle: &str) -> Option<(i32, i32)> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let lowercase = text.to_lowercase();
    let needle_lowercase = needle.to_lowercase();
    let byte_start = lowercase.find(&needle_lowercase)?;
    let byte_end = byte_start + needle_lowercase.len();
    let start = lowercase[..byte_start].chars().count() as i32;
    let end = start + lowercase[byte_start..byte_end].chars().count() as i32;
    Some((start, end))
}

fn select_label_search_match(label: &gtk::Label, needle: &str) -> bool {
    let text = label.text();
    let Some((start, end)) = case_insensitive_match_character_offsets(text.as_str(), needle) else {
        return false;
    };
    label.select_region(start, end);
    true
}

fn select_text_view_search_match(
    text_view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    text: &str,
    needle: &str,
) -> bool {
    let Some((start_offset, end_offset)) = case_insensitive_match_character_offsets(text, needle)
    else {
        return false;
    };
    let mut start = buffer.iter_at_offset(start_offset);
    let end = buffer.iter_at_offset(end_offset);
    buffer.select_range(&start, &end);
    text_view.scroll_to_iter(&mut start, 0.15, false, 0.0, 0.0);
    true
}

fn project_modules(project: &Value) -> impl Iterator<Item = &Value> {
    project
        .pointer("/model/modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn project_tab_button(
    label: &str,
    tab: &'static str,
    active_tab: &str,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::with_label(label);
    button.set_active(tab == active_tab);
    let app_state = Arc::clone(app_state);
    let surface_id = surface_id.to_string();
    button.connect_clicked(move |_| {
        call_app(
            &app_state,
            "project.set_tab",
            json!({"surface_id": surface_id, "tab": tab}),
        );
    });
    button
}

fn append_project_nodes(
    rows: &gtk::Box,
    nodes: &[Value],
    depth: usize,
    selected_file: &str,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
    count: &mut usize,
) {
    for node in nodes {
        if *count >= 600 {
            return;
        }
        *count += 1;
        let kind = node.get("kind").and_then(Value::as_str).unwrap_or("file");
        let name = node.get("name").and_then(Value::as_str).unwrap_or("Item");
        let path = node.get("path").and_then(Value::as_str).unwrap_or_default();
        let button = gtk::ToggleButton::new();
        button.add_css_class("cmux-project-row");
        button.set_active(kind == "file" && path == selected_file);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.set_margin_start((depth.min(12) * 16) as i32);
        content.append(&gtk::Image::from_icon_name(if kind == "group" {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        }));
        let item_label = label(
            name,
            if path.is_empty() {
                "cmux-muted"
            } else {
                "cmux-project-value"
            },
        );
        item_label.set_xalign(0.0);
        content.append(&item_label);
        button.set_child(Some(&content));
        if kind == "file" && !path.is_empty() {
            let app_state = Arc::clone(app_state);
            let surface_id = surface_id.to_string();
            let path = path.to_string();
            button.connect_clicked(move |_| {
                call_app(
                    &app_state,
                    "project.set_selected_file",
                    json!({"surface_id": surface_id, "path": path}),
                );
            });
        } else {
            button.set_sensitive(false);
        }
        rows.append(&button);
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            append_project_nodes(
                rows,
                children,
                depth + 1,
                selected_file,
                surface_id,
                app_state,
                count,
            );
        }
    }
}

fn project_files_view(
    project: &Value,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::ScrolledWindow {
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 1);
    rows.add_css_class("cmux-project-content");
    let selected_file = project
        .get("selected_file")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut count = 0;
    for module in project_modules(project) {
        let module_name = module
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or("Project");
        rows.append(&label(module_name, "cmux-heading"));
        if let Some(nodes) = module.get("files").and_then(Value::as_array) {
            let visible_nodes = nodes
                .first()
                .filter(|node| node.get("kind").and_then(Value::as_str) == Some("group"))
                .and_then(|node| node.get("children").and_then(Value::as_array))
                .map(Vec::as_slice)
                .unwrap_or(nodes.as_slice());
            append_project_nodes(
                &rows,
                visible_nodes,
                0,
                selected_file,
                surface_id,
                app_state,
                &mut count,
            );
        }
    }
    gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&rows)
        .build()
}

fn project_targets(project: &Value) -> Vec<&Value> {
    project_modules(project)
        .flat_map(|module| {
            module
                .get("targets")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect()
}

fn project_targets_view(
    project: &Value,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    root.add_css_class("cmux-project-content");
    root.set_hexpand(true);
    root.set_vexpand(true);
    let selected_id = project
        .get("selected_target_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let targets = project_targets(project);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    list.set_size_request(260, -1);
    for target in &targets {
        let id = target.get("id").and_then(Value::as_str).unwrap_or_default();
        let name = target.get("name").and_then(Value::as_str).unwrap_or(id);
        let button = gtk::ToggleButton::with_label(name);
        button.add_css_class("cmux-project-row");
        button.set_active(id == selected_id);
        let app_state = Arc::clone(app_state);
        let surface_id = surface_id.to_string();
        let id = id.to_string();
        button.connect_clicked(move |_| {
            call_app(
                &app_state,
                "project.set_selected_target",
                json!({"surface_id": surface_id, "name": id}),
            );
        });
        list.append(&button);
    }
    root.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&list)
            .build(),
    );
    let details = gtk::Box::new(gtk::Orientation::Vertical, 8);
    details.set_hexpand(true);
    if let Some(target) = targets
        .iter()
        .find(|target| target.get("id").and_then(Value::as_str) == Some(selected_id))
        .or_else(|| targets.first())
    {
        let name = target
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Target");
        details.append(&label(name, "cmux-heading"));
        for (title, key) in [
            ("Product type", "product_type"),
            ("Bundle identifier", "bundle_identifier"),
            ("Deployment target", "deployment_target"),
        ] {
            details.append(&settings_row(
                title,
                target
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("Not specified"),
                None::<&gtk::Widget>,
            ));
        }
        let dependency_count = target
            .get("dependencies")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        details.append(&settings_row(
            "Dependencies",
            &dependency_count.to_string(),
            None::<&gtk::Widget>,
        ));
    }
    root.append(&details);
    root
}

fn project_configurations(project: &Value) -> Vec<&Value> {
    project_modules(project)
        .flat_map(|module| {
            module
                .get("configurations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect()
}

fn project_build_settings_view(
    project: &Value,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("cmux-project-content");
    root.set_hexpand(true);
    root.set_vexpand(true);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let configurations = project_configurations(project);
    let selected_target = project
        .get("selected_target_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = gtk::ComboBoxText::new();
    for item in project_targets(project) {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
        target.append(Some(id), name);
    }
    target.set_active_id(Some(selected_target));
    let app_state_for_target = Arc::clone(app_state);
    let surface_for_target = surface_id.to_string();
    target.connect_changed(move |target| {
        if let Some(id) = target.active_id() {
            call_app(
                &app_state_for_target,
                "project.set_selected_target",
                json!({"surface_id": surface_for_target, "name": id.as_str()}),
            );
        }
    });
    controls.append(&target);
    let selected_configuration = project
        .get("selected_configuration")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let configuration = gtk::ComboBoxText::new();
    let mut names = HashSet::new();
    for item in &configurations {
        if let Some(name) = item.get("name").and_then(Value::as_str) {
            if names.insert(name.to_string()) {
                configuration.append(Some(name), name);
            }
        }
    }
    configuration.set_active_id(Some(selected_configuration));
    let app_state_for_configuration = Arc::clone(app_state);
    let surface_for_configuration = surface_id.to_string();
    configuration.connect_changed(move |configuration| {
        if let Some(name) = configuration.active_id() {
            call_app(
                &app_state_for_configuration,
                "project.set_configuration",
                json!({"surface_id": surface_for_configuration, "name": name.as_str()}),
            );
        }
    });
    controls.append(&configuration);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Filter build settings"));
    search.set_text(
        project
            .get("settings_filter")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    search.set_hexpand(true);
    let app_state_for_search = Arc::clone(app_state);
    let surface_for_search = surface_id.to_string();
    search.connect_search_changed(move |search| {
        call_app(
            &app_state_for_search,
            "project.set_settings_filter",
            json!({"surface_id": surface_for_search, "text": search.text().as_str()}),
        );
    });
    controls.append(&search);
    root.append(&controls);

    let filter = project
        .get("settings_filter")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut effective = std::collections::BTreeMap::<String, (String, String)>::new();
    for item in configurations {
        if item.get("name").and_then(Value::as_str) != Some(selected_configuration) {
            continue;
        }
        let scope = item
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("project");
        if scope == "target"
            && item.get("target_id").and_then(Value::as_str) != Some(selected_target)
        {
            continue;
        }
        if let Some(settings) = item.get("settings").and_then(Value::as_object) {
            for (key, value) in settings {
                if !filter.is_empty() && !key.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                let value = value.as_str().unwrap_or_default().to_string();
                let entry = effective.entry(key.clone()).or_default();
                if scope == "target" {
                    *entry = (value, "Target".to_string());
                } else if entry.0.is_empty() {
                    *entry = (value, "Project".to_string());
                }
            }
        }
    }
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (key, (value, source)) in effective.into_iter().take(600) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("cmux-project-setting");
        let key_label = label(&key, "cmux-heading");
        key_label.set_width_chars(32);
        key_label.set_xalign(0.0);
        row.append(&key_label);
        let value_label = label(&value, "cmux-project-value");
        value_label.set_hexpand(true);
        value_label.set_xalign(0.0);
        value_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&value_label);
        row.append(&label(&source, "cmux-muted"));
        rows.append(&row);
    }
    root.append(
        &gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&rows)
            .build(),
    );
    root
}

fn project_schemes_view(
    project: &Value,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::ScrolledWindow {
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
    rows.add_css_class("cmux-project-content");
    let selected = project
        .get("selected_scheme")
        .and_then(Value::as_str)
        .unwrap_or_default();
    for module in project_modules(project) {
        let module_name = module
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or("Project");
        rows.append(&label(module_name, "cmux-heading"));
        for scheme in module
            .get("schemes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = scheme
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Scheme");
            let shared = scheme
                .get("shared")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let target_count = scheme
                .get("target_ids")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let button = gtk::ToggleButton::with_label(&format!(
                "{}  -  {} / {} targets",
                name,
                if shared { "Shared" } else { "User" },
                target_count
            ));
            button.add_css_class("cmux-project-row");
            button.set_active(name == selected);
            let app_state = Arc::clone(app_state);
            let surface_id = surface_id.to_string();
            let name = name.to_string();
            button.connect_clicked(move |_| {
                call_app(
                    &app_state,
                    "project.set_scheme",
                    json!({"surface_id": surface_id, "name": name}),
                );
            });
            rows.append(&button);
        }
    }
    gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&rows)
        .build()
}

fn project_surface_view(view: &Value, app_state: &Arc<Mutex<AppState>>) -> Option<gtk::Box> {
    let project = view.get("project")?;
    let surface_id = surface_id_or_ref(view)?;
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("cmux-project");
    root.set_hexpand(true);
    root.set_vexpand(true);
    let active_tab = project
        .get("active_tab")
        .and_then(Value::as_str)
        .unwrap_or("files");
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    toolbar.add_css_class("cmux-project-toolbar");
    for (title, tab) in [
        ("Files", "files"),
        ("Targets", "targets"),
        ("Build Settings", "buildSettings"),
        ("Schemes", "schemes"),
    ] {
        toolbar.append(&project_tab_button(
            title,
            tab,
            active_tab,
            &surface_id,
            app_state,
        ));
    }
    root.append(&toolbar);
    if project.get("load_state").and_then(Value::as_str) == Some("failed") {
        root.append(&label(
            project
                .get("load_error")
                .and_then(Value::as_str)
                .unwrap_or("Failed to load project"),
            "cmux-muted",
        ));
        return Some(root);
    }
    match active_tab {
        "targets" => root.append(&project_targets_view(project, &surface_id, app_state)),
        "buildSettings" => root.append(&project_build_settings_view(
            project,
            &surface_id,
            app_state,
        )),
        "schemes" => root.append(&project_schemes_view(project, &surface_id, app_state)),
        _ => root.append(&project_files_view(project, &surface_id, app_state)),
    }
    Some(root)
}

fn settings_row(title: &str, detail: &str, control: Option<&impl IsA<gtk::Widget>>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row.add_css_class("cmux-settings-row");
    row.set_hexpand(true);
    let copy = gtk::Box::new(gtk::Orientation::Vertical, 3);
    copy.set_hexpand(true);
    copy.append(&label(title, "cmux-heading"));
    let detail = label(detail, "cmux-muted");
    detail.set_wrap(true);
    detail.set_xalign(0.0);
    copy.append(&detail);
    row.append(&copy);
    if let Some(control) = control {
        control.add_css_class("cmux-settings-control");
        row.append(control);
    }
    row
}

fn settings_action_button(
    title: &str,
    app_state: &Arc<Mutex<AppState>>,
    method: &'static str,
    params: Value,
) -> gtk::Button {
    let button = gtk::Button::with_label(title);
    let app_state = Arc::clone(app_state);
    button.connect_clicked(move |_| {
        call_app(&app_state, method, params.clone());
    });
    button
}

fn linux_shortcut_combo_text(combo: &str) -> String {
    combo
        .split_whitespace()
        .map(|stroke| {
            stroke
                .split('+')
                .map(|part| if part == "cmd" { "super" } else { part })
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn shortcut_editor_controls(row: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.set_valign(gtk::Align::Center);
    let name = value_str(row, "name", "").to_string();
    let combo = value_str(row, "combo", "").to_string();
    let default_combo = value_str(row, "default_combo", "").to_string();

    let entry = gtk::Entry::new();
    entry.set_width_chars(18);
    entry.set_max_width_chars(24);
    entry.set_placeholder_text(Some("Unbound"));
    entry.set_text(&linux_shortcut_combo_text(&combo));
    let entry_state = Arc::clone(app_state);
    let entry_name = name.clone();
    entry.connect_activate(move |entry| {
        let combo = entry.text().trim().to_string();
        if combo.is_empty() {
            return;
        }
        call_app(
            &entry_state,
            "settings.shortcut.set",
            json!({"name": entry_name, "combo": combo}),
        );
    });
    controls.append(&entry);

    let clear = gtk::Button::from_icon_name("edit-clear-symbolic");
    clear.set_tooltip_text(Some("Unbind shortcut"));
    let clear_state = Arc::clone(app_state);
    let clear_name = name.clone();
    let clear_entry = entry.clone();
    clear.connect_clicked(move |_| {
        if call_app(
            &clear_state,
            "settings.shortcut.set",
            json!({"name": clear_name, "combo": "clear"}),
        ) {
            clear_entry.set_text("");
        }
    });
    controls.append(&clear);

    let reset = gtk::Button::from_icon_name("view-refresh-symbolic");
    reset.set_tooltip_text(Some("Restore default shortcut"));
    let reset_state = Arc::clone(app_state);
    let reset_name = name;
    let reset_entry = entry;
    reset.connect_clicked(move |_| {
        if call_app(
            &reset_state,
            "settings.shortcut.set",
            json!({"name": reset_name, "combo": "reset"}),
        ) {
            reset_entry.set_text(&linux_shortcut_combo_text(&default_combo));
        }
    });
    controls.append(&reset);
    controls
}

fn shortcut_settings_detail(row: &Value) -> String {
    let description = value_str(row, "description", "");
    let when = row.get("when").and_then(Value::as_str).unwrap_or_default();
    if when.is_empty() {
        description.to_string()
    } else if description.is_empty() {
        format!("When: {when}")
    } else {
        format!("{description}\nWhen: {when}")
    }
}

fn workspace_color_swatch(color: &str) -> gtk::Box {
    let swatch = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    swatch.add_css_class("cmux-color-swatch");
    swatch.set_size_request(24, 24);
    if valid_hex_color(color) {
        install_custom_sidebar_style(swatch.upcast_ref(), &format!("background: {color};"));
    } else {
        install_custom_sidebar_style(
            swatch.upcast_ref(),
            "background: transparent; border: 1px dashed #727b82;",
        );
    }
    swatch
}

fn workspace_color_is_builtin(name: &str) -> bool {
    config::WORKSPACE_COLOR_DEFAULT_PALETTE
        .iter()
        .any(|(default_name, _)| *default_name == name)
}

fn sidebar_setting_switch(
    status: &Value,
    key: &'static str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Switch {
    let control = gtk::Switch::new();
    control.set_active(status.get(key).and_then(Value::as_bool).unwrap_or(false));
    let state = Arc::clone(app_state);
    control.connect_active_notify(move |control| {
        call_app(
            &state,
            "settings.sidebar.set",
            json!({"key": key, "value": control.is_active()}),
        );
    });
    control
}

fn beta_feature_setting_switch(
    status: &Value,
    key: &'static str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Switch {
    let control = gtk::Switch::new();
    control.set_active(status.get(key).and_then(Value::as_bool).unwrap_or(false));
    let state = Arc::clone(app_state);
    control.connect_active_notify(move |control| {
        call_app(
            &state,
            "settings.beta_features.set",
            json!({"key": key, "value": control.is_active()}),
        );
    });
    control
}

fn settings_content(
    target: &str,
    section_title: &str,
    app_state: &Arc<Mutex<AppState>>,
) -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.add_css_class("cmux-settings-content");
    content.set_hexpand(true);
    content.set_vexpand(true);
    let snapshot = config::snapshot();

    match target {
        "account" => {
            content.append(&label("Account", "cmux-heading"));
            let status =
                call_app_value(app_state, "auth.status", json!({})).unwrap_or_else(|| json!({}));
            let signed_in = status
                .get("signed_in")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if signed_in {
                let user = status.get("user").cloned().unwrap_or(Value::Null);
                let email = value_str(&user, "email", "");
                let display_name = value_str(&user, "display_name", "");
                let identity = if !email.is_empty() && !display_name.is_empty() {
                    format!("{display_name}\n{email}")
                } else if !email.is_empty() {
                    email.to_string()
                } else if !display_name.is_empty() {
                    display_name.to_string()
                } else {
                    "Signed-in cmux account".to_string()
                };
                let sign_out =
                    settings_action_button("Sign Out", app_state, "auth.sign_out", json!({}));
                content.append(&settings_row("Signed in", &identity, Some(&sign_out)));

                let teams = status
                    .get("teams")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !teams.is_empty() {
                    let selector = gtk::ComboBoxText::new();
                    selector.append(Some("__none"), "None");
                    for team in &teams {
                        let id = value_str(team, "id", "");
                        if id.is_empty() {
                            continue;
                        }
                        let display_name = value_str(team, "display_name", id);
                        selector.append(Some(id), display_name);
                    }
                    selector.set_active_id(Some(
                        status
                            .get("selected_team_id")
                            .and_then(Value::as_str)
                            .unwrap_or("__none"),
                    ));
                    let team_state = Arc::clone(app_state);
                    selector.connect_changed(move |selector| {
                        let Some(team_id) = selector.active_id() else {
                            return;
                        };
                        let team_id = if team_id.as_str() == "__none" {
                            Value::Null
                        } else {
                            json!(team_id.as_str())
                        };
                        call_app(&team_state, "auth.team.select", json!({"team_id": team_id}));
                    });
                    content.append(&settings_row(
                        "Active team",
                        "Cloud VM and team-scoped requests use this team.",
                        Some(&selector),
                    ));
                }
            } else {
                let sign_in =
                    settings_action_button("Sign In", app_state, "auth.begin_sign_in", json!({}));
                content.append(&settings_row(
                    "Not signed in",
                    "Sign in to use team-scoped cloud and synchronization features.",
                    Some(&sign_in),
                ));
            }
            let credential_store = value_str(&status, "credential_store", "file");
            let credential_detail = status
                .get("credential_store_fallback_reason")
                .and_then(Value::as_str)
                .map(|reason| format!("{credential_store}\n{reason}"))
                .unwrap_or_else(|| credential_store.to_string());
            content.append(&settings_row(
                "Credential storage",
                &credential_detail,
                None::<&gtk::Widget>,
            ));
        }
        "general" | "app" => {
            content.append(&label("General", "cmux-heading"));
            content.append(&settings_row(
                "cmux configuration",
                &snapshot.cmux.path,
                None::<&gtk::Widget>,
            ));
            content.append(&settings_row(
                "Ghostty configuration",
                &snapshot.ghostty.path,
                None::<&gtk::Widget>,
            ));
            let reload = settings_action_button("Reload", app_state, "config.reload", json!({}));
            content.append(&settings_row(
                "Reload configuration",
                "Re-read cmux and Ghostty settings for all windows.",
                Some(&reload),
            ));
            let app_settings = call_app_value(app_state, "settings.app.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let placement = gtk::ComboBoxText::new();
            placement.append(Some("top"), "Top");
            placement.append(Some("afterCurrent"), "After current");
            placement.append(Some("end"), "End");
            placement.set_active_id(Some(value_str(
                &app_settings,
                "newWorkspacePlacement",
                "afterCurrent",
            )));
            let placement_state = Arc::clone(app_state);
            placement.connect_changed(move |selector| {
                if let Some(value) = selector.active_id() {
                    call_app(
                        &placement_state,
                        "settings.app.set",
                        json!({"key": "newWorkspacePlacement", "value": value.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "New workspace placement",
                "Choose where ungrouped workspaces created from the app are inserted.",
                Some(&placement),
            ));
            let inherit_cwd = gtk::Switch::new();
            inherit_cwd.set_active(
                app_settings
                    .get("workspaceInheritWorkingDirectory")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            );
            let inherit_cwd_state = Arc::clone(app_state);
            inherit_cwd.connect_active_notify(move |switch| {
                call_app(
                    &inherit_cwd_state,
                    "settings.app.set",
                    json!({
                        "key": "workspaceInheritWorkingDirectory",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Inherit workspace directory",
                "Start new workspaces in the active workspace's current directory.",
                Some(&inherit_cwd),
            ));
            let keep_workspace_open = gtk::Switch::new();
            keep_workspace_open.set_active(
                app_settings
                    .get("keepWorkspaceOpenWhenClosingLastSurface")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let keep_workspace_open_state = Arc::clone(app_state);
            keep_workspace_open.connect_active_notify(move |switch| {
                call_app(
                    &keep_workspace_open_state,
                    "settings.app.set",
                    json!({
                        "key": "keepWorkspaceOpenWhenClosingLastSurface",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Keep workspace open on last close",
                "Replace the last closed surface with a new terminal instead of closing the workspace.",
                Some(&keep_workspace_open),
            ));
            let confirm_quit = gtk::ComboBoxText::new();
            confirm_quit.append(Some("always"), "Always");
            confirm_quit.append(Some("dirty-only"), "Only when terminals are busy");
            confirm_quit.append(Some("never"), "Never");
            confirm_quit.set_active_id(Some(value_str(&app_settings, "confirmQuit", "always")));
            let confirm_quit_state = Arc::clone(app_state);
            confirm_quit.connect_changed(move |selector| {
                if let Some(value) = selector.active_id() {
                    call_app(
                        &confirm_quit_state,
                        "settings.app.set",
                        json!({"key": "confirmQuit", "value": value.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "Confirm before quitting",
                "Choose when closing the final window asks before quitting cmux.",
                Some(&confirm_quit),
            ));

            let warn_close_tab = gtk::Switch::new();
            warn_close_tab.set_active(
                app_settings
                    .get("warnBeforeClosingTab")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            );
            let warn_close_tab_state = Arc::clone(app_state);
            warn_close_tab.connect_active_notify(move |switch| {
                call_app(
                    &warn_close_tab_state,
                    "settings.app.set",
                    json!({
                        "key": "warnBeforeClosingTab",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Warn before closing busy tabs",
                "Ask before a shortcut closes a terminal with a running process.",
                Some(&warn_close_tab),
            ));

            let warn_close_button = gtk::Switch::new();
            warn_close_button.set_active(
                app_settings
                    .get("warnBeforeClosingTabXButton")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let warn_close_button_state = Arc::clone(app_state);
            warn_close_button.connect_active_notify(move |switch| {
                call_app(
                    &warn_close_button_state,
                    "settings.app.set",
                    json!({
                        "key": "warnBeforeClosingTabXButton",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Always warn for the tab close button",
                "Ask before closing a tab from its close button, even when the terminal is idle.",
                Some(&warn_close_button),
            ));

            let hide_close_button = gtk::Switch::new();
            hide_close_button.set_active(
                app_settings
                    .get("hideTabCloseButton")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let hide_close_button_state = Arc::clone(app_state);
            hide_close_button.connect_active_notify(move |switch| {
                call_app(
                    &hide_close_button_state,
                    "settings.app.set",
                    json!({
                        "key": "hideTabCloseButton",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Hide tab close button",
                "Remove the close button from the pane tab strip.",
                Some(&hide_close_button),
            ));
            let canvas = config::canvas_settings();
            let gap = gtk::SpinButton::with_range(0.0, 64.0, 1.0);
            gap.set_value(canvas.pane_gap);
            let gap_app_state = Arc::clone(app_state);
            gap.connect_value_changed(move |gap| {
                if config::set_canvas_pane_gap(gap.value()).is_ok() {
                    call_app(&gap_app_state, "config.reload", json!({}));
                }
            });
            content.append(&settings_row(
                "Canvas pane gap",
                "Spacing used by pane placement and snapping.",
                Some(&gap),
            ));
            let snapping = gtk::Switch::new();
            snapping.set_active(canvas.snapping_enabled);
            let snapping_app_state = Arc::clone(app_state);
            snapping.connect_active_notify(move |snapping| {
                if config::set_canvas_snapping_enabled(snapping.is_active()).is_ok() {
                    call_app(&snapping_app_state, "config.reload", json!({}));
                }
            });
            content.append(&settings_row(
                "Canvas snapping",
                "Align pane edges, gaps, and centers while moving or resizing.",
                Some(&snapping),
            ));
        }
        "terminal" => {
            content.append(&label("Terminal", "cmux-heading"));
            let terminal_settings =
                call_app_value(app_state, "settings.terminal.status", json!({}))
                    .unwrap_or_else(|| json!({}));
            let show_scroll_bar = gtk::Switch::new();
            show_scroll_bar.set_active(
                terminal_settings
                    .get("showScrollBar")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            );
            let show_scroll_bar_state = Arc::clone(app_state);
            show_scroll_bar.connect_active_notify(move |switch| {
                call_app(
                    &show_scroll_bar_state,
                    "settings.terminal.set",
                    json!({
                        "key": "showScrollBar",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Show terminal scroll bar",
                "Show an overlay scroll bar when the terminal has scrollback.",
                Some(&show_scroll_bar),
            ));

            let copy_on_select = gtk::Switch::new();
            copy_on_select.set_active(
                terminal_settings
                    .get("copyOnSelect")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let copy_on_select_state = Arc::clone(app_state);
            copy_on_select.connect_active_notify(move |switch| {
                call_app(
                    &copy_on_select_state,
                    "settings.terminal.set",
                    json!({
                        "key": "copyOnSelect",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Copy on selection",
                "Copy committed terminal selections to the system clipboard.",
                Some(&copy_on_select),
            ));

            let auto_resume_agents = gtk::Switch::new();
            auto_resume_agents.set_active(
                terminal_settings
                    .get("autoResumeAgentSessions")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            );
            let auto_resume_state = Arc::clone(app_state);
            auto_resume_agents.connect_active_notify(move |switch| {
                call_app(
                    &auto_resume_state,
                    "settings.terminal.set",
                    json!({
                        "key": "autoResumeAgentSessions",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Resume agent sessions on reopen",
                "Restart agent-hook and native agent sessions that were running when cmux quit.",
                Some(&auto_resume_agents),
            ));

            let hibernation = call_app_value(app_state, "agent.hibernation.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let hibernation_enabled = gtk::Switch::new();
            hibernation_enabled.set_active(
                hibernation
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let hibernation_enabled_state = Arc::clone(app_state);
            hibernation_enabled.connect_active_notify(move |switch| {
                call_app(
                    &hibernation_enabled_state,
                    "agent.hibernation.set",
                    json!({"enabled": switch.is_active()}),
                );
            });
            content.append(&settings_row(
                "Agent Hibernation",
                "Suspend idle background agent terminals after the live-terminal limit is exceeded.",
                Some(&hibernation_enabled),
            ));

            let idle_seconds = gtk::SpinButton::with_range(5.0, 604_800.0, 1.0);
            idle_seconds.set_value(
                hibernation
                    .get("idle_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(5) as f64,
            );
            idle_seconds.set_width_chars(8);
            let idle_seconds_state = Arc::clone(app_state);
            idle_seconds.connect_value_changed(move |spin| {
                call_app(
                    &idle_seconds_state,
                    "agent.hibernation.set",
                    json!({"idle_seconds": spin.value_as_int().max(5)}),
                );
            });
            content.append(&settings_row(
                "Hibernate after idle seconds",
                "Output, input, lifecycle, and process identity must remain stable through the confirmation window.",
                Some(&idle_seconds),
            ));

            let max_live = gtk::SpinButton::with_range(1.0, 256.0, 1.0);
            max_live.set_value(
                hibernation
                    .get("max_live_terminals")
                    .and_then(Value::as_u64)
                    .unwrap_or(12) as f64,
            );
            max_live.set_width_chars(5);
            let max_live_state = Arc::clone(app_state);
            max_live.connect_value_changed(move |spin| {
                call_app(
                    &max_live_state,
                    "agent.hibernation.set",
                    json!({"max_live_terminals": spin.value_as_int().max(1)}),
                );
            });
            content.append(&settings_row(
                "Max live agent terminals",
                "Visible terminals stay live; excess idle background terminals hibernate oldest first.",
                Some(&max_live),
            ));

            let resume_status =
                call_app_value(app_state, "settings.terminal.resume.list", json!({}))
                    .unwrap_or_else(|| json!({"records": []}));
            let resume_records = resume_status
                .get("records")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let has_resume_records = !resume_records.is_empty();
            content.append(&label("Resume Commands", "cmux-heading"));
            if !has_resume_records {
                content.append(&row_label(
                    "No approved resume command prefixes",
                    "cmux-muted",
                ));
            }
            for record in resume_records {
                let record_id = value_str(&record, "id", "").to_string();
                let title = value_string(&record, "name")
                    .or_else(|| value_string(&record, "commandPrefixText"))
                    .unwrap_or_else(|| "Resume command".to_string());
                let cwd = value_string(&record, "cwd")
                    .map(|cwd| format!("Working directory: {cwd}"))
                    .unwrap_or_else(|| "Any saved working directory".to_string());
                let valid = record
                    .get("validSignature")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let description = if valid {
                    cwd
                } else {
                    "Signature invalid. Delete and approve this command again.".to_string()
                };
                let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let policy = gtk::ComboBoxText::new();
                policy.append(Some("manual"), "Manual");
                policy.append(Some("prompt"), "Ask");
                policy.append(Some("auto"), "Auto");
                policy.set_active_id(Some(value_str(&record, "policy", "manual")));
                policy.set_sensitive(valid);
                let policy_state = Arc::clone(app_state);
                let policy_record_id = record_id.clone();
                policy.connect_changed(move |selector| {
                    let Some(policy) = selector.active_id() else {
                        return;
                    };
                    call_app(
                        &policy_state,
                        "settings.terminal.resume.update",
                        json!({
                            "id": policy_record_id,
                            "policy": policy.as_str()
                        }),
                    );
                });
                controls.append(&policy);
                let delete = gtk::Button::from_icon_name("user-trash-symbolic");
                delete.add_css_class("cmux-icon-action");
                delete.set_tooltip_text(Some("Delete resume command approval"));
                let delete_state = Arc::clone(app_state);
                delete.connect_clicked(move |_| {
                    call_app(
                        &delete_state,
                        "settings.terminal.resume.delete",
                        json!({"id": record_id}),
                    );
                });
                controls.append(&delete);
                content.append(&settings_row(&title, &description, Some(&controls)));
            }
            if has_resume_records {
                let clear = gtk::Button::from_icon_name("edit-clear-symbolic");
                clear.add_css_class("cmux-icon-action");
                clear.set_tooltip_text(Some("Delete all resume command approvals"));
                let clear_state = Arc::clone(app_state);
                clear.connect_clicked(move |_| {
                    call_app(&clear_state, "settings.terminal.resume.clear", json!({}));
                });
                content.append(&settings_row(
                    "Clear resume commands",
                    "Remove every signed command-prefix approval.",
                    Some(&clear),
                ));
            }

            let themes = config::themes_list_payload();
            let selector = gtk::ComboBoxText::new();
            selector.append(Some("__ghostty_default"), "Ghostty default");
            for theme in &themes.themes {
                selector.append(Some(&theme.name), &theme.name);
            }
            if let Some(current) = themes.current.dark.or(themes.current.light) {
                selector.set_active_id(Some(&current));
            } else {
                selector.set_active_id(Some("__ghostty_default"));
            }
            selector.connect_changed(move |selector| {
                if let Some(theme) = selector.active_id() {
                    if theme.as_str() == "__ghostty_default" {
                        let _ = config::clear_theme_override();
                    } else {
                        let theme = theme.to_string();
                        let _ = config::set_theme_override(Some(theme.clone()), Some(theme));
                    }
                }
            });
            content.append(&settings_row(
                "Terminal theme",
                &themes.config_path,
                Some(&selector),
            ));
            let clear = gtk::Button::with_label("Use Ghostty default");
            let reload_state = Arc::clone(app_state);
            clear.connect_clicked(move |_| {
                let _ = config::clear_theme_override();
                call_app(&reload_state, "config.reload", json!({}));
            });
            content.append(&settings_row(
                "Theme override",
                "Clear the cmux-managed light and dark theme override.",
                Some(&clear),
            ));
        }
        "sidebarAppearance" => {
            content.append(&label("Sidebar", "cmux-heading"));
            let status = call_app_value(app_state, "settings.sidebar.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let match_terminal =
                sidebar_setting_switch(&status, "matchTerminalBackground", app_state);
            content.append(&settings_row(
                "Match terminal background",
                "Use the window background behind the left sidebar instead of the default sidebar fill.",
                Some(&match_terminal),
            ));
            for (key, title) in [
                ("sidebar-font-size", "Sidebar font size"),
                ("surface-tab-bar-font-size", "Surface tab font size"),
            ] {
                let value = config::get_font_size(key).ok();
                let spin = gtk::SpinButton::with_range(8.0, 32.0, 0.5);
                spin.set_digits(1);
                spin.set_value(value.as_ref().map(|value| value.value).unwrap_or(13.0));
                let app_state = Arc::clone(app_state);
                spin.connect_value_changed(move |spin| {
                    let raw = spin.value().to_string();
                    if config::set_font_size(key, &raw, "all".to_string(), None).is_ok() {
                        call_app(&app_state, "config.reload", json!({}));
                    }
                });
                content.append(&settings_row(
                    title,
                    value
                        .as_ref()
                        .map(|value| value.path.as_str())
                        .unwrap_or("cmux.json"),
                    Some(&spin),
                ));
            }
            let hide_all = sidebar_setting_switch(&status, "hideAllDetails", app_state);
            content.append(&settings_row(
                "Hide all sidebar details",
                "Show only workspace titles and unread indicators.",
                Some(&hide_all),
            ));
            let wrap_titles = sidebar_setting_switch(&status, "wrapWorkspaceTitles", app_state);
            content.append(&settings_row(
                "Wrap workspace titles",
                "Allow long workspace titles to use multiple lines.",
                Some(&wrap_titles),
            ));
            let descriptions =
                sidebar_setting_switch(&status, "showWorkspaceDescription", app_state);
            content.append(&settings_row(
                "Show workspace descriptions",
                "Show the custom Markdown description beneath each workspace title.",
                Some(&descriptions),
            ));
            let branch_layout = gtk::ComboBoxText::new();
            branch_layout.append(Some("vertical"), "Vertical");
            branch_layout.append(Some("inline"), "Inline");
            branch_layout.set_active_id(Some(value_str(&status, "branchLayout", "vertical")));
            let branch_state = Arc::clone(app_state);
            branch_layout.connect_changed(move |selector| {
                if let Some(value) = selector.active_id() {
                    call_app(
                        &branch_state,
                        "settings.sidebar.set",
                        json!({"key": "branchLayout", "value": value.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "Branch and directory layout",
                "Render git branch and working-directory information on separate lines or inline.",
                Some(&branch_layout),
            ));
            for (key, title, detail) in [
                (
                    "stackBranchDirectory",
                    "Stack branch and directory",
                    "Keep branch and directory on separate lines even when Inline layout is selected.",
                ),
                (
                    "pathLastSegmentOnly",
                    "Show final directory only",
                    "Display only the final path component in workspace rows.",
                ),
                (
                    "showNotificationMessage",
                    "Show notification message",
                    "Display the latest desktop or agent notification text.",
                ),
                (
                    "showBranchDirectory",
                    "Show branch and directory",
                    "Display git branch and workspace working-directory information.",
                ),
                (
                    "watchGitStatus",
                    "Read git status",
                    "Refresh branch names and dirty state from each workspace repository.",
                ),
                (
                    "showPullRequests",
                    "Show pull requests",
                    "Display pull request links when workspace metadata provides them.",
                ),
                (
                    "makePullRequestsClickable",
                    "Make pull requests clickable",
                    "Allow pull request rows to open their destination.",
                ),
                (
                    "openPullRequestLinksInCmuxBrowser",
                    "Open pull requests in cmux",
                    "Open sidebar pull request links in an embedded browser surface.",
                ),
                (
                    "openPortLinksInCmuxBrowser",
                    "Open ports in cmux",
                    "Open listening-port buttons in an embedded browser surface.",
                ),
                (
                    "showSSH",
                    "Show SSH target",
                    "Display the remote destination for SSH-backed workspaces.",
                ),
                (
                    "showPorts",
                    "Show listening ports",
                    "Display detected remote listening ports as openable buttons.",
                ),
                (
                    "showLog",
                    "Show latest log",
                    "Display the latest sidebar log entry.",
                ),
                (
                    "showProgress",
                    "Show progress",
                    "Display workspace progress reported through the sidebar API.",
                ),
                (
                    "showCustomMetadata",
                    "Show custom metadata",
                    "Display status entries and metadata blocks reported through the sidebar API.",
                ),
            ] {
                let control = sidebar_setting_switch(&status, key, app_state);
                content.append(&settings_row(title, detail, Some(&control)));
            }

            let width_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let width_enabled = gtk::Switch::new();
            let configured_width = status.get("rightMaxWidth").and_then(Value::as_f64);
            width_enabled.set_active(configured_width.is_some());
            let width = gtk::SpinButton::with_range(276.0, 4096.0, 10.0);
            width.set_digits(0);
            width.set_value(configured_width.unwrap_or(300.0));
            width.set_sensitive(configured_width.is_some());
            let enabled_state = Arc::clone(app_state);
            let enabled_width = width.clone();
            width_enabled.connect_active_notify(move |control| {
                enabled_width.set_sensitive(control.is_active());
                let value = if control.is_active() {
                    json!(enabled_width.value())
                } else {
                    Value::Null
                };
                call_app(
                    &enabled_state,
                    "settings.sidebar.set",
                    json!({"key": "rightMaxWidth", "value": value}),
                );
            });
            let width_state = Arc::clone(app_state);
            let width_toggle = width_enabled.clone();
            width.connect_value_changed(move |control| {
                if width_toggle.is_active() {
                    call_app(
                        &width_state,
                        "settings.sidebar.set",
                        json!({"key": "rightMaxWidth", "value": control.value()}),
                    );
                }
            });
            width_controls.append(&width_enabled);
            width_controls.append(&width);
            content.append(&settings_row(
                "Right sidebar width",
                "Override the default width used by Files, Find, Vault, Feed, and Dock.",
                Some(&width_controls),
            ));
        }
        "textBox" => {
            content.append(&label("TextBox (Beta)", "cmux-heading"));
            let settings = call_app_value(app_state, "settings.textbox.status", json!({}))
                .unwrap_or_else(|| json!({}));

            let show = gtk::Switch::new();
            show.set_active(
                settings
                    .get("showTextBoxOnNewTerminals")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let show_state = Arc::clone(app_state);
            show.connect_active_notify(move |switch| {
                call_app(
                    &show_state,
                    "settings.textbox.set",
                    json!({
                        "key": "showTextBoxOnNewTerminals",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Show on new terminals",
                "Open the composer when a terminal is created.",
                Some(&show),
            ));

            let focus = gtk::Switch::new();
            focus.set_active(
                settings
                    .get("focusTextBoxOnNewTerminals")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let focus_state = Arc::clone(app_state);
            focus.connect_active_notify(move |switch| {
                call_app(
                    &focus_state,
                    "settings.textbox.set",
                    json!({
                        "key": "focusTextBoxOnNewTerminals",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Focus on new terminals",
                "Place keyboard focus in the composer when it opens.",
                Some(&focus),
            ));

            let max_lines = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
            max_lines.set_digits(0);
            max_lines.set_value(
                settings
                    .get("textBoxMaxLines")
                    .and_then(Value::as_f64)
                    .unwrap_or(10.0),
            );
            let lines_state = Arc::clone(app_state);
            max_lines.connect_value_changed(move |spin| {
                call_app(
                    &lines_state,
                    "settings.textbox.set",
                    json!({"key": "textBoxMaxLines", "value": spin.value_as_int()}),
                );
            });
            content.append(&settings_row(
                "Maximum lines",
                "Maximum composer height before scrolling.",
                Some(&max_lines),
            ));
        }
        "customSidebars" => {
            content.append(&label("Custom Sidebars", "cmux-heading"));
            let status = call_app_value(app_state, "settings.custom_sidebars.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let beta_status = call_app_value(app_state, "settings.beta_features.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let enabled = beta_feature_setting_switch(&beta_status, "customSidebars", app_state);
            content.append(&settings_row(
                "Show Custom Sidebars",
                "List user-authored sidebars in the sidebar provider picker.",
                Some(&enabled),
            ));
            let renderer = gtk::ComboBoxText::new();
            renderer.append(Some("inProcess"), "In process");
            renderer.append(Some("remote"), "Remote worker");
            renderer.set_active_id(Some(value_str(&status, "renderer", "inProcess")));
            let renderer_state = Arc::clone(app_state);
            renderer.connect_changed(move |selector| {
                if let Some(renderer) = selector.active_id() {
                    call_app(
                        &renderer_state,
                        "settings.custom_sidebars.set_renderer",
                        json!({"renderer": renderer.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "Renderer",
                "Use the remote worker to isolate parsing and evaluation from the app process.",
                Some(&renderer),
            ));
            content.append(&settings_row(
                "Sidebar directory",
                value_str(&status, "directory", "~/.config/cmux/sidebars"),
                None::<&gtk::Widget>,
            ));
        }
        "betaFeatures" => {
            content.append(&label("Beta Features", "cmux-heading"));
            let status = call_app_value(app_state, "settings.beta_features.status", json!({}))
                .unwrap_or_else(|| json!({}));
            content.append(&settings_row(
                "Feed",
                "Show Feed in the right sidebar for agent decisions and activity.",
                Some(&beta_feature_setting_switch(
                    &status,
                    "rightSidebarFeed",
                    app_state,
                )),
            ));
            content.append(&settings_row(
                "Dock",
                "Show Dock in the right sidebar for terminal controls.",
                Some(&beta_feature_setting_switch(
                    &status,
                    "rightSidebarDock",
                    app_state,
                )),
            ));

            let extensions = gtk::Switch::new();
            extensions.set_active(
                status
                    .get("extensions")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let extensions_state = Arc::clone(app_state);
            extensions.connect_active_notify(move |switch| {
                call_app(
                    &extensions_state,
                    "settings.beta_features.set",
                    json!({
                        "key": "extensions.beta.enabled",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Extensions",
                "Run installed sidebar extensions in a bounded Bubblewrap host.",
                Some(&extensions),
            ));
            content.append(&settings_row(
                "Custom Sidebars",
                "Show interpreted JSON and Swift-style sidebars in the provider picker.",
                Some(&beta_feature_setting_switch(
                    &status,
                    "customSidebars",
                    app_state,
                )),
            ));
            content.append(&settings_row(
                "Remote tmux",
                "Enable the experimental remote tmux socket and CLI entry points.",
                Some(&beta_feature_setting_switch(
                    &status,
                    "remoteTmux",
                    app_state,
                )),
            ));
        }
        "mobile" => {
            content.append(&label("Mobile", "cmux-heading"));
            let status = call_app_value(app_state, "mobile.host.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let display_name = value_str(&status, "display_name", "Linux host");
            let device_id = value_str(&status, "device_id", "unknown");
            content.append(&settings_row(
                "This host",
                &format!("{display_name}\nDevice ID: {device_id}"),
                None::<&gtk::Widget>,
            ));

            let service = status.get("host_service").cloned().unwrap_or(Value::Null);
            let running = service
                .get("is_running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let configured_port = service
                .get("configured_port")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let service_detail = if running {
                format!("Available on configured mobile routes. Port {configured_port}.")
            } else {
                "No mobile route is configured. Configure a Tailscale route or enable the debug loopback route.".to_string()
            };
            content.append(&settings_row(
                "Mobile host service",
                &service_detail,
                None::<&gtk::Widget>,
            ));

            let routes = service
                .get("routes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for route in &routes {
                let id = value_str(route, "id", "route");
                let kind = value_str(route, "kind", "route");
                let endpoint = route.get("endpoint").cloned().unwrap_or(Value::Null);
                let host = value_str(&endpoint, "host", "");
                let port = endpoint.get("port").and_then(Value::as_u64).unwrap_or(0);
                content.append(&settings_row(
                    &format!("Route: {id}"),
                    &format!("{kind} - {host}:{port}"),
                    None::<&gtk::Widget>,
                ));
            }

            let pairing = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let copy = gtk::Button::with_label("Copy Pairing Link");
            copy.set_sensitive(!routes.is_empty());
            let pairing_status = label("", "cmux-muted");
            let copy_state = Arc::clone(app_state);
            let copy_status = pairing_status.clone();
            copy.connect_clicked(move |_| {
                let Some(ticket) = call_app_value(
                    &copy_state,
                    "mobile.attach_ticket.create",
                    json!({"scope": "linux"}),
                ) else {
                    copy_status.set_text("Pairing link unavailable");
                    return;
                };
                let Some(url) = ticket.get("attach_url").and_then(Value::as_str) else {
                    copy_status.set_text("Pairing link unavailable");
                    return;
                };
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(url);
                    copy_status.set_text("Copied");
                } else {
                    copy_status.set_text("Clipboard unavailable");
                }
            });
            pairing.append(&copy);
            pairing.append(&pairing_status);
            content.append(&settings_row(
                "Pair an iPhone or iPad",
                "Creates a short-lived host-wide attach link using the configured routes.",
                Some(&pairing),
            ));

            let workspace_count = status
                .get("workspace_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let fidelity = value_str(&status, "terminal_fidelity", "render_grid");
            content.append(&settings_row(
                "Published session state",
                &format!("{workspace_count} workspaces - terminal fidelity: {fidelity}"),
                None::<&gtk::Widget>,
            ));
        }
        "automation" => {
            content.append(&label("Automation", "cmux-heading"));
            content.append(&settings_row(
                "Control socket",
                &std::env::var("CMUX_SOCKET_PATH")
                    .or_else(|_| std::env::var("CMUX_SOCKET"))
                    .unwrap_or_else(|_| {
                        "~/.local/state/cmux/cmux.sock (default launcher path)".to_string()
                    }),
                None::<&gtk::Widget>,
            ));
            for (title, detail, method) in [
                (
                    "Claude Code",
                    "Install or update cmux notification and Feed hooks.",
                    "integration.claude.open_installer",
                ),
                (
                    "Codex",
                    "Install or update cmux hooks for Codex sessions.",
                    "integration.codex.open_installer",
                ),
                (
                    "OpenCode",
                    "Install or update the cmux OpenCode integration.",
                    "integration.opencode.open_installer",
                ),
            ] {
                let install = settings_action_button("Configure", app_state, method, json!({}));
                content.append(&settings_row(title, detail, Some(&install)));
            }
            content.append(&settings_row(
                "Socket API",
                "Use `cmux capabilities --json` to inspect the methods exposed by the running app.",
                None::<&gtk::Widget>,
            ));
        }
        "workspaceColors" => {
            content.append(&label("Workspace Colors", "cmux-heading"));
            let status = call_app_value(app_state, "settings.workspace_colors.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let indicator = gtk::ComboBoxText::new();
            indicator.append(Some("leftRail"), "Left Rail");
            indicator.append(Some("solidFill"), "Solid Fill");
            indicator.set_active_id(Some(value_str(&status, "indicatorStyle", "leftRail")));
            let indicator_state = Arc::clone(app_state);
            indicator.connect_changed(move |selector| {
                if let Some(style) = selector.active_id() {
                    call_app(
                        &indicator_state,
                        "settings.workspace_colors.set",
                        json!({"key": "indicatorStyle", "value": style.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "Workspace Color Indicator",
                "Choose how each workspace's assigned color appears in the sidebar.",
                Some(&indicator),
            ));

            for (key, title, detail) in [
                (
                    "selectionColor",
                    "Selection Highlight",
                    "Background color of the selected workspace when no solid workspace fill is active.",
                ),
                (
                    "notificationBadgeColor",
                    "Notification Badge",
                    "Color of the unread notification marker on workspace rows.",
                ),
            ] {
                let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let current = status
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let swatch = workspace_color_swatch(current);
                controls.append(&swatch);
                let entry = gtk::Entry::new();
                entry.set_width_chars(9);
                entry.set_max_width_chars(9);
                entry.set_placeholder_text(Some("Default"));
                entry.set_text(current);
                let entry_state = Arc::clone(app_state);
                let entry_key = key.to_string();
                entry.connect_activate(move |entry| {
                    let value = entry.text();
                    let value = if value.trim().is_empty() {
                        Value::Null
                    } else {
                        json!(value.as_str())
                    };
                    call_app(
                        &entry_state,
                        "settings.workspace_colors.set",
                        json!({"key": entry_key, "value": value}),
                    );
                });
                controls.append(&entry);
                let reset = gtk::Button::from_icon_name("view-refresh-symbolic");
                reset.set_tooltip_text(Some("Use default color"));
                let reset_state = Arc::clone(app_state);
                let reset_key = key.to_string();
                let reset_entry = entry.clone();
                reset.connect_clicked(move |_| {
                    if call_app(
                        &reset_state,
                        "settings.workspace_colors.set",
                        json!({"key": reset_key, "value": Value::Null}),
                    ) {
                        reset_entry.set_text("");
                    }
                });
                controls.append(&reset);
                content.append(&settings_row(title, detail, Some(&controls)));
            }

            content.append(&label("Named Palette", "cmux-heading"));
            for color in status
                .get("colors")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = value_str(color, "name", "").to_string();
                let hex = value_str(color, "color", "").to_string();
                if name.is_empty() || hex.is_empty() {
                    continue;
                }
                let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                controls.append(&workspace_color_swatch(&hex));
                let entry = gtk::Entry::new();
                entry.set_width_chars(9);
                entry.set_max_width_chars(9);
                entry.set_text(&hex);
                let color_state = Arc::clone(app_state);
                let color_name = name.clone();
                entry.connect_activate(move |entry| {
                    call_app(
                        &color_state,
                        "settings.workspace_colors.color.set",
                        json!({"name": color_name, "color": entry.text().as_str()}),
                    );
                });
                controls.append(&entry);
                if !workspace_color_is_builtin(&name) {
                    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
                    remove.set_tooltip_text(Some("Remove named color"));
                    let remove_state = Arc::clone(app_state);
                    let remove_name = name.clone();
                    remove.connect_clicked(move |_| {
                        call_app(
                            &remove_state,
                            "settings.workspace_colors.color.remove",
                            json!({"name": remove_name}),
                        );
                    });
                    controls.append(&remove);
                }
                let base = config::WORKSPACE_COLOR_DEFAULT_PALETTE
                    .iter()
                    .find_map(|(default_name, default_hex)| {
                        (*default_name == name).then_some(*default_hex)
                    })
                    .map(|base| format!("Built-in color. Default: {base}"))
                    .unwrap_or_else(|| "Custom named palette entry.".to_string());
                content.append(&settings_row(&name, &base, Some(&controls)));
            }

            let add_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let add_name = gtk::Entry::new();
            add_name.set_width_chars(14);
            add_name.set_placeholder_text(Some("Color name"));
            let add_hex = gtk::Entry::new();
            add_hex.set_width_chars(9);
            add_hex.set_max_width_chars(9);
            add_hex.set_placeholder_text(Some("#RRGGBB"));
            let add = gtk::Button::with_label("Add");
            let add_state = Arc::clone(app_state);
            let add_name_entry = add_name.clone();
            let add_hex_entry = add_hex.clone();
            add.connect_clicked(move |_| {
                if call_app(
                    &add_state,
                    "settings.workspace_colors.color.set",
                    json!({
                        "name": add_name_entry.text().as_str(),
                        "color": add_hex_entry.text().as_str()
                    }),
                ) {
                    add_name_entry.set_text("");
                    add_hex_entry.set_text("");
                }
            });
            add_controls.append(&add_name);
            add_controls.append(&add_hex);
            add_controls.append(&add);
            content.append(&settings_row(
                "Add Named Color",
                "Adds a reusable color to workspace context menus.",
                Some(&add_controls),
            ));

            let reset = settings_action_button(
                "Reset",
                app_state,
                "settings.workspace_colors.palette.reset",
                json!({}),
            );
            content.append(&settings_row(
                "Reset Palette",
                "Restore the built-in palette and remove custom named colors.",
                Some(&reset),
            ));
        }
        "browser" | "browserImport" => {
            content.append(&label("Browser", "cmux-heading"));
            let status = call_app_value(app_state, "browser.status", json!({}))
                .and_then(|value| value.get("enabled").and_then(Value::as_bool))
                .unwrap_or(false);
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            actions.append(&settings_action_button(
                "Enable",
                app_state,
                "browser.enable",
                json!({}),
            ));
            actions.append(&settings_action_button(
                "Disable",
                app_state,
                "browser.disable",
                json!({}),
            ));
            content.append(&settings_row(
                "Embedded browser",
                if status { "Enabled" } else { "Disabled" },
                Some(&actions),
            ));
            let import_sources = call_app_value(app_state, "browser.import.sources", json!({}))
                .unwrap_or_else(|| json!({}));
            let source_rows = import_sources
                .get("sources")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|source| {
                    let browser = source.get("browser").and_then(Value::as_str)?.trim();
                    let profile = source.get("profile").and_then(Value::as_str)?.trim();
                    if browser.is_empty() || profile.is_empty() {
                        return None;
                    }
                    let mut kinds = Vec::new();
                    for (key, label) in [
                        ("cookies", "cookies"),
                        ("history", "history"),
                        ("bookmarks", "bookmarks"),
                        ("settings", "settings"),
                    ] {
                        if source.get(key).and_then(Value::as_bool).unwrap_or(false) {
                            kinds.push(label);
                        }
                    }
                    Some((
                        browser.to_string(),
                        profile.to_string(),
                        format!("{browser} - {profile} ({})", kinds.join(", ")),
                    ))
                })
                .collect::<Vec<_>>();
            let source_selector = gtk::ComboBoxText::new();
            for (index, (_, _, title)) in source_rows.iter().enumerate() {
                source_selector.append(Some(&index.to_string()), title);
            }
            if source_rows.is_empty() {
                source_selector.append(Some("none"), "No supported browser profiles detected");
                source_selector.set_active_id(Some("none"));
                source_selector.set_sensitive(false);
            } else {
                let default_index = import_sources
                    .get("sources")
                    .and_then(Value::as_array)
                    .and_then(|sources| {
                        sources.iter().position(|source| {
                            source
                                .get("default")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(0);
                source_selector.set_active_id(Some(&default_index.to_string()));
            }
            let destination_selector = gtk::ComboBoxText::new();
            let destination_profiles = browser_profile_choices(&import_sources);
            for (id, name) in &destination_profiles {
                destination_selector.append(Some(id), name);
            }
            let current_profile_id = call_app_value(app_state, "browser.profiles.list", json!({}))
                .and_then(|value| {
                    value
                        .get("current_profile_id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                });
            if let Some(current_profile_id) = current_profile_id.as_deref() {
                destination_selector.set_active_id(Some(current_profile_id));
            } else if !destination_profiles.is_empty() {
                destination_selector.set_active(Some(0));
            }
            let import_button = gtk::Button::with_label("Import");
            import_button
                .set_sensitive(!source_rows.is_empty() && !destination_profiles.is_empty());
            let import_state = Arc::clone(app_state);
            let import_source_selector = source_selector.clone();
            let import_destination_selector = destination_selector.clone();
            import_button.connect_clicked(move |_| {
                let Some(index) = import_source_selector
                    .active_id()
                    .and_then(|id| id.parse::<usize>().ok())
                else {
                    return;
                };
                let Some((browser, profile, _)) = source_rows.get(index) else {
                    return;
                };
                let Some(destination) = import_destination_selector.active_id() else {
                    return;
                };
                call_app(
                    &import_state,
                    "browser.import.data",
                    json!({
                        "browser": browser,
                        "source_profile": profile,
                        "destination_profile": destination.as_str(),
                        "scope": "all"
                    }),
                );
            });
            let import_controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            import_controls.append(&source_selector);
            import_controls.append(&destination_selector);
            import_controls.append(&import_button);
            content.append(&settings_row(
                "Import Browser Data",
                "Cookies, history, bookmarks, and portable profile settings.",
                Some(&import_controls),
            ));
            let search = call_app_value(app_state, "settings.browser.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let engine = gtk::ComboBoxText::new();
            for (id, name) in [
                ("google", "Google"),
                ("duckduckgo", "DuckDuckGo"),
                ("bing", "Bing"),
                ("kagi", "Kagi"),
                ("startpage", "Startpage"),
                ("brave", "Brave Search"),
                ("perplexity", "Perplexity"),
                ("exa", "Exa"),
                ("yahoo", "Yahoo"),
                ("ecosia", "Ecosia"),
                ("qwant", "Qwant"),
                ("mojeek", "Mojeek"),
                ("wikipedia", "Wikipedia"),
                ("github", "GitHub"),
                ("baidu", "Baidu"),
                ("yandex", "Yandex"),
                ("custom", "Custom"),
            ] {
                engine.append(Some(id), name);
            }
            engine.set_active_id(Some(value_str(&search, "defaultSearchEngine", "google")));
            let engine_state = Arc::clone(app_state);
            engine.connect_changed(move |selector| {
                if let Some(engine) = selector.active_id() {
                    call_app(
                        &engine_state,
                        "settings.browser.set",
                        json!({"key": "defaultSearchEngine", "value": engine.as_str()}),
                    );
                }
            });
            content.append(&settings_row(
                "Default search engine",
                "Used when omnibar input is not a URL.",
                Some(&engine),
            ));

            let custom_name = gtk::Entry::new();
            custom_name.set_width_chars(18);
            custom_name.set_max_width_chars(24);
            custom_name.set_placeholder_text(Some("Custom"));
            custom_name.set_text(value_str(&search, "customSearchEngineName", ""));
            let custom_name_state = Arc::clone(app_state);
            custom_name.connect_activate(move |entry| {
                call_app(
                    &custom_name_state,
                    "settings.browser.set",
                    json!({
                        "key": "customSearchEngineName",
                        "value": entry.text().as_str()
                    }),
                );
            });
            content.append(&settings_row(
                "Custom engine name",
                "Display name used when Custom is selected.",
                Some(&custom_name),
            ));

            let custom_template = gtk::Entry::new();
            custom_template.set_width_chars(28);
            custom_template.set_max_width_chars(42);
            custom_template.set_placeholder_text(Some("https://example.com/search?q={query}"));
            custom_template.set_text(value_str(
                &search,
                "customSearchEngineURLTemplate",
                "https://www.google.com/search?q={query}",
            ));
            let custom_template_state = Arc::clone(app_state);
            custom_template.connect_activate(move |entry| {
                call_app(
                    &custom_template_state,
                    "settings.browser.set",
                    json!({
                        "key": "customSearchEngineURLTemplate",
                        "value": entry.text().as_str()
                    }),
                );
            });
            content.append(&settings_row(
                "Custom search URL",
                "Use {query} or %s for the encoded search text.",
                Some(&custom_template),
            ));
            let show_suggestions = gtk::Switch::new();
            show_suggestions.set_active(
                search
                    .get("showSearchSuggestions")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            );
            let show_suggestions_state = Arc::clone(app_state);
            show_suggestions.connect_active_notify(move |switch| {
                call_app(
                    &show_suggestions_state,
                    "settings.browser.set",
                    json!({
                        "key": "showSearchSuggestions",
                        "value": switch.is_active()
                    }),
                );
            });
            content.append(&settings_row(
                "Show search suggestions",
                "Fetch query predictions from supported search providers.",
                Some(&show_suggestions),
            ));
        }
        "globalHotkey" => {
            content.append(&label("Global Hotkey", "cmux-heading"));
            let status = call_app_value(app_state, "settings.global_hotkey.status", json!({}))
                .unwrap_or_else(|| json!({}));
            let enabled = status
                .get("system_wide_hotkey_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let enable = gtk::Switch::new();
            enable.set_active(enabled);
            let enable_state = Arc::clone(app_state);
            enable.connect_active_notify(move |switch| {
                call_app(
                    &enable_state,
                    "settings.global_hotkey.set_enabled",
                    json!({"enabled": switch.is_active()}),
                );
            });
            content.append(&settings_row(
                "Show or Hide All Windows",
                "Register this action system-wide.",
                Some(&enable),
            ));

            let backend = value_str(&status, "backend", "inactive");
            let state = value_str(&status, "state", "inactive");
            let detail = status
                .get("detail")
                .and_then(Value::as_str)
                .filter(|detail| !detail.is_empty())
                .map(|detail| format!("{backend}: {state}\n{detail}"))
                .unwrap_or_else(|| format!("{backend}: {state}"));
            content.append(&settings_row(
                "Registration backend",
                &detail,
                None::<&gtk::Widget>,
            ));

            if let Some(rows) = call_app_value(app_state, "settings.shortcuts", json!({}))
                .and_then(|value| value.get("rows").and_then(Value::as_array).cloned())
            {
                for row in rows.into_iter().filter(|row| {
                    matches!(
                        value_str(row, "name", ""),
                        "global_search" | "show_hide_all_windows"
                    )
                }) {
                    let controls = shortcut_editor_controls(&row, app_state);
                    let detail = shortcut_settings_detail(&row);
                    content.append(&settings_row(
                        &value_str(&row, "title", "Shortcut"),
                        &detail,
                        Some(&controls),
                    ));
                }
            }
        }
        "keyboardShortcuts" => {
            content.append(&label("Keyboard Shortcuts", "cmux-heading"));
            let help = settings_action_button(
                "Open Shortcut Help",
                app_state,
                "help.shortcuts.toggle",
                json!({}),
            );
            content.append(&settings_row(
                "Shortcut reference",
                &snapshot.cmux.path,
                Some(&help),
            ));
            if let Some(rows) = call_app_value(app_state, "settings.shortcuts", json!({}))
                .and_then(|value| value.get("rows").and_then(Value::as_array).cloned())
            {
                for row in rows {
                    let controls = shortcut_editor_controls(&row, app_state);
                    let detail = shortcut_settings_detail(&row);
                    content.append(&settings_row(
                        &value_str(&row, "title", "Shortcut"),
                        &detail,
                        Some(&controls),
                    ));
                }
            }
        }
        "settingsJSON" => {
            content.append(&label("cmux.json", "cmux-heading"));
            content.append(&label(&snapshot.cmux.path, "cmux-muted"));
            let buffer = gtk::TextBuffer::new(None);
            buffer.set_text(&snapshot.cmux.contents);
            let text = gtk::TextView::with_buffer(&buffer);
            text.set_editable(false);
            text.set_monospace(true);
            text.set_wrap_mode(gtk::WrapMode::None);
            let scroll = gtk::ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .child(&text)
                .build();
            content.append(&scroll);
        }
        _ => {
            content.append(&label(section_title, "cmux-heading"));
            let docs = config::settings_docs_payload();
            content.append(&settings_row(
                "Configuration source",
                &docs.primary,
                None::<&gtk::Widget>,
            ));
            content.append(&settings_row(
                "Status",
                "This section uses the shared cmux configuration on Linux.",
                None::<&gtk::Widget>,
            ));
        }
    }
    content
}

fn settings_surface_view(view: &Value, app_state: &Arc<Mutex<AppState>>) -> Option<gtk::Box> {
    let settings = view.get("settings")?;
    let target = settings
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("general");
    let section_title = settings
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Settings");
    let surface_id = surface_id_or_ref(view)?;
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.add_css_class("cmux-settings");
    root.set_hexpand(true);
    root.set_vexpand(true);

    let nav = gtk::Box::new(gtk::Orientation::Vertical, 2);
    nav.add_css_class("cmux-settings-nav");
    nav.set_size_request(210, -1);
    for item in settings
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = item.get("key").and_then(Value::as_str).unwrap_or_default();
        let title = item.get("title").and_then(Value::as_str).unwrap_or(key);
        let button = gtk::ToggleButton::with_label(title);
        button.set_active(key == target);
        let app_state = Arc::clone(app_state);
        let surface_id = surface_id.clone();
        let key = key.to_string();
        button.connect_clicked(move |_| {
            call_app(
                &app_state,
                "settings.set_target",
                json!({"surface_id": surface_id, "target": key}),
            );
        });
        nav.append(&button);
    }
    let nav_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&nav)
        .build();
    root.append(&nav_scroll);
    root.append(&settings_content(target, section_title, app_state));
    Some(root)
}

fn prune_terminal_text_box_controls(controls: &TerminalTextBoxControlsCache, surfaces: &[Value]) {
    let live = surfaces
        .iter()
        .filter_map(surface_id_or_ref)
        .collect::<HashSet<_>>();
    controls
        .borrow_mut()
        .retain(|surface_id, _| live.contains(surface_id));
}

fn terminal_text_box_state(view: &Value) -> Option<&Value> {
    view.get("text_box")
        .filter(|state| state.get("active").is_some())
}

fn text_box_has_content(state: &Value) -> bool {
    state
        .get("can_submit")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn open_terminal_text_box_file_picker(
    controls: &TerminalTextBoxControls,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
) {
    let dialog = gtk::FileChooserNative::builder()
        .title("Attach Files")
        .action(gtk::FileChooserAction::Open)
        .accept_label("Attach")
        .cancel_label("Cancel")
        .select_multiple(true)
        .build();
    if let Some(parent) = controls
        .root
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
    {
        dialog.set_transient_for(Some(&parent));
    }
    let surface_id = surface_id.to_string();
    let app_state = Arc::clone(app_state);
    let text_view = controls.text_view.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let files = dialog.files();
            let paths = (0..files.n_items())
                .filter_map(|index| files.item(index))
                .filter_map(|item| item.downcast::<gio::File>().ok())
                .filter_map(|file| file.path())
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                call_app(
                    &app_state,
                    "terminal.textbox.attach",
                    json!({
                        "surface_id": surface_id,
                        "paths": paths,
                        "offset": text_view.buffer().cursor_position()
                    }),
                );
            }
        }
        dialog.destroy();
    });
    dialog.show();
}

fn is_feedback_surface(view: &Value) -> bool {
    value_str(view, "kind", "") == "browser"
        && value_str(view, "url", "").ends_with("#cmux-feedback")
}

fn set_feedback_attachment_summary(label: &gtk::Label, paths: &[String]) {
    if paths.is_empty() {
        label.set_text("No images attached");
        return;
    }
    let names = paths
        .iter()
        .filter_map(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
        .collect::<Vec<_>>();
    label.set_text(&format!(
        "{} image{}: {}",
        paths.len(),
        if paths.len() == 1 { "" } else { "s" },
        names.join(", ")
    ));
}

fn feedback_surface_view(_app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("cmux-feedback");
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let email_label = label("Email", "cmux-heading");
    email_label.set_xalign(0.0);
    root.append(&email_label);
    let email = gtk::Entry::new();
    email.set_input_purpose(gtk::InputPurpose::Email);
    email.set_placeholder_text(Some("you@example.com"));
    root.append(&email);

    let message_label = label("Feedback", "cmux-heading");
    message_label.set_xalign(0.0);
    root.append(&message_label);
    let message = gtk::TextView::new();
    message.set_wrap_mode(gtk::WrapMode::WordChar);
    message.set_accepts_tab(false);
    message.set_vexpand(true);
    let message_scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_height(180)
        .child(&message)
        .build();
    root.append(&message_scroll);

    let attachment_paths = Rc::new(RefCell::new(Vec::<String>::new()));
    let attachment_summary = label("No images attached", "cmux-muted");
    attachment_summary.set_xalign(0.0);
    attachment_summary.set_wrap(true);
    root.append(&attachment_summary);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let attach = gtk::Button::from_icon_name("mail-attachment-symbolic");
    attach.add_css_class("cmux-icon-action");
    attach.set_tooltip_text(Some("Attach images"));
    let clear_attachments = gtk::Button::from_icon_name("edit-clear-symbolic");
    clear_attachments.add_css_class("cmux-icon-action");
    clear_attachments.set_tooltip_text(Some("Clear attachments"));
    clear_attachments.set_sensitive(false);

    let send_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    send_content.append(&gtk::Image::from_icon_name("mail-send-symbolic"));
    send_content.append(&gtk::Label::new(Some("Send")));
    let send = gtk::Button::builder().child(&send_content).build();
    send.add_css_class("suggested-action");
    let status = label("", "cmux-muted");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_selectable(true);

    {
        let root = root.clone();
        let attachment_paths = Rc::clone(&attachment_paths);
        let attachment_summary = attachment_summary.clone();
        let clear_attachments = clear_attachments.clone();
        let status = status.clone();
        attach.connect_clicked(move |_| {
            let dialog = gtk::FileChooserNative::builder()
                .title("Attach Feedback Images")
                .action(gtk::FileChooserAction::Open)
                .accept_label("Attach")
                .cancel_label("Cancel")
                .select_multiple(true)
                .build();
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            for mime_type in [
                "image/gif",
                "image/heic",
                "image/heif",
                "image/jpeg",
                "image/png",
                "image/tiff",
                "image/webp",
            ] {
                filter.add_mime_type(mime_type);
            }
            dialog.add_filter(&filter);
            if let Some(parent) = root
                .root()
                .and_then(|root| root.downcast::<gtk::Window>().ok())
            {
                dialog.set_transient_for(Some(&parent));
            }
            let attachment_paths = Rc::clone(&attachment_paths);
            let attachment_summary = attachment_summary.clone();
            let clear_attachments = clear_attachments.clone();
            let status = status.clone();
            dialog.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Accept {
                    let files = dialog.files();
                    let selected = (0..files.n_items())
                        .filter_map(|index| files.item(index))
                        .filter_map(|item| item.downcast::<gio::File>().ok())
                        .filter_map(|file| file.path())
                        .map(|path| path.to_string_lossy().to_string())
                        .collect::<Vec<_>>();
                    let mut paths = attachment_paths.borrow_mut();
                    for path in selected {
                        if paths.len() >= 10 {
                            status.set_text("Feedback supports at most 10 images.");
                            break;
                        }
                        if !paths.contains(&path) {
                            paths.push(path);
                        }
                    }
                    set_feedback_attachment_summary(&attachment_summary, &paths);
                    clear_attachments.set_sensitive(!paths.is_empty());
                }
                dialog.destroy();
            });
            dialog.show();
        });
    }

    {
        let attachment_paths = Rc::clone(&attachment_paths);
        let attachment_summary = attachment_summary.clone();
        let clear_attachments_for_click = clear_attachments.clone();
        clear_attachments.connect_clicked(move |_| {
            attachment_paths.borrow_mut().clear();
            set_feedback_attachment_summary(&attachment_summary, &[]);
            clear_attachments_for_click.set_sensitive(false);
        });
    }

    {
        let email = email.clone();
        let message = message.clone();
        let attachment_paths = Rc::clone(&attachment_paths);
        let attachment_summary = attachment_summary.clone();
        let attach = attach.clone();
        let clear_attachments = clear_attachments.clone();
        let send_for_click = send.clone();
        let status = status.clone();
        send.connect_clicked(move |_| {
            let buffer = message.buffer();
            let body = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let params = json!({
                "email": email.text().to_string(),
                "body": body,
                "image_paths": attachment_paths.borrow().clone()
            });
            attach.set_sensitive(false);
            clear_attachments.set_sensitive(false);
            send_for_click.set_sensitive(false);
            status.set_text("Sending feedback...");

            let result = Arc::new(Mutex::new(None::<std::result::Result<Value, String>>));
            let thread_result = Arc::clone(&result);
            std::thread::spawn(move || {
                let response =
                    crate::app::submit_feedback_request(&params).map_err(|error| error.message);
                if let Ok(mut slot) = thread_result.lock() {
                    *slot = Some(response);
                }
            });

            let result = Arc::clone(&result);
            let message = message.clone();
            let attachment_paths = Rc::clone(&attachment_paths);
            let attachment_summary = attachment_summary.clone();
            let attach = attach.clone();
            let clear_attachments = clear_attachments.clone();
            let send = send_for_click.clone();
            let status = status.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                let response = result.lock().ok().and_then(|mut slot| slot.take());
                let Some(response) = response else {
                    return glib::ControlFlow::Continue;
                };
                attach.set_sensitive(true);
                send.set_sensitive(true);
                match response {
                    Ok(value) => {
                        if value
                            .get("delivered")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        {
                            status.set_text("Feedback sent.");
                        } else {
                            status.set_text(
                                "Feedback saved to the retry queue. It can be sent with `cmux feedback retry`.",
                            );
                        }
                        message.buffer().set_text("");
                        attachment_paths.borrow_mut().clear();
                        set_feedback_attachment_summary(&attachment_summary, &[]);
                        clear_attachments.set_sensitive(false);
                    }
                    Err(error) => {
                        status.set_text(&error);
                        clear_attachments.set_sensitive(!attachment_paths.borrow().is_empty());
                    }
                }
                glib::ControlFlow::Break
            });
        });
    }

    actions.append(&attach);
    actions.append(&clear_attachments);
    actions.append(&send);
    root.append(&actions);
    root.append(&status);
    root
}

fn ensure_terminal_text_box_controls(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    cache: &TerminalTextBoxControlsCache,
) -> Option<TerminalTextBoxControls> {
    let surface_id = surface_id_or_ref(view)?;
    if !cache.borrow().contains_key(&surface_id) {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("cmux-text-box");
        root.set_hexpand(true);

        let attachments = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        attachments.add_css_class("cmux-text-box-attachments");
        root.append(&attachments);

        let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let attach = gtk::Button::from_icon_name("list-add-symbolic");
        attach.add_css_class("cmux-text-box-tool");
        attach.set_tooltip_text(Some("Attach Files"));
        input_row.append(&attach);

        let text_view = gtk::TextView::new();
        text_view.add_css_class("cmux-text-box-editor");
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_accepts_tab(false);
        text_view.set_hexpand(true);
        text_view.set_vexpand(false);
        text_view.set_tooltip_text(Some("Prompt or command"));
        let editor = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .min_content_height(42)
            .max_content_height(220)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&text_view)
            .build();
        input_row.append(&editor);

        let send = gtk::Button::from_icon_name("mail-send-symbolic");
        send.add_css_class("cmux-text-box-tool");
        send.add_css_class("cmux-text-box-send");
        send.set_tooltip_text(Some("Send to Terminal"));
        send.set_sensitive(false);
        input_row.append(&send);
        root.append(&input_row);

        let syncing = Rc::new(Cell::new(false));
        let changed_syncing = Rc::clone(&syncing);
        let changed_state = Arc::clone(app_state);
        let changed_surface = surface_id.clone();
        let changed_send = send.clone();
        text_view.buffer().connect_changed(move |buffer| {
            if changed_syncing.get() {
                return;
            }
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            changed_send.set_sensitive(!text.trim().is_empty());
            call_app(
                &changed_state,
                "terminal.textbox.set_text",
                json!({"surface_id": changed_surface, "text": text}),
            );
        });

        let focus_state = Arc::clone(app_state);
        let focus_surface = surface_id.clone();
        text_view.connect_has_focus_notify(move |view| {
            if view.has_focus() {
                call_app(
                    &focus_state,
                    "terminal.textbox.set_focus",
                    json!({"surface_id": focus_surface, "focus": "textBox"}),
                );
            }
        });

        let submit_state = Arc::clone(app_state);
        let submit_surface = surface_id.clone();
        let submit_view = text_view.clone();
        send.connect_clicked(move |_| {
            if call_app_value(
                &submit_state,
                "terminal.textbox.submit",
                json!({"surface_id": submit_surface}),
            )
            .is_some()
            {
                submit_view.buffer().set_text("");
            }
        });

        let key = gtk::EventControllerKey::new();
        let key_state = Arc::clone(app_state);
        let key_surface = surface_id.clone();
        let key_view = text_view.clone();
        key.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gdk::Key::Escape {
                call_app(
                    &key_state,
                    "terminal.textbox.escape",
                    json!({"surface_id": key_surface}),
                );
                return glib::Propagation::Stop;
            }
            if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter)
                && modifiers.contains(gdk::ModifierType::CONTROL_MASK)
            {
                if call_app_value(
                    &key_state,
                    "terminal.textbox.submit",
                    json!({"surface_id": key_surface}),
                )
                .is_some()
                {
                    key_view.buffer().set_text("");
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        text_view.add_controller(key);

        let controls = TerminalTextBoxControls {
            root,
            text_view,
            attachments,
            send,
            syncing,
            focus_generation: Rc::new(Cell::new(0)),
            file_picker_generation: Rc::new(Cell::new(0)),
        };
        let attach_controls = controls.clone();
        let attach_state = Arc::clone(app_state);
        let attach_surface = surface_id.clone();
        attach.connect_clicked(move |_| {
            open_terminal_text_box_file_picker(&attach_controls, &attach_surface, &attach_state);
        });
        cache.borrow_mut().insert(surface_id.clone(), controls);
    }
    cache.borrow().get(&surface_id).cloned()
}

fn sync_terminal_text_box_controls(
    controls: &TerminalTextBoxControls,
    state: &Value,
    surface_id: &str,
    app_state: &Arc<Mutex<AppState>>,
    terminal_focus: Option<&gtk::Widget>,
) {
    let buffer = controls.text_view.buffer();
    let model_text = value_str(state, "text", "");
    if !controls.text_view.has_focus()
        && buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .as_str()
            != model_text
    {
        controls.syncing.set(true);
        buffer.set_text(model_text);
        controls.syncing.set(false);
    }
    controls.send.set_sensitive(text_box_has_content(state));
    let max_lines = state
        .get("max_lines")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 20) as i32;
    if let Some(parent) = controls.text_view.parent() {
        if let Ok(scroll) = parent.downcast::<gtk::ScrolledWindow>() {
            scroll.set_max_content_height(max_lines * 22);
        }
    }

    while let Some(child) = controls.attachments.first_child() {
        controls.attachments.remove(&child);
    }
    for attachment in state
        .get("attachments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let chip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        chip.add_css_class("cmux-text-box-chip");
        let name = label(value_str(attachment, "displayName", "File"), "cmux-muted");
        name.set_max_width_chars(24);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        chip.append(&name);
        let remove = gtk::Button::from_icon_name("window-close-symbolic");
        remove.add_css_class("cmux-text-box-tool");
        remove.set_tooltip_text(Some("Remove Attachment"));
        let attachment_id = value_str(attachment, "id", "").to_string();
        let surface_id = surface_id.to_string();
        let remove_state = Arc::clone(app_state);
        remove.connect_clicked(move |_| {
            call_app(
                &remove_state,
                "terminal.textbox.remove_attachment",
                json!({"surface_id": surface_id, "attachment_id": attachment_id}),
            );
        });
        chip.append(&remove);
        controls.attachments.append(&chip);
    }
    controls.attachments.set_visible(
        !state
            .get("attachments")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty),
    );

    let focus_generation = state
        .get("focus_generation")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if focus_generation > controls.focus_generation.replace(focus_generation) {
        if value_str(state, "focus", "terminal") == "textBox" {
            let text_view = controls.text_view.clone();
            glib::idle_add_local_once(move || {
                text_view.grab_focus();
            });
        } else if let Some(terminal) = terminal_focus {
            let terminal = terminal.clone();
            glib::idle_add_local_once(move || {
                terminal.grab_focus();
            });
        }
    }
    let picker_generation = state
        .get("file_picker_generation")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if picker_generation > controls.file_picker_generation.replace(picker_generation) {
        let controls = controls.clone();
        let surface_id = surface_id.to_string();
        let app_state = Arc::clone(app_state);
        glib::idle_add_local_once(move || {
            open_terminal_text_box_file_picker(&controls, &surface_id, &app_state);
        });
    }
}

fn agent_hibernation_placeholder(view: &Value, app_state: &Arc<Mutex<AppState>>) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("cmux-agent-hibernation-placeholder");
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name("media-playback-pause-symbolic");
    icon.set_pixel_size(34);
    root.append(&icon);
    root.append(&label("Agent hibernated", "cmux-heading"));

    let state = view.get("agent_hibernation").unwrap_or(&Value::Null);
    let agent_name = value_str(state, "agent_name", "Agent");
    root.append(&label(agent_name, "cmux-muted"));
    if let Some(last_activity_ms) = state.get("last_activity_ms").and_then(Value::as_u64) {
        root.append(&label(
            &format!(
                "Last activity {}",
                relative_elapsed_text(current_unix_millis().saturating_sub(last_activity_ms))
            ),
            "cmux-muted",
        ));
    }

    let resume = gtk::Button::with_label("Resume");
    resume.add_css_class("suggested-action");
    resume.set_halign(gtk::Align::Center);
    resume.set_tooltip_text(Some("Resume this saved agent session"));
    if let Some(surface_id) = surface_id_or_ref(view) {
        let state = Arc::clone(app_state);
        resume.connect_clicked(move |_| {
            call_app(
                &state,
                "agent.hibernation.resume",
                json!({"surface_id": surface_id, "focus": true}),
            );
        });
    } else {
        resume.set_sensitive(false);
    }
    root.append(&resume);
    root
}

fn relative_elapsed_text(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1_000;
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

fn surface_card(
    view: &Value,
    app_state: &Arc<Mutex<AppState>>,
    pane_allocations: &PaneAllocations,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    diff_controls: &DiffSurfaceControlsCache,
    terminal_search_controls: &TerminalSearchControlsCache,
    terminal_text_box_controls: &TerminalTextBoxControlsCache,
    renderer_mode: GtkRendererMode,
    config_reload_generation: u64,
    ui_mode: GtkUiMode,
    local_refresh: &GtkLocalRefresh,
) -> gtk::Box {
    let card = gtk::Box::new(
        gtk::Orientation::Vertical,
        if ui_mode.is_next() { 0 } else { 8 },
    );
    card.add_css_class("cmux-surface");
    if let Some(pane_id) = pane_id_or_ref(view) {
        card.set_widget_name(&pane_id);
    }
    let kind = value_str(view, "kind", "terminal");
    card.add_css_class(match kind {
        "browser" => "cmux-surface-browser",
        "markdown" => "cmux-surface-markdown",
        "diff" => "cmux-surface-diff",
        _ => "cmux-surface-terminal-context",
    });
    if view
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        card.add_css_class("cmux-surface-focused");
    }
    card.set_hexpand(true);
    card.set_vexpand(true);
    let margin = if ui_mode.is_next() { 0 } else { 6 };
    card.set_margin_start(margin);
    card.set_margin_end(margin);
    card.set_margin_top(margin);
    card.set_margin_bottom(margin);

    let title = value_str(view, "title", "Terminal");
    if let Some(tab_strip) = pane_tab_strip(view, app_state, Some(local_refresh)) {
        card.append(&tab_strip);
    } else {
        card.append(&label(title, "cmux-heading"));
    }
    if !ui_mode.is_next() {
        let meta = format!("{} · {}", value_str(view, "surface_ref", "surface:-"), kind);
        card.append(&label(&meta, "cmux-muted"));
    }

    let is_terminal = value_str(view, "kind", "terminal") == "terminal";
    if is_terminal
        && (value_bool(view, "hibernated")
            || view
                .get("agent_hibernation")
                .is_some_and(|value| value.is_object()))
    {
        card.append(&agent_hibernation_placeholder(view, app_state));
        return card;
    }
    let ghostty = if is_terminal && renderer_mode == GtkRendererMode::Ghostty {
        ensure_ghostty_surface_widget(view, app_state, ghostty_widgets, config_reload_generation)
    } else {
        None
    };
    if let (Some(state), Some(ghostty)) = (terminal_search_state(view), ghostty.as_ref()) {
        let controls =
            ensure_terminal_search_controls(&state, app_state, ghostty, terminal_search_controls);
        detach_widget(&controls.root);
        card.append(&controls.root);
        if !widget_contains_focus(&controls.entry) {
            let entry = controls.entry.clone();
            glib::idle_add_local_once(move || {
                entry.grab_focus();
                entry.select_region(0, -1);
            });
        }
    }
    if is_terminal && renderer_mode != GtkRendererMode::Ghostty {
        if let Some(status) = terminal_status_display(view) {
            card.append(&label(&status, "cmux-muted"));
        }
    }
    if is_terminal && renderer_mode == GtkRendererMode::Ghostty {
        if let Some(ghostty) = ghostty.as_ref() {
            detach_widget(ghostty.root());
            ghostty.root().add_css_class("cmux-terminal-preview");
            card.append(ghostty.root());
        } else {
            card.append(&label("Ghostty surface missing id", "cmux-muted"));
        }
    } else if is_feedback_surface(view) {
        card.append(&feedback_surface_view(app_state));
    } else if value_str(view, "kind", "") == "settings" {
        if let Some(settings) = settings_surface_view(view, app_state) {
            card.append(&settings);
        } else {
            card.append(&label("Settings state unavailable", "cmux-muted"));
        }
    } else if value_str(view, "kind", "") == "project" {
        if let Some(project) = project_surface_view(view, app_state) {
            card.append(&project);
        } else {
            card.append(&label("Project state unavailable", "cmux-muted"));
        }
    } else if value_str(view, "kind", "") == "agent-session" {
        if let Some(agent_session) = agent_session_surface_view(view, app_state) {
            card.append(&agent_session);
        } else {
            card.append(&label("Agent session state unavailable", "cmux-muted"));
        }
    } else if matches!(
        value_str(view, "kind", ""),
        "filePreview" | "markdown" | "diff"
    ) {
        if let Some(document) = native_document_surface_view(view, app_state, diff_controls) {
            card.append(&document);
        } else {
            card.append(&label("Document state unavailable", "cmux-muted"));
        }
    } else {
        let mut native_browser_view = None;
        if let Some(state) = ui::browser_navigation_state(view) {
            let controls = ensure_browser_surface_controls(
                &state,
                view.get("global_search_needle").and_then(Value::as_str),
                app_state,
                browser_controls,
                ghostty_widgets,
            );
            detach_widget(&controls.root);
            card.append(&controls.root);
            detach_widget(&controls.find_bar);
            card.append(&controls.find_bar);
            native_browser_view = controls.web_view;
        }
        if let Some(web_view) = native_browser_view {
            detach_widget(web_view.widget());
            card.append(web_view.widget());
        } else {
            let TerminalDisplay {
                text: preview,
                markup,
            } = if is_terminal {
                terminal_preview_display(view)
            } else {
                surface_preview_display(view)
            };
            let preview_label = label(
                &preview,
                if is_terminal {
                    "cmux-terminal-preview"
                } else {
                    "cmux-surface-preview"
                },
            );
            if is_terminal && terminal_loading(view) {
                preview_label.add_css_class("cmux-terminal-loading");
            }
            if !is_terminal && value_str(view, "kind", "") == "project" {
                preview_label.add_css_class("cmux-project-preview");
            }
            if let Some(markup) = markup.as_deref() {
                preview_label.set_markup(markup);
            }
            preview_label.set_wrap(true);
            preview_label.set_selectable(true);
            preview_label.set_hexpand(true);
            preview_label.set_vexpand(true);
            preview_label.set_xalign(0.0);
            preview_label.set_yalign(0.0);
            if is_terminal {
                attach_terminal_link_gesture(&preview_label, view, &preview, app_state);
            }
            if is_terminal {
                if let Some(pane_id) = pane_id_or_ref(view) {
                    connect_pane_allocation_probe(
                        &preview_label,
                        pane_id,
                        Arc::clone(app_state),
                        Rc::clone(pane_allocations),
                    );
                }
            }
            card.append(&preview_label);
        }
    }

    if is_terminal {
        if let Some(state) = terminal_text_box_state(view).filter(|state| {
            state
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        }) {
            if let Some(controls) =
                ensure_terminal_text_box_controls(view, app_state, terminal_text_box_controls)
            {
                detach_widget(&controls.root);
                let surface_id = surface_id_or_ref(view).unwrap_or_default();
                sync_terminal_text_box_controls(
                    &controls,
                    state,
                    &surface_id,
                    app_state,
                    ghostty
                        .as_ref()
                        .map(|ghostty| ghostty.root().upcast_ref::<gtk::Widget>()),
                );
                card.append(&controls.root);
            }
        }
    }

    if let Some(target) = surface_id_or_ref(view) {
        let gesture = gtk::GestureClick::new();
        let app_state = Arc::clone(app_state);
        gesture.connect_pressed(move |_, _, _, _| {
            call_app(&app_state, "surface.focus", json!({"surface_id": target}));
        });
        card.add_controller(gesture);
    }
    attach_surface_context_menu_for(&card, app_state, view, ghostty.as_ref());
    card
}

fn attach_terminal_link_gesture(
    label: &gtk::Label,
    view: &Value,
    text: &str,
    app_state: &Arc<Mutex<AppState>>,
) {
    let Some(url) = first_terminal_link(text) else {
        return;
    };
    let Some(params) = terminal_link_browser_open_params(view, &url) else {
        return;
    };

    let gesture = gtk::GestureClick::new();
    let app_state = Arc::clone(app_state);
    gesture.connect_pressed(move |gesture, _, _, _| {
        if !cmd_click_modifier_active(gesture.current_event_state()) {
            return;
        }
        call_app(&app_state, "browser.open_split", params.clone());
    });
    label.add_controller(gesture);
}

fn terminal_link_browser_open_params(view: &Value, url: &str) -> Option<Value> {
    let surface_id = surface_id_or_ref(view)?;
    let mut params = json!({
        "surface_id": surface_id,
        "url": url,
        "focus": true
    });
    if let Some(workspace_id) = workspace_id_or_ref(view) {
        params["workspace_id"] = json!(workspace_id);
    }
    Some(params)
}

fn cmd_click_modifier_active(modifiers: gdk::ModifierType) -> bool {
    modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK)
}

fn workspace_sidebar_select_params(target: &str, modifiers: gdk::ModifierType) -> Value {
    let range = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
    let additive = modifiers.intersects(
        gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::CONTROL_MASK,
    );
    json!({
        "workspace_id": target,
        "toggle": additive && !range,
        "range": range,
        "extend": additive && range
    })
}

fn first_terminal_link(text: &str) -> Option<String> {
    text.split_whitespace()
        .filter_map(normalize_terminal_link_token)
        .next()
}

fn normalize_terminal_link_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}'
        )
    });
    let start = trimmed
        .find("https://")
        .or_else(|| trimmed.find("http://"))?;
    let candidate = &trimmed[start..];
    let candidate = candidate.trim_end_matches(|ch: char| {
        matches!(
            ch,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\''
        )
    });
    (!candidate.is_empty()).then(|| candidate.to_string())
}

fn surface_rename_params(view: &Value, title: &str) -> Option<Value> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    surface_action_params(view, "rename", &[("title", json!(title))])
}

fn surface_action_params(view: &Value, action: &str, extra: &[(&str, Value)]) -> Option<Value> {
    let surface_id = surface_id_or_ref(view)?;
    let mut params = json!({
        "surface_id": surface_id,
        "action": action
    });
    if let Some(workspace_id) = workspace_id_or_ref(view) {
        params["workspace_id"] = json!(workspace_id);
    }
    if let Some(window_id) = view
        .get("window_id")
        .or_else(|| view.get("window_ref"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        params["window_id"] = json!(window_id);
    }
    for (key, value) in extra {
        params[*key] = value.clone();
    }
    Some(params)
}

fn surface_context_action_specs(view: &Value) -> Vec<(&'static str, &'static str)> {
    let mut actions = Vec::new();
    if view
        .get("custom_title")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        actions.push((SURFACE_CLEAR_NAME_LABEL, "clear-name"));
    }
    if view.get("pinned").and_then(Value::as_bool).unwrap_or(false) {
        actions.push(("Unpin Tab", "unpin"));
    } else {
        actions.push(("Pin Tab", "pin"));
    }
    if view.get("unread").and_then(Value::as_bool).unwrap_or(false) {
        actions.push(("Mark Tab as Read", "mark-read"));
    } else {
        actions.push(("Mark Tab as Unread", "mark-unread"));
    }
    let kind = value_str(view, "kind", value_str(view, "type", "terminal"));
    if kind == "browser" {
        actions.push(("Reload Tab", "reload"));
        actions.push(("Duplicate Tab", "duplicate"));
    }
    actions.push(("New Terminal to the Right", "new-terminal-right"));
    actions.push(("New Browser to the Right", "new-browser-right"));
    actions.push((SURFACE_DETACH_LABEL, "move-to-new-workspace"));
    actions.push(("Close Tabs to the Left", "close-left"));
    actions.push(("Close Tabs to the Right", "close-right"));
    actions.push(("Close Other Tabs", "close-others"));
    actions
}

fn surface_context_action_params(view: &Value, action: &str) -> Option<Value> {
    if matches!(action, "new-terminal-right" | "new-browser-right") {
        surface_action_params(view, action, &[("focus", Value::Bool(true))])
    } else {
        surface_action_params(view, action, &[])
    }
}

fn parent_context_popover<W: IsA<gtk::Widget>>(popover: &gtk::Popover, parent: &W) {
    popover.set_parent(parent);
    let popover = popover.downgrade();
    parent.connect_destroy(move |_| {
        if let Some(popover) = popover
            .upgrade()
            .filter(|popover| popover.parent().is_some())
        {
            popover.unparent();
        }
    });
}

fn attach_surface_context_menu_for(
    card: &gtk::Box,
    app_state: &Arc<Mutex<AppState>>,
    view: &Value,
    ghostty: Option<&crate::gtk_ghostty::GhosttySurfaceWidget>,
) {
    if surface_id_or_ref(view).is_none() {
        return;
    }
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    parent_context_popover(&popover, card);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 6);
    menu.add_css_class("cmux-context-menu");
    if value_str(view, "kind", value_str(view, "type", "terminal")) == "terminal" {
        for (label, action) in [
            ("Copy", "copy_to_clipboard"),
            ("Paste", "paste_from_clipboard"),
        ] {
            let button = gtk::Button::with_label(label);
            button.add_css_class("cmux-context-item");
            if let Some(ghostty) = ghostty.cloned() {
                let popover = popover.downgrade();
                button.connect_clicked(move |_| {
                    let _ = ghostty.perform_binding_action(action);
                    if let Some(popover) = popover.upgrade() {
                        popover.popdown();
                    }
                    ghostty.grab_focus();
                });
            } else {
                button.set_sensitive(false);
            }
            menu.append(&button);
        }
        append_context_separator(&menu);
    }
    let entry = gtk::Entry::new();
    entry.set_text(value_str(view, "title", "Terminal"));
    entry.set_width_chars(24);
    menu.append(&entry);

    let rename = gtk::Button::with_label(SURFACE_RENAME_LABEL);
    rename.add_css_class("cmux-context-item");
    {
        let app_state = Arc::clone(app_state);
        let popover = popover.downgrade();
        let entry = entry.downgrade();
        let view = view.clone();
        rename.connect_clicked(move |_| {
            let Some(entry) = entry.upgrade() else {
                return;
            };
            if let Some(params) = surface_rename_params(&view, &entry.text()) {
                call_app(&app_state, "surface.action", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    {
        let app_state = Arc::clone(app_state);
        let popover = popover.downgrade();
        let view = view.clone();
        entry.connect_activate(move |entry| {
            if let Some(params) = surface_rename_params(&view, &entry.text()) {
                call_app(&app_state, "surface.action", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    menu.append(&rename);

    append_context_separator(&menu);
    for (title, action) in surface_context_action_specs(view) {
        if let Some(params) = surface_context_action_params(view, action) {
            menu.append(&context_menu_rpc_button(
                title,
                app_state,
                &popover,
                "surface.action",
                params,
            ));
        }
    }

    popover.set_child(Some(&menu));
    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let popover = popover.downgrade();
    let entry = entry.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        let (Some(popover), Some(entry)) = (popover.upgrade(), entry.upgrade()) else {
            return;
        };
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
    card.add_controller(gesture);
}

fn connect_pane_allocation_probe(
    widget: &gtk::Label,
    pane_id: String,
    app_state: Arc<Mutex<AppState>>,
    pane_allocations: PaneAllocations,
) {
    widget.add_tick_callback(move |widget, _| {
        let Some(allocation) =
            pane_allocation_from_pixels(widget.allocated_width(), widget.allocated_height())
        else {
            return glib::ControlFlow::Continue;
        };

        {
            let mut allocations = pane_allocations.borrow_mut();
            if allocations.get(&pane_id) == Some(&allocation) {
                return glib::ControlFlow::Continue;
            }
            allocations.insert(pane_id.clone(), allocation);
        }

        let (cols, rows) = terminal_grid_for_allocation(allocation);
        call_app(
            &app_state,
            "renderer.apply_size",
            json!({
                "pane_id": pane_id,
                "cols": cols,
                "rows": rows,
                "pixel_width": allocation.width,
                "pixel_height": allocation.height
            }),
        );
        glib::ControlFlow::Continue
    });
}

fn pane_allocation_from_pixels(width: i32, height: i32) -> Option<GtkPaneAllocation> {
    (width > 1 && height > 1).then_some(GtkPaneAllocation { width, height })
}

fn terminal_grid_for_allocation(allocation: GtkPaneAllocation) -> (u16, u16) {
    let cols = (allocation.width / GTK_CELL_WIDTH)
        .max(1)
        .min(u16::MAX as i32) as u16;
    let rows = (allocation.height / GTK_CELL_HEIGHT)
        .max(1)
        .min(u16::MAX as i32) as u16;
    (cols, rows)
}

fn connect_terminal_keys(
    window: &gtk::ApplicationWindow,
    app_state: &Arc<Mutex<AppState>>,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    diff_controls: &DiffSurfaceControlsCache,
    pending_browser_shortcut_actions: &PendingBrowserShortcutActions,
    window_id: &str,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let browser_focus_escape = Rc::new(RefCell::new(BrowserFocusEscapeState::default()));
    let app_state = Arc::clone(app_state);
    let ghostty_widgets = Rc::clone(ghostty_widgets);
    let browser_controls = Rc::clone(browser_controls);
    let browser_controls_for_release = Rc::clone(&browser_controls);
    let diff_controls = Rc::clone(diff_controls);
    let pending_browser_shortcut_actions = Rc::clone(pending_browser_shortcut_actions);
    let weak_window = window.downgrade();
    let browser_focus_escape_for_press = Rc::clone(&browser_focus_escape);
    let window_id = window_id.to_string();
    controller.connect_key_pressed(move |_, keyval, keycode, modifiers| {
        if is_plain_escape(keyval, modifiers) && shortcut_help_is_visible(&app_state, &window_id) {
            handle_shortcut_help_dismissal(
                &app_state,
                &window_id,
                ShortcutHelpDismissInteraction::PlainEscape,
            );
            return glib::Propagation::Stop;
        }
        let focused_widget = weak_window
            .upgrade()
            .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window));
        if control_activation_key_should_propagate(
            focused_widget_is_activation_control(focused_widget.as_ref()),
            keyval,
            modifiers,
        ) {
            return glib::Propagation::Proceed;
        }
        let (model_surface_id, model_terminal_route) = app_state
            .lock()
            .ok()
            .map(|app| {
                (
                    app.current_input_surface_id(),
                    app.current_terminal_input_route(),
                )
            })
            .unwrap_or_default();
        if let Some((surface_id, controls)) =
            active_focused_browser_controls(&browser_controls, focused_widget.as_ref())
        {
            if browser_widget_owns_model_focus(&surface_id, model_surface_id.as_deref()) {
                if is_plain_escape(keyval, modifiers) {
                    match browser_focus_escape_for_press
                        .borrow_mut()
                        .press(&surface_id, Instant::now())
                    {
                        BrowserFocusEscapeDecision::Forward => return glib::Propagation::Proceed,
                        BrowserFocusEscapeDecision::Consume => return glib::Propagation::Stop,
                        BrowserFocusEscapeDecision::Exit => {
                            call_app(
                                &app_state,
                                "browser.focus_mode.set",
                                json!({"surface_id": surface_id, "mode": "exit"}),
                            );
                            controls.focus_mode_active.set(false);
                            controls.focus_mode.set_active(false);
                            controls
                                .focus_mode
                                .set_tooltip_text(Some("Enter Browser Focus Mode"));
                            return glib::Propagation::Stop;
                        }
                    }
                }
                browser_focus_escape_for_press.borrow_mut().clear();
                return glib::Propagation::Proceed;
            }
        }
        browser_focus_escape_for_press.borrow_mut().clear();
        let model_terminal_surface_id = model_terminal_route
            .as_ref()
            .map(|(surface_id, _)| surface_id.clone());
        let model_input_queued = model_terminal_route
            .as_ref()
            .is_some_and(|(_, queued)| *queued);
        let (focused_ghostty_surface_id, focused_ghostty_widget, model_ghostty_surface_id) = {
            let widgets = ghostty_widgets.borrow();
            let focused = focused_widget.as_ref().and_then(|focused| {
                widgets
                    .iter()
                    .find(|(_, widget)| widget.contains_widget(focused))
                    .map(|(surface_id, widget)| (surface_id.clone(), widget.clone()))
            });
            (
                focused.as_ref().map(|(surface_id, _)| surface_id.clone()),
                focused.map(|(_, widget)| widget),
                model_terminal_surface_id,
            )
        };
        let focused_webkit_widget = focused_widget.as_ref().is_some_and(|focused| {
            browser_controls.borrow().values().any(|controls| {
                controls
                    .web_view
                    .as_ref()
                    .is_some_and(|view| widget_is_or_descendant_of(focused, view.widget()))
            })
        });
        if let Some(widget) = focused_ghostty_widget
            .as_ref()
            .filter(|widget| widget.copy_mode_active())
        {
            if widget.handle_keyboard_copy_mode_key(keyval, keycode, modifiers) {
                return glib::Propagation::Stop;
            }
        }
        let text_view_focused = focused_widget
            .as_ref()
            .is_some_and(|widget| widget.is::<gtk::TextView>());
        let editable_focused = focused_widget.as_ref().is_some_and(|widget| {
            widget.is::<gtk::Editable>()
                || widget.is::<gtk::Entry>()
                || widget.is::<gtk::SearchEntry>()
                || widget.is::<gtk::TextView>()
        });
        let browser_location_focused = focused_widget
            .as_ref()
            .is_some_and(|widget| widget.has_css_class("cmux-browser-location"));
        let browser_location_navigation_key = browser_location_focused
            && !modifiers.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SUPER_MASK
                    | gdk::ModifierType::META_MASK,
            )
            && matches!(
                keyval,
                gdk::Key::Up
                    | gdk::Key::Down
                    | gdk::Key::Return
                    | gdk::Key::KP_Enter
                    | gdk::Key::Escape
            );
        if browser_location_navigation_key {
            return glib::Propagation::Proceed;
        }
        let diff_focused =
            widget_or_ancestor_has_css_class(focused_widget.as_ref(), "cmux-surface-diff");
        let chord_pending = shortcut_chord_pending(&app_state);
        let palette_open = palette_visible(&app_state);
        let combo = if chord_pending {
            app_shortcut_combo_for_key(keyval, modifiers).or_else(|| shortcut_key_name(keyval))
        } else if palette_open {
            command_palette_submit_combo(keyval, modifiers)
                .or_else(|| app_shortcut_combo_for_key(keyval, modifiers))
        } else if diff_focused && !editable_focused {
            diff_shortcut_combo_for_key(keyval, modifiers)
        } else if editable_focused && !browser_location_focused && !text_view_focused {
            None
        } else {
            app_shortcut_combo_for_key(keyval, modifiers)
        };
        if chord_pending && combo.is_none() {
            if let Ok(mut app) = app_state.lock() {
                app.cancel_pending_shortcut_chord();
            }
        }
        if let Some(combo) = combo.as_deref() {
            let previous_model_surface_id = model_surface_id.clone();
            let result = call_app_value(
                &app_state,
                "debug.shortcut.simulate",
                json!({
                    "combo": combo,
                    "context": shortcut_focus_context(focused_widget.as_ref())
                }),
            );
            let handled = result
                .as_ref()
                .is_some_and(|value| value.get("handled").and_then(Value::as_bool) != Some(false));
            if handled {
                if result
                    .as_ref()
                    .is_some_and(|value| value.get("quit").and_then(Value::as_bool) == Some(true))
                {
                    if let Some(application) = weak_window
                        .upgrade()
                        .and_then(|window| window.application())
                    {
                        application.quit();
                    }
                    return glib::Propagation::Stop;
                }
                if let Some(result) = result.as_ref() {
                    apply_document_shortcut_result(&app_state, focused_widget.as_ref(), result);
                    apply_diff_shortcut_result(&diff_controls, result);
                    apply_terminal_shortcut_result(&ghostty_widgets, result);
                    apply_clipboard_shortcut_result(result);
                }
                if let Some(result) = result.filter(browser_shortcut_result) {
                    if !apply_browser_shortcut_result(&browser_controls, &result) {
                        queue_pending_browser_shortcut_action(
                            &browser_controls,
                            &pending_browser_shortcut_actions,
                            result,
                        );
                    }
                }
                focus_changed_model_surface(
                    &app_state,
                    &ghostty_widgets,
                    &browser_controls,
                    previous_model_surface_id.as_deref(),
                );
                return glib::Propagation::Stop;
            }
        }
        let ghostty_focus_in_transition = model_ghostty_surface_id.is_some()
            && model_ghostty_surface_id != focused_ghostty_surface_id;
        if ghostty_focus_in_transition
            || model_input_queued
            || focused_ghostty_widget
                .as_ref()
                .is_some_and(|widget| !widget.input_ready())
        {
            let Some(input) = terminal_input_for_key(keyval, modifiers) else {
                return glib::Propagation::Proceed;
            };
            return if dispatch_terminal_input(&app_state, input) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            };
        }
        if focused_ghostty_widget.is_some() || focused_webkit_widget || editable_focused {
            return glib::Propagation::Proceed;
        }
        let Some(input) = terminal_input_for_key(keyval, modifiers) else {
            return glib::Propagation::Proceed;
        };
        if dispatch_terminal_input(&app_state, input) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    let weak_window_for_release = window.downgrade();
    controller.connect_key_released(move |_, keyval, _, modifiers| {
        if !is_plain_escape(keyval, modifiers) {
            return;
        }
        let focused_widget = weak_window_for_release
            .upgrade()
            .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window));
        if let Some((surface_id, _)) =
            active_focused_browser_controls(&browser_controls_for_release, focused_widget.as_ref())
        {
            browser_focus_escape.borrow_mut().release(&surface_id);
        }
    });
    window.add_controller(controller);
}

fn command_palette_submit_combo(keyval: gdk::Key, modifiers: gdk::ModifierType) -> Option<String> {
    if !matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter) {
        return None;
    }
    Some(
        if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
            "shift+enter"
        } else {
            "enter"
        }
        .to_string(),
    )
}

fn diff_shortcut_combo_for_key(keyval: gdk::Key, modifiers: gdk::ModifierType) -> Option<String> {
    if modifiers.intersects(
        gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK,
    ) {
        return app_shortcut_combo_for_key(keyval, modifiers);
    }
    let key = shortcut_key_name(keyval)?;
    Some(if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        format!("shift+{key}")
    } else {
        key
    })
}

fn apply_document_shortcut_result(
    app_state: &Arc<Mutex<AppState>>,
    focused_widget: Option<&gtk::Widget>,
    result: &Value,
) -> bool {
    if result.get("document_action").and_then(Value::as_str) != Some("save") {
        return false;
    }
    let Some(surface_id) = result.get("surface_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(text_view) = focused_widget.and_then(|widget| widget.downcast_ref::<gtk::TextView>())
    else {
        return false;
    };
    let buffer = text_view.buffer();
    let content = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    call_app(
        app_state,
        "document.save",
        json!({"surface_id": surface_id, "content": content}),
    )
}

fn clipboard_text_from_shortcut_result(result: &Value) -> Option<&str> {
    (result.get("clipboard_action").and_then(Value::as_str) == Some("copy"))
        .then(|| result.get("clipboard_text").and_then(Value::as_str))
        .flatten()
}

fn apply_clipboard_shortcut_result(result: &Value) -> bool {
    let Some(text) = clipboard_text_from_shortcut_result(result) else {
        return false;
    };
    let Some(display) = gdk::Display::default() else {
        return false;
    };
    display.clipboard().set_text(text);
    true
}

fn diff_scroll_value(
    action: &str,
    value: f64,
    lower: f64,
    upper: f64,
    page_size: f64,
    step_increment: f64,
) -> Option<f64> {
    let maximum = (upper - page_size).max(lower);
    let step = step_increment.max(32.0);
    let target = match action {
        "scroll_down" => value + step,
        "scroll_up" => value - step,
        "scroll_to_bottom" => maximum,
        "scroll_to_top" => lower,
        _ => return None,
    };
    Some(target.clamp(lower, maximum))
}

fn apply_diff_shortcut_result(cache: &DiffSurfaceControlsCache, result: &Value) -> bool {
    let Some(action) = result.get("diff_shortcut_action").and_then(Value::as_str) else {
        return false;
    };
    let Some(surface_id) = result.get("surface_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(controls) = cache.borrow().get(surface_id).cloned() else {
        return false;
    };
    if action == "open_file_search" {
        controls.search_row.set_visible(true);
        controls.search.grab_focus();
        controls.search.select_region(0, -1);
        return true;
    }
    let adjustment = controls.scroll.vadjustment();
    let Some(value) = diff_scroll_value(
        action,
        adjustment.value(),
        adjustment.lower(),
        adjustment.upper(),
        adjustment.page_size(),
        adjustment.step_increment(),
    ) else {
        return false;
    };
    adjustment.set_value(value);
    true
}

fn apply_terminal_shortcut_result(cache: &GhosttySurfaceWidgets, result: &Value) -> bool {
    if result
        .get("terminal_shortcut_action")
        .and_then(Value::as_str)
        != Some("toggle_copy_mode")
    {
        return false;
    }
    let Some(surface_id) = result.get("surface_id").and_then(Value::as_str) else {
        return false;
    };
    let active = result
        .get("terminal_copy_mode_active")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    cache
        .borrow()
        .get(surface_id)
        .is_some_and(|widget| widget.set_keyboard_copy_mode_active(active))
}

fn browser_shortcut_result(value: &Value) -> bool {
    value
        .get("browser_shortcut_action")
        .and_then(Value::as_str)
        .is_some()
        && value.get("surface_id").and_then(Value::as_str).is_some()
}

fn apply_browser_shortcut_result(cache: &BrowserSurfaceControlsCache, result: &Value) -> bool {
    let Some(surface_id) = result.get("surface_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(action) = result
        .get("browser_shortcut_action")
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(controls) = cache.borrow().get(surface_id).cloned() else {
        return false;
    };
    match action {
        "enter_focus_mode" | "toggle_focus_mode" => {
            let active = result
                .get("focus_mode_active")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            controls.focus_mode_active.set(active);
            controls.focus_mode.set_active(active);
            controls.focus_mode.set_tooltip_text(Some(if active {
                "Exit Browser Focus Mode"
            } else {
                "Enter Browser Focus Mode"
            }));
            if active {
                if let Some(view) = controls.web_view.as_ref() {
                    view.widget().grab_focus();
                }
            }
        }
        "toggle_react_grab" => {
            if let Some(view) = controls.web_view.as_ref() {
                let active = result
                    .get("react_grab_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                view.evaluate_javascript(&react_grab_runtime_script(active));
                view.widget().grab_focus();
            }
        }
        "focus_address_bar" => {
            controls.browser_chrome_focused.set(true);
            focus_browser_chrome_widget(
                controls.location.clone().upcast(),
                Rc::clone(&controls.browser_chrome_focused),
            );
            controls.location.select_region(0, -1);
        }
        "find" => {
            controls.browser_chrome_focused.set(true);
            controls.find_bar.set_visible(true);
            focus_browser_chrome_widget(
                controls.find_entry.clone().upcast(),
                Rc::clone(&controls.browser_chrome_focused),
            );
            controls.find_entry.select_region(0, -1);
            if let Some(view) = controls.web_view.as_ref() {
                view.find_text(controls.find_entry.text().as_str());
            }
        }
        "back" => {
            if let Some(view) = controls.web_view.as_ref() {
                view.go_back();
            }
        }
        "forward" => {
            if let Some(view) = controls.web_view.as_ref() {
                view.go_forward();
            }
        }
        "reload" => {
            if let Some(view) = controls.web_view.as_ref() {
                view.reload();
            }
        }
        "global_search" => {
            let Some(needle) = result
                .get("browser_search_needle")
                .or_else(|| result.get("search_needle"))
                .and_then(Value::as_str)
                .filter(|needle| !needle.is_empty())
            else {
                return true;
            };
            controls
                .global_search_needle
                .replace(Some(needle.to_string()));
            if let Some(view) = controls.web_view.as_ref() {
                view.evaluate_javascript(&browser_global_search_script(needle));
                view.widget().grab_focus();
            }
        }
        "hard_reload" => {
            if let Some(view) = controls.web_view.as_ref() {
                view.reload_bypass_cache();
            }
        }
        "zoom_in" | "zoom_out" | "zoom_reset" => {
            if let (Some(view), Some(level)) = (
                controls.web_view.as_ref(),
                result.get("page_zoom").and_then(Value::as_f64),
            ) {
                view.set_zoom_level(level);
            }
        }
        "toggle_developer_tools" | "show_javascript_console" => {
            if let Some(view) = controls.web_view.as_ref() {
                let visible = result
                    .get("developer_tools_visible")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                view.set_inspector_visible(visible);
            }
        }
        _ => return false,
    }
    true
}

fn focus_browser_chrome_widget(widget: gtk::Widget, chrome_focus_requested: Rc<Cell<bool>>) {
    widget.grab_focus();
    let immediate = widget.clone();
    let immediate_focus_requested = Rc::clone(&chrome_focus_requested);
    glib::idle_add_local_once(move || {
        if immediate_focus_requested.get() && !immediate.has_focus() {
            immediate.grab_focus();
        }
    });
    let attempts = Rc::new(Cell::new(0_u8));
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if !chrome_focus_requested.get() || widget.has_focus() {
            return glib::ControlFlow::Break;
        }
        let attempt = attempts.get().saturating_add(1);
        attempts.set(attempt);
        widget.grab_focus();
        if attempt >= BROWSER_FOCUS_RETRY_ATTEMPTS {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn react_grab_runtime_script(active: bool) -> String {
    if !active {
        return "window.__REACT_GRAB__?.deactivate()".to_string();
    }
    format!(
        r#"(function() {{
            var activate = function(api) {{ if (api && api.activate) api.activate(); }};
            if (window.__REACT_GRAB__) {{ activate(window.__REACT_GRAB__); return; }}
            window.addEventListener('react-grab:init', function(event) {{
                activate(event.detail);
            }}, {{ once: true }});
            if (document.querySelector('script[data-cmux-react-grab]')) return;
            var script = document.createElement('script');
            script.dataset.cmuxReactGrab = '{}';
            script.src = 'https://unpkg.com/react-grab@{}/dist/index.global.js';
            script.integrity = '{}';
            script.crossOrigin = 'anonymous';
            (document.head || document.documentElement).appendChild(script);
        }})()"#,
        REACT_GRAB_VERSION, REACT_GRAB_VERSION, REACT_GRAB_INTEGRITY
    )
}

fn browser_global_search_script(needle: &str) -> String {
    let encoded = serde_json::to_string(needle).unwrap_or_else(|_| "\"\"".to_string());
    format!("window.find({encoded}, false, false, true, false, true, false)")
}

fn active_focused_browser_controls(
    cache: &BrowserSurfaceControlsCache,
    focused: Option<&gtk::Widget>,
) -> Option<(String, BrowserSurfaceControls)> {
    let focused = focused?;
    cache.borrow().iter().find_map(|(surface_id, controls)| {
        let web_view = controls.web_view.as_ref()?;
        (controls.focus_mode_active.get() && widget_is_or_descendant_of(focused, web_view.widget()))
            .then(|| (surface_id.clone(), controls.clone()))
    })
}

fn browser_widget_owns_model_focus(
    focused_surface_id: &str,
    model_surface_id: Option<&str>,
) -> bool {
    model_surface_id == Some(focused_surface_id)
}

fn focus_changed_model_surface(
    app_state: &Arc<Mutex<AppState>>,
    ghostty_widgets: &GhosttySurfaceWidgets,
    browser_controls: &BrowserSurfaceControlsCache,
    previous_surface_id: Option<&str>,
) -> bool {
    let current_surface_id = app_state
        .lock()
        .ok()
        .and_then(|app| app.current_input_surface_id());
    let Some(current_surface_id) = current_surface_id else {
        return false;
    };
    if previous_surface_id == Some(current_surface_id.as_str()) {
        return false;
    }
    if let Some(widget) = ghostty_widgets.borrow().get(&current_surface_id).cloned() {
        widget.grab_focus();
        return true;
    }
    let controls = browser_controls.borrow().get(&current_surface_id).cloned();
    let Some(controls) = controls else {
        return false;
    };
    let Some(view) = controls.web_view.as_ref() else {
        return false;
    };
    controls.browser_chrome_focused.set(false);
    view.widget().grab_focus();
    true
}

fn widget_is_or_descendant_of(widget: &gtk::Widget, ancestor: &gtk::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(widget) = current {
        if widget == *ancestor {
            return true;
        }
        current = widget.parent();
    }
    false
}

fn is_plain_escape(keyval: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    keyval == gdk::Key::Escape
        && !modifiers.intersects(
            gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK,
        )
}

fn flush_pending_browser_shortcut_actions(
    cache: &BrowserSurfaceControlsCache,
    pending: &PendingBrowserShortcutActions,
) {
    let actions = pending.borrow_mut().drain(..).collect::<Vec<_>>();
    let mut remaining = Vec::new();
    for action in actions {
        if !apply_browser_shortcut_result(cache, &action) {
            remaining.push(action);
        }
    }
    pending.borrow_mut().extend(remaining);
}

fn queue_pending_browser_shortcut_action(
    cache: &BrowserSurfaceControlsCache,
    pending: &PendingBrowserShortcutActions,
    action: Value,
) {
    pending.borrow_mut().push(action);
    let cache = Rc::clone(cache);
    let pending = Rc::clone(pending);
    let attempts = Rc::new(Cell::new(0_u8));
    glib::timeout_add_local(Duration::from_millis(16), move || {
        flush_pending_browser_shortcut_actions(&cache, &pending);
        if pending.borrow().is_empty() {
            return glib::ControlFlow::Break;
        }
        let attempt = attempts.get().saturating_add(1);
        attempts.set(attempt);
        if attempt >= BROWSER_FOCUS_RETRY_ATTEMPTS {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn shortcut_focus_context(focused: Option<&gtk::Widget>) -> Value {
    let sidebar = widget_or_ancestor_has_css_class(focused, "cmux-chrome");
    let browser = !sidebar
        && (widget_or_ancestor_has_css_class(focused, "cmux-surface-browser")
            || widget_or_ancestor_has_css_class(focused, "cmux-surface-diff"));
    let markdown = !sidebar && widget_or_ancestor_has_css_class(focused, "cmux-surface-markdown");
    shortcut_focus_context_from_flags(sidebar, browser, markdown)
}

fn shortcut_focus_context_from_flags(sidebar: bool, browser: bool, markdown: bool) -> Value {
    let browser = !sidebar && browser;
    let markdown = !sidebar && !browser && markdown;
    json!({
        "sidebarFocus": sidebar,
        "browserFocus": browser,
        "markdownFocus": markdown,
        "terminalFocus": !sidebar && !browser && !markdown
    })
}

fn widget_or_ancestor_has_css_class(focused: Option<&gtk::Widget>, class: &str) -> bool {
    let mut current = focused.cloned();
    while let Some(widget) = current {
        if widget.has_css_class(class) {
            return true;
        }
        current = widget.parent();
    }
    false
}

fn focused_widget_is_activation_control(focused: Option<&gtk::Widget>) -> bool {
    let mut current = focused.cloned();
    while let Some(widget) = current {
        if widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() {
            return true;
        }
        current = widget.parent();
    }
    false
}

fn control_activation_key_should_propagate(
    focused_activation_control: bool,
    keyval: gdk::Key,
    modifiers: gdk::ModifierType,
) -> bool {
    focused_activation_control
        && matches!(
            keyval,
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space | gdk::Key::KP_Space
        )
        && !modifiers.intersects(
            gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK,
        )
}

fn widget_contains_focus(widget: &impl IsA<gtk::Widget>) -> bool {
    widget.has_focus() || widget.focus_child().is_some()
}

fn app_shortcut_combo_for_key(keyval: gdk::Key, modifiers: gdk::ModifierType) -> Option<String> {
    if !modifiers.intersects(
        gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK,
    ) {
        return None;
    }
    let mut key = shortcut_key_name(keyval)?;
    let layout_consumed_shift_for_digit = modifiers.contains(gdk::ModifierType::SHIFT_MASK)
        && key.len() == 1
        && key.as_bytes()[0].is_ascii_digit();
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        if let Some(digit) = shifted_number_key_digit(&key) {
            key = digit.to_string();
        }
    }
    let mut parts = Vec::new();
    if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
        parts.push("cmd".to_string());
    }
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("ctrl".to_string());
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("opt".to_string());
    }
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) && !layout_consumed_shift_for_digit {
        parts.push("shift".to_string());
    }
    parts.push(key);
    Some(parts.join("+"))
}

fn omnibar_pane_focus_combo(keyval: gdk::Key, modifiers: gdk::ModifierType) -> Option<String> {
    let combo = app_shortcut_combo_for_key(keyval, modifiers)?;
    matches!(
        combo.as_str(),
        "ctrl+shift+h"
            | "ctrl+shift+j"
            | "ctrl+shift+k"
            | "ctrl+shift+l"
            | "cmd+ctrl+h"
            | "cmd+ctrl+j"
            | "cmd+ctrl+k"
            | "cmd+ctrl+l"
    )
    .then_some(combo)
}

fn shifted_number_key_digit(key: &str) -> Option<u8> {
    match key {
        "!" => Some(1),
        "@" => Some(2),
        "#" => Some(3),
        "$" => Some(4),
        "%" => Some(5),
        "^" => Some(6),
        "&" => Some(7),
        "*" => Some(8),
        "(" => Some(9),
        _ => None,
    }
}

fn shortcut_chord_pending(app_state: &Arc<Mutex<AppState>>) -> bool {
    app_state
        .lock()
        .map(|app| app.shortcut_chord_pending_for_current_window())
        .unwrap_or(false)
}

fn shortcut_key_name(keyval: gdk::Key) -> Option<String> {
    if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter) {
        Some("enter".to_string())
    } else if keyval == gdk::Key::Escape {
        Some("escape".to_string())
    } else if keyval == gdk::Key::BackSpace {
        Some("backspace".to_string())
    } else if matches!(keyval, gdk::Key::Left | gdk::Key::KP_Left) {
        Some("left".to_string())
    } else if matches!(keyval, gdk::Key::Right | gdk::Key::KP_Right) {
        Some("right".to_string())
    } else if matches!(keyval, gdk::Key::Up | gdk::Key::KP_Up) {
        Some("up".to_string())
    } else if matches!(keyval, gdk::Key::Down | gdk::Key::KP_Down) {
        Some("down".to_string())
    } else if matches!(keyval, gdk::Key::Page_Up | gdk::Key::KP_Page_Up) {
        Some("pageup".to_string())
    } else if matches!(keyval, gdk::Key::Page_Down | gdk::Key::KP_Page_Down) {
        Some("pagedown".to_string())
    } else {
        keyval
            .to_unicode()
            .filter(|ch| ch.is_ascii_graphic())
            .map(|ch| ch.to_ascii_lowercase().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalInput {
    Text(String),
    Key(String),
}

fn terminal_input_for_key(keyval: gdk::Key, modifiers: gdk::ModifierType) -> Option<TerminalInput> {
    if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
        return None;
    }

    let key = if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter) {
        Some("enter")
    } else if keyval == gdk::Key::BackSpace {
        Some("backspace")
    } else if matches!(
        keyval,
        gdk::Key::Tab | gdk::Key::KP_Tab | gdk::Key::ISO_Left_Tab
    ) {
        Some("tab")
    } else if keyval == gdk::Key::Escape {
        Some("escape")
    } else if matches!(keyval, gdk::Key::Left | gdk::Key::KP_Left) {
        Some("left")
    } else if matches!(keyval, gdk::Key::Right | gdk::Key::KP_Right) {
        Some("right")
    } else if matches!(keyval, gdk::Key::Up | gdk::Key::KP_Up) {
        Some("up")
    } else if matches!(keyval, gdk::Key::Down | gdk::Key::KP_Down) {
        Some("down")
    } else if matches!(keyval, gdk::Key::Delete | gdk::Key::KP_Delete) {
        Some("delete")
    } else if matches!(keyval, gdk::Key::Home | gdk::Key::KP_Home) {
        Some("home")
    } else if matches!(keyval, gdk::Key::End | gdk::Key::KP_End) {
        Some("end")
    } else if matches!(keyval, gdk::Key::Page_Up | gdk::Key::KP_Page_Up) {
        Some("page-up")
    } else if matches!(keyval, gdk::Key::Page_Down | gdk::Key::KP_Page_Down) {
        Some("page-down")
    } else {
        None
    };
    if let Some(key) = key {
        return Some(terminal_key_input_with_modifiers(key, modifiers));
    }

    let ch = keyval.to_unicode()?;
    if ch.is_control() {
        return None;
    }
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        if ch.is_ascii_alphabetic() {
            return Some(terminal_key_input_with_modifiers(
                &ch.to_ascii_lowercase().to_string(),
                modifiers,
            ));
        }
        return None;
    }

    let mut text = ch.to_string();
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        text.insert(0, '\x1b');
    }
    Some(TerminalInput::Text(text))
}

fn terminal_key_input_with_modifiers(key: &str, modifiers: gdk::ModifierType) -> TerminalInput {
    let mut parts = Vec::new();
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("ctrl");
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("alt");
    }
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("shift");
    }
    parts.push(key);
    TerminalInput::Key(parts.join("-"))
}

fn dispatch_terminal_input(app_state: &Arc<Mutex<AppState>>, input: TerminalInput) -> bool {
    match input {
        TerminalInput::Text(text) => call_app(app_state, "debug.type", json!({"text": text})),
        TerminalInput::Key(key) => {
            if palette_visible(app_state) {
                if key == "escape" {
                    return call_app(app_state, "debug.command_palette.toggle", json!({}));
                }
                if key == "backspace" {
                    return call_app(
                        app_state,
                        "debug.command_palette.delete_backward",
                        json!({}),
                    );
                }
                if let Some(combo) = palette_shortcut_combo(&key) {
                    return call_app(
                        app_state,
                        "debug.shortcut.simulate",
                        json!({"combo": combo}),
                    );
                }
            }
            call_app(app_state, "surface.send_key", json!({"key": key}))
        }
    }
}

fn call_app(app_state: &Arc<Mutex<AppState>>, method: &str, params: Value) -> bool {
    call_app_value(app_state, method, params).is_some()
}

fn call_app_value(app_state: &Arc<Mutex<AppState>>, method: &str, params: Value) -> Option<Value> {
    let Ok(mut app) = app_state.lock() else {
        return None;
    };
    app.handle(method, &params).ok()
}

fn palette_visible(app_state: &Arc<Mutex<AppState>>) -> bool {
    let Ok(mut app) = app_state.lock() else {
        return false;
    };
    app.handle("debug.command_palette.visible", &json!({}))
        .ok()
        .and_then(|value| value.get("visible").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn palette_shortcut_combo(key: &str) -> Option<&'static str> {
    match key {
        "up" => Some("up"),
        "down" => Some("down"),
        "page-up" => Some("pageup"),
        "page-down" => Some("pagedown"),
        "enter" => Some("enter"),
        _ => None,
    }
}

fn label(text: &str, css_class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(css_class);
    label.set_xalign(0.0);
    label
}

fn section_heading(text: &str) -> gtk::Label {
    let heading = label(text, "cmux-section-heading");
    heading.add_css_class("cmux-section");
    heading
}

fn row_label(text: &str, css_class: &str) -> gtk::Label {
    let label = label(text, css_class);
    label.set_wrap(true);
    label
}

fn value_str<'a>(value: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalDisplay {
    text: String,
    markup: Option<String>,
}

impl TerminalDisplay {
    fn plain(text: String) -> Self {
        Self { text, markup: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalCursor {
    row: usize,
    column: usize,
    style: TerminalCursorStyle,
    blinking: bool,
}

impl TerminalCursor {
    fn from_json(cursor: &Value, row_key: &str, column_key: &str) -> Option<Self> {
        Some(Self {
            row: cursor.get(row_key).and_then(Value::as_u64)? as usize,
            column: cursor.get(column_key).and_then(Value::as_u64)? as usize,
            style: TerminalCursorStyle::from_value(cursor.get("style").and_then(Value::as_str)),
            blinking: cursor
                .get("blinking")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCursorStyle {
    Block,
    Underline,
    Bar,
}

impl TerminalCursorStyle {
    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("underline") => Self::Underline,
            Some("bar") => Self::Bar,
            _ => Self::Block,
        }
    }
}

fn terminal_display(view: &Value) -> Option<TerminalDisplay> {
    let ghostty_vt = view.get("ghostty_vt");
    if let Some(text) = ghostty_vt_render_text(ghostty_vt) {
        if terminal_frame_has_content(&text, ghostty_vt.and_then(ghostty_vt_cursor)) {
            return Some(TerminalDisplay {
                text,
                markup: ghostty_vt_render_markup(ghostty_vt),
            });
        }
    }

    let render_grid = view.get("render_grid");
    if let Some(text) = render_grid_text(render_grid) {
        if terminal_frame_has_content(&text, render_grid.and_then(render_grid_cursor)) {
            return Some(TerminalDisplay {
                text,
                markup: render_grid_markup(render_grid),
            });
        }
    }

    view.get("preview")
        .and_then(Value::as_str)
        .map(|text| TerminalDisplay::plain(text.to_string()))
}

fn terminal_frame_has_content(text: &str, cursor: Option<TerminalCursor>) -> bool {
    !text.trim().is_empty() || cursor.is_some()
}

fn terminal_preview_display(view: &Value) -> TerminalDisplay {
    let display = terminal_display(view).unwrap_or_else(|| TerminalDisplay::plain(String::new()));
    if display.text.is_empty() {
        if terminal_loading(view) {
            return TerminalDisplay::plain(
                value_str(view, "loading_message", "Loading terminal...").to_string(),
            );
        }
        return TerminalDisplay::plain(" ".to_string());
    }
    display
}

fn terminal_status_display(view: &Value) -> Option<String> {
    let mut parts = Vec::new();

    if terminal_bool(view, "terminal_readonly", "readonly") {
        parts.push("Read-only".to_string());
    }
    if terminal_bool(view, "terminal_needs_confirm_quit", "needs_confirm_quit") {
        parts.push("Close confirmation required".to_string());
    }
    if terminal_bool(view, "terminal_mouse_captured", "mouse_captured") {
        parts.push("Mouse captured".to_string());
    }
    if terminal_bool(view, "terminal_has_selection", "has_selection") {
        parts.push("Selection active".to_string());
    }
    if let Some(url) = view
        .get("terminal_mouse_over_link_url")
        .or_else(|| view.get("mouse_over_link_url"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Link: {}", compact_status_text(url, 64)));
    }
    if !terminal_bool(view, "terminal_cursor_visible", "cursor_visible")
        && (view.get("terminal_cursor_visible").is_some() || view.get("cursor_visible").is_some())
    {
        parts.push("Cursor hidden".to_string());
    }
    if let Some(shape) = view
        .get("terminal_cursor_shape")
        .or_else(|| view.get("cursor_shape"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Cursor: {}", compact_status_text(shape, 24)));
    }
    if let Some(count) = view
        .get("terminal_config_reload_count")
        .and_then(Value::as_u64)
        .filter(|count| *count > 0)
    {
        let mode = match view
            .get("terminal_last_config_reload_soft")
            .and_then(Value::as_bool)
        {
            Some(true) => "soft",
            Some(false) => "hard",
            None => "unknown",
        };
        parts.push(format!("Config reload: {mode} #{count}"));
    }

    if let Some(progress) = view
        .get("terminal_progress")
        .filter(|value| value.is_object())
    {
        let state = value_str(progress, "state", "");
        if !state.is_empty() {
            let label = match state {
                "set" => "Progress",
                "error" => "Progress error",
                "indeterminate" => "Progress active",
                "pause" => "Progress paused",
                other => other,
            };
            if let Some(percent) = progress
                .get("percent")
                .and_then(Value::as_i64)
                .filter(|value| (0..=100).contains(value))
            {
                parts.push(format!("{label}: {percent}%"));
            } else {
                parts.push(label.to_string());
            }
        }
    }

    if view
        .get("terminal_key_sequence")
        .and_then(|value| value.get("active"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let trigger = view
            .get("terminal_key_sequence")
            .and_then(|value| value.get("trigger"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        parts.push(match trigger {
            Some(trigger) => format!("Key sequence: {trigger}"),
            None => "Key sequence active".to_string(),
        });
    }

    if let Some(table) = view
        .get("terminal_key_table")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Key table: {table}"));
    }

    if let Some(command) = view
        .get("terminal_last_command")
        .filter(|value| value.is_object())
    {
        let status = match command.get("exit_code").and_then(Value::as_i64) {
            Some(0) => "succeeded".to_string(),
            Some(code) => format!("failed ({code})"),
            None => "finished".to_string(),
        };
        let duration = command
            .get("duration_ms")
            .and_then(Value::as_u64)
            .map(format_duration_ms);
        parts.push(match duration {
            Some(duration) => format!("Last command {status} in {duration}"),
            None => format!("Last command {status}"),
        });
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn terminal_bool(view: &Value, primary: &str, alias: &str) -> bool {
    view.get(primary)
        .or_else(|| view.get(alias))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn compact_status_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = normalized.chars().count();
    if count <= max_chars {
        return normalized;
    }
    let suffix = "...";
    let mut truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(suffix.len()))
        .collect::<String>();
    if let Some(index) = truncated
        .rfind(|ch: char| ch == '/' || ch.is_whitespace())
        .filter(|index| *index >= max_chars / 2)
    {
        truncated.truncate(index);
    }
    truncated.push_str(suffix);
    truncated
}

fn format_duration_ms(duration_ms: u64) -> String {
    if duration_ms >= 1000 {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    } else {
        format!("{duration_ms}ms")
    }
}

fn surface_preview_display(view: &Value) -> TerminalDisplay {
    let kind = value_str(view, "kind", "surface");
    let title = value_str(view, "title", "");
    let url = value_str(view, "url", "");
    let preview = view
        .get("preview")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = Vec::new();

    match kind {
        "browser" => {
            let browser = view.get("browser").unwrap_or(&Value::Null);
            push_non_empty_line(
                &mut lines,
                browser
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(title),
            );
            push_non_empty_line(&mut lines, url);
            if browser
                .get("developer_tools_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                lines.push("Developer tools open".to_string());
            }
            if browser
                .get("import_dialog_opened")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let scope = browser
                    .get("import_dialog_scope")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("cookiesOnly");
                lines.push(format!("Browser import dialog open ({scope})"));
            }
            push_non_empty_line(&mut lines, preview);
        }
        "project" => {
            let project = view.get("project").unwrap_or(&Value::Null);
            push_non_empty_line(&mut lines, title);
            push_non_empty_line(
                &mut lines,
                project
                    .get("project_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            push_non_empty_line(&mut lines, preview);
        }
        _ => {
            push_non_empty_line(&mut lines, title);
            push_non_empty_line(&mut lines, url);
            push_non_empty_line(&mut lines, preview);
        }
    }

    let text = lines
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    TerminalDisplay::plain(if text.is_empty() {
        " ".to_string()
    } else {
        text
    })
}

fn push_non_empty_line(lines: &mut Vec<String>, text: &str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        lines.push(trimmed.to_string());
    }
}

fn terminal_loading(view: &Value) -> bool {
    view.get("terminal_loading")
        .or_else(|| view.get("loading"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn ghostty_vt_render_text(render: Option<&Value>) -> Option<String> {
    let render = render?;
    let row_count = render
        .get("row_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            render
                .get("rows")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        }) as usize;
    let mut lines = vec![String::new(); row_count.max(1)];
    let cursor = ghostty_vt_cursor(render);
    for row in render
        .get("rows_data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let y = row.get("y").and_then(Value::as_u64).unwrap_or(0) as usize;
        if y >= lines.len() {
            lines.resize(y + 1, String::new());
        }
        let line = &mut lines[y];
        for cell in row
            .get("cells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(text) = cell.get("text").and_then(Value::as_str) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let x = cell.get("x").and_then(Value::as_u64).unwrap_or(0) as usize;
            pad_to_cell(line, x);
            line.push_str(text);
        }
    }
    if let Some(cursor) = cursor {
        if cursor.row >= lines.len() {
            lines.resize(cursor.row + 1, String::new());
        }
        let current_col = lines[cursor.row].chars().count();
        if current_col <= cursor.column {
            pad_to_cell(&mut lines[cursor.row], cursor.column);
            lines[cursor.row].push(' ');
        }
    }
    trim_trailing_blank_text_lines(&mut lines, cursor.map(|cursor| cursor.row));
    Some(lines.join("\n"))
}

fn ghostty_vt_render_markup(render: Option<&Value>) -> Option<String> {
    let render = render?;
    let row_count = render
        .get("row_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            render
                .get("rows")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        }) as usize;
    let mut lines = vec![MarkupLine::default(); row_count.max(1)];
    let cursor = ghostty_vt_cursor(render);
    for row in render
        .get("rows_data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let y = row.get("y").and_then(Value::as_u64).unwrap_or(0) as usize;
        if y >= lines.len() {
            lines.resize(y + 1, MarkupLine::default());
        }
        let line = &mut lines[y];
        for cell in row
            .get("cells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(text) = cell.get("text").and_then(Value::as_str) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let x = cell.get("x").and_then(Value::as_u64).unwrap_or(0) as usize;
            pad_markup_line_to_cell(line, y, x, cursor);
            line.text.push_str(text);
            if let Some(cursor) = cursor_at(cursor, y, x) {
                line.markup.push_str(&cursor_cell_markup(text, cursor));
            } else {
                line.markup.push_str(&cell_markup(cell, text));
            }
        }
    }
    if let Some(cursor) = cursor {
        if cursor.row >= lines.len() {
            lines.resize(cursor.row + 1, MarkupLine::default());
        }
        let current_col = lines[cursor.row].text.chars().count();
        if current_col <= cursor.column {
            pad_markup_line_to_cell(
                &mut lines[cursor.row],
                cursor.row,
                cursor.column,
                Some(cursor),
            );
            lines[cursor.row].text.push(' ');
            lines[cursor.row]
                .markup
                .push_str(&cursor_cell_markup(" ", cursor));
        }
    }
    trim_trailing_blank_markup_lines(&mut lines, cursor.map(|cursor| cursor.row));
    Some(
        lines
            .into_iter()
            .map(|line| line.markup)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn trim_trailing_blank_text_lines(lines: &mut Vec<String>, protected_row: Option<usize>) {
    while lines.len() > 1
        && protected_row != Some(lines.len() - 1)
        && lines
            .last()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
    {
        lines.pop();
    }
}

fn trim_trailing_blank_markup_lines(lines: &mut Vec<MarkupLine>, protected_row: Option<usize>) {
    while lines.len() > 1
        && protected_row != Some(lines.len() - 1)
        && lines
            .last()
            .map(|line| line.text.trim().is_empty())
            .unwrap_or(false)
    {
        lines.pop();
    }
}

fn ghostty_vt_cursor(render: &Value) -> Option<TerminalCursor> {
    let cursor = render.get("cursor")?;
    if !cursor
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || !cursor
            .get("in_viewport")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return None;
    }
    TerminalCursor::from_json(cursor, "y", "x")
}

fn render_grid_text(render: Option<&Value>) -> Option<String> {
    let render = render?;
    if render.get("format").and_then(Value::as_str) != Some("cmux.render-grid.v1") {
        return None;
    }
    let row_count = render
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let mut lines = vec![String::new(); row_count.max(1)];
    let cursor = render_grid_cursor(render);
    for span in render
        .get("row_spans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let row = span.get("row").and_then(Value::as_u64).unwrap_or(0) as usize;
        if row >= lines.len() {
            lines.resize(row + 1, String::new());
        }
        let Some(text) = span.get("text").and_then(Value::as_str) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let column = span.get("column").and_then(Value::as_u64).unwrap_or(0) as usize;
        let line = &mut lines[row];
        pad_to_cell(line, column);
        line.push_str(text);
    }
    if let Some(cursor) = cursor {
        if cursor.row >= lines.len() {
            lines.resize(cursor.row + 1, String::new());
        }
        let current_col = lines[cursor.row].chars().count();
        if current_col <= cursor.column {
            pad_to_cell(&mut lines[cursor.row], cursor.column);
            lines[cursor.row].push(' ');
        }
    }
    trim_trailing_blank_text_lines(&mut lines, cursor.map(|cursor| cursor.row));
    Some(lines.join("\n"))
}

fn render_grid_markup(render: Option<&Value>) -> Option<String> {
    let render = render?;
    if render.get("format").and_then(Value::as_str) != Some("cmux.render-grid.v1") {
        return None;
    }
    let row_count = render
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let mut lines = vec![MarkupLine::default(); row_count.max(1)];
    let styles = render.get("styles").and_then(Value::as_array);
    let cursor = render_grid_cursor(render);

    for span in render
        .get("row_spans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let row = span.get("row").and_then(Value::as_u64).unwrap_or(0) as usize;
        if row >= lines.len() {
            lines.resize(row + 1, MarkupLine::default());
        }
        let Some(text) = span.get("text").and_then(Value::as_str) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let column = span.get("column").and_then(Value::as_u64).unwrap_or(0) as usize;
        let style = render_grid_span_style(span, styles);
        pad_markup_line_to_cell(&mut lines[row], row, column, cursor);
        append_render_grid_span_markup(&mut lines[row], row, column, text, style, cursor);
    }
    if let Some(cursor) = cursor {
        if cursor.row >= lines.len() {
            lines.resize(cursor.row + 1, MarkupLine::default());
        }
        let current_col = lines[cursor.row].text.chars().count();
        if current_col <= cursor.column {
            pad_markup_line_to_cell(
                &mut lines[cursor.row],
                cursor.row,
                cursor.column,
                Some(cursor),
            );
            lines[cursor.row].text.push(' ');
            lines[cursor.row]
                .markup
                .push_str(&cursor_cell_markup(" ", cursor));
        }
    }

    trim_trailing_blank_markup_lines(&mut lines, cursor.map(|cursor| cursor.row));
    Some(
        lines
            .into_iter()
            .map(|line| line.markup)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn render_grid_cursor(render: &Value) -> Option<TerminalCursor> {
    let cursor = render.get("cursor")?;
    if !cursor
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    TerminalCursor::from_json(cursor, "row", "column")
}

fn pad_markup_line_to_cell(
    line: &mut MarkupLine,
    row: usize,
    target_col: usize,
    cursor: Option<TerminalCursor>,
) {
    let mut current_col = line.text.chars().count();
    while current_col < target_col {
        line.text.push(' ');
        if let Some(cursor) = cursor_at(cursor, row, current_col) {
            line.markup.push_str(&cursor_cell_markup(" ", cursor));
        } else {
            line.markup.push(' ');
        }
        current_col += 1;
    }
}

fn append_render_grid_span_markup(
    line: &mut MarkupLine,
    row: usize,
    column: usize,
    text: &str,
    style: Option<&Value>,
    cursor: Option<TerminalCursor>,
) {
    for (offset, ch) in text.chars().enumerate() {
        let cell_text = ch.to_string();
        line.text.push(ch);
        if let Some(cursor) = cursor_at(cursor, row, column + offset) {
            line.markup
                .push_str(&cursor_cell_markup(cell_text.as_str(), cursor));
        } else {
            line.markup
                .push_str(&render_grid_cell_markup(cell_text.as_str(), style));
        }
    }
}

fn render_grid_span_style<'a>(span: &Value, styles: Option<&'a Vec<Value>>) -> Option<&'a Value> {
    let style_id = span.get("style_id").and_then(Value::as_u64)?;
    styles?
        .iter()
        .find(|style| style.get("id").and_then(Value::as_u64) == Some(style_id))
}

fn cursor_at(cursor: Option<TerminalCursor>, row: usize, column: usize) -> Option<TerminalCursor> {
    cursor.filter(|cursor| cursor.row == row && cursor.column == column)
}

fn cursor_cell_markup(text: &str, cursor: TerminalCursor) -> String {
    let escaped = glib::markup_escape_text(text).to_string();
    let color = if cursor.blinking {
        "#73c7df"
    } else {
        "#4aa3c7"
    };
    match cursor.style {
        TerminalCursorStyle::Block => {
            format!("<span foreground=\"#070809\" background=\"{color}\">{escaped}</span>")
        }
        TerminalCursorStyle::Underline => {
            format!("<span foreground=\"{color}\" underline=\"single\">{escaped}</span>")
        }
        TerminalCursorStyle::Bar => format!("<span foreground=\"{color}\">|</span>{escaped}"),
    }
}

fn render_grid_cell_markup(text: &str, style: Option<&Value>) -> String {
    let escaped = glib::markup_escape_text(text).to_string();
    let Some(style) = style else {
        return escaped;
    };
    let attrs = terminal_style_attrs(style);
    if attrs.is_empty() {
        escaped
    } else {
        format!("<span {}>{escaped}</span>", attrs.join(" "))
    }
}

#[derive(Debug, Clone, Default)]
struct MarkupLine {
    text: String,
    markup: String,
}

fn cell_markup(cell: &Value, text: &str) -> String {
    let escaped = glib::markup_escape_text(text).to_string();
    let Some(style) = cell.get("style") else {
        return escaped;
    };

    let attrs = terminal_style_attrs(style);
    if attrs.is_empty() {
        escaped
    } else {
        format!("<span {}>{escaped}</span>", attrs.join(" "))
    }
}

fn terminal_style_attrs(style: &Value) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut fg = style_rgb_hex(style, "fg");
    let mut bg = style_rgb_hex(style, "bg");
    if style_bool(style, "inverse") {
        std::mem::swap(&mut fg, &mut bg);
        fg.get_or_insert_with(|| "#070809".to_string());
        bg.get_or_insert_with(|| "#d7e2dc".to_string());
    }
    if style_bool(style, "selected") && bg.is_none() {
        bg = Some("#314a55".to_string());
    }
    if style_bool(style, "invisible") {
        fg = Some(bg.clone().unwrap_or_else(|| "#070809".to_string()));
    }
    if let Some(color) = fg {
        attrs.push(format!("foreground=\"{color}\""));
    }
    if let Some(color) = bg {
        attrs.push(format!("background=\"{color}\""));
    }
    if style_bool(style, "bold") {
        attrs.push("weight=\"bold\"".to_string());
    } else if style_bool(style, "faint") {
        attrs.push("weight=\"light\"".to_string());
    }
    if style_bool(style, "italic") {
        attrs.push("style=\"italic\"".to_string());
    }
    if style_bool(style, "underline") {
        attrs.push("underline=\"single\"".to_string());
    }
    if style_bool(style, "overline") {
        attrs.push("overline=\"single\"".to_string());
    }
    if style_bool(style, "strikethrough") {
        attrs.push("strikethrough=\"true\"".to_string());
    }
    attrs
}

fn style_rgb_hex(style: &Value, key: &str) -> Option<String> {
    let rgb = style.get(key)?;
    let r = u8::try_from(rgb.get("r")?.as_u64()?).ok()?;
    let g = u8::try_from(rgb.get("g")?.as_u64()?).ok()?;
    let b = u8::try_from(rgb.get("b")?.as_u64()?).ok()?;
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn style_bool(style: &Value, key: &str) -> bool {
    style.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn pad_to_cell(line: &mut String, target_col: usize) {
    let current_col = line.chars().count();
    if current_col < target_col {
        line.push_str(&" ".repeat(target_col - current_col));
    }
}

fn workspace_id_or_ref(value: &Value) -> Option<String> {
    value
        .get("workspace_id")
        .or_else(|| value.get("workspace_ref"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn workspace_selected(value: &Value) -> bool {
    value
        .get("selected")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn workspace_close_button_visible(value: &Value, workspace_count: usize) -> bool {
    workspace_count > 1 && workspace_selected(value)
}

fn workspace_rename_params(workspace_id: &str, title: &str) -> Option<Value> {
    let title = title.trim();
    if workspace_id.trim().is_empty() || title.is_empty() {
        return None;
    }
    Some(json!({"workspace_id": workspace_id, "title": title}))
}

fn workspace_action_params(workspace_id: &str, action: &str) -> Option<Value> {
    if workspace_id.trim().is_empty() {
        return None;
    }
    Some(json!({"workspace_id": workspace_id, "action": action}))
}

fn workspace_context_action_specs(
    row_model: &GtkWorkspaceSidebarRow,
) -> Vec<(&'static str, &'static str)> {
    let mut actions = Vec::new();
    if row_model.custom_title {
        actions.push((WORKSPACE_CLEAR_NAME_LABEL, "clear-name"));
    }
    if row_model.is_pinned {
        actions.push(("Unpin Workspace", "unpin"));
    } else {
        actions.push(("Pin Workspace", "pin"));
    }
    if row_model.unread {
        actions.push(("Mark Workspace as Read", "mark-read"));
    } else {
        actions.push(("Mark Workspace as Unread", "mark-unread"));
    }
    actions.extend([
        ("Move Workspace Up", "move-up"),
        ("Move Workspace Down", "move-down"),
        ("Move Workspace to Top", "move-top"),
        ("Close Workspaces Above", "close-above"),
        ("Close Workspaces Below", "close-below"),
        ("Close Other Workspaces", "close-others"),
    ]);
    actions
}

fn workspace_new_group_params(workspace_id: &str) -> Option<Value> {
    if workspace_id.trim().is_empty() {
        return None;
    }
    Some(json!({"child_workspace_ids": [workspace_id]}))
}

fn workspace_remove_from_group_params(workspace_id: &str) -> Option<Value> {
    if workspace_id.trim().is_empty() {
        return None;
    }
    Some(json!({"workspace_id": workspace_id}))
}

fn workspace_move_to_group_params(workspace_id: &str, group_id: &str) -> Option<Value> {
    if workspace_id.trim().is_empty() || group_id.trim().is_empty() {
        return None;
    }
    Some(json!({"workspace_id": workspace_id, "group_id": group_id}))
}

fn workspace_group_rename_params(group_id: &str, name: &str) -> Option<Value> {
    let name = name.trim();
    if group_id.trim().is_empty() || name.is_empty() {
        return None;
    }
    Some(json!({"group_id": group_id, "name": name}))
}

fn attach_workspace_context_menu_for(
    button: &gtk::Button,
    app_state: &Arc<Mutex<AppState>>,
    row_model: &GtkWorkspaceSidebarRow,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    parent_context_popover(&popover, button);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 6);
    menu.add_css_class("cmux-context-menu");
    let entry = gtk::Entry::new();
    entry.set_text(&row_model.title);
    entry.set_width_chars(24);
    menu.append(&entry);

    let rename = gtk::Button::with_label("Rename Workspace");
    rename.add_css_class("cmux-context-item");
    {
        let app_state = Arc::clone(app_state);
        let workspace_id = row_model.target.clone();
        let popover = popover.downgrade();
        let entry = entry.downgrade();
        rename.connect_clicked(move |_| {
            let Some(entry) = entry.upgrade() else {
                return;
            };
            if let Some(params) = workspace_rename_params(&workspace_id, &entry.text()) {
                call_app(&app_state, "workspace.rename", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    {
        let app_state = Arc::clone(app_state);
        let workspace_id = row_model.target.clone();
        let popover = popover.downgrade();
        entry.connect_activate(move |entry| {
            if let Some(params) = workspace_rename_params(&workspace_id, &entry.text()) {
                call_app(&app_state, "workspace.rename", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    menu.append(&rename);

    append_context_separator(&menu);
    for (title, action) in workspace_context_action_specs(row_model) {
        if let Some(params) = workspace_action_params(&row_model.target, action) {
            menu.append(&context_menu_rpc_button(
                title,
                app_state,
                &popover,
                "workspace.action",
                params,
            ));
        }
    }

    append_workspace_color_context_controls(
        &menu,
        app_state,
        &popover,
        &row_model.target,
        row_model.tint_hex.as_deref(),
    );

    append_context_separator(&menu);
    if row_model.indented {
        if let Some(params) = workspace_remove_from_group_params(&row_model.target) {
            menu.append(&context_menu_rpc_button(
                WORKSPACE_REMOVE_FROM_GROUP_LABEL,
                app_state,
                &popover,
                "workspace.group.remove",
                params,
            ));
        }
    } else if let Some(params) = workspace_new_group_params(&row_model.target) {
        menu.append(&context_menu_rpc_button(
            WORKSPACE_NEW_GROUP_LABEL,
            app_state,
            &popover,
            "workspace.group.create",
            params,
        ));
    }
    let move_targets = row_model
        .available_group_targets
        .iter()
        .filter(|group| row_model.group_target.as_deref() != Some(group.target.as_str()))
        .collect::<Vec<_>>();
    if !move_targets.is_empty() {
        append_context_separator(&menu);
        for group in move_targets {
            if let Some(params) = workspace_move_to_group_params(&row_model.target, &group.target) {
                menu.append(&context_menu_rpc_button(
                    &format!("{WORKSPACE_MOVE_TO_GROUP_PREFIX} {}", group.title),
                    app_state,
                    &popover,
                    "workspace.group.add",
                    params,
                ));
            }
        }
    }
    popover.set_child(Some(&menu));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let popover = popover.downgrade();
    let entry = entry.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        let (Some(popover), Some(entry)) = (popover.upgrade(), entry.upgrade()) else {
            return;
        };
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
    button.add_controller(gesture);
}

fn append_workspace_color_context_controls(
    menu: &gtk::Box,
    app_state: &Arc<Mutex<AppState>>,
    popover: &gtk::Popover,
    workspace_id: &str,
    current_color: Option<&str>,
) {
    let settings = config::workspace_color_settings();
    if settings.colors.is_empty() {
        return;
    }
    append_context_separator(menu);
    menu.append(&label("Workspace Color", "cmux-muted"));
    let colors = gtk::FlowBox::new();
    colors.set_selection_mode(gtk::SelectionMode::None);
    colors.set_max_children_per_line(8);
    colors.set_row_spacing(4);
    colors.set_column_spacing(4);
    for (name, color) in settings.colors {
        let swatch = gtk::Button::new();
        swatch.set_size_request(24, 24);
        swatch.set_tooltip_text(Some(&format!("{name}: {color}")));
        swatch.add_css_class("cmux-color-swatch");
        let outline = if current_color.is_some_and(|current| current.eq_ignore_ascii_case(&color)) {
            "outline: 2px solid #f4f0e8; outline-offset: -2px;"
        } else {
            ""
        };
        install_custom_sidebar_style(
            swatch.upcast_ref(),
            &format!("background: {color}; {outline}"),
        );
        let color_state = Arc::clone(app_state);
        let color_workspace = workspace_id.to_string();
        let color_popover = popover.downgrade();
        swatch.connect_clicked(move |_| {
            call_app(
                &color_state,
                "workspace.action",
                json!({
                    "workspace_id": color_workspace,
                    "action": "set-color",
                    "color": color
                }),
            );
            if let Some(popover) = color_popover.upgrade() {
                popover.popdown();
            }
        });
        colors.insert(&swatch, -1);
    }
    menu.append(&colors);
    let clear = gtk::Button::with_label("Clear Color");
    clear.add_css_class("cmux-context-item");
    clear.set_sensitive(current_color.is_some());
    let clear_state = Arc::clone(app_state);
    let clear_workspace = workspace_id.to_string();
    let clear_popover = popover.downgrade();
    clear.connect_clicked(move |_| {
        call_app(
            &clear_state,
            "workspace.action",
            json!({"workspace_id": clear_workspace, "action": "clear-color"}),
        );
        if let Some(popover) = clear_popover.upgrade() {
            popover.popdown();
        }
    });
    menu.append(&clear);
}

fn attach_workspace_group_context_menu_for(
    button: &gtk::Button,
    app_state: &Arc<Mutex<AppState>>,
    row_model: &GtkWorkspaceSidebarRow,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    parent_context_popover(&popover, button);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 6);
    menu.add_css_class("cmux-context-menu");
    let entry = gtk::Entry::new();
    entry.set_text(&row_model.title);
    entry.set_width_chars(24);
    menu.append(&entry);

    let rename = gtk::Button::with_label("Rename Group");
    rename.add_css_class("cmux-context-item");
    {
        let app_state = Arc::clone(app_state);
        let group_id = row_model.target.clone();
        let popover = popover.downgrade();
        let entry = entry.downgrade();
        rename.connect_clicked(move |_| {
            let Some(entry) = entry.upgrade() else {
                return;
            };
            if let Some(params) = workspace_group_rename_params(&group_id, &entry.text()) {
                call_app(&app_state, "workspace.group.rename", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    {
        let app_state = Arc::clone(app_state);
        let group_id = row_model.target.clone();
        let popover = popover.downgrade();
        entry.connect_activate(move |entry| {
            if let Some(params) = workspace_group_rename_params(&group_id, &entry.text()) {
                call_app(&app_state, "workspace.group.rename", params);
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            }
        });
    }
    menu.append(&rename);

    let collapse_method = if row_model.collapsed {
        "workspace.group.expand"
    } else {
        "workspace.group.collapse"
    };
    let collapse_title = if row_model.collapsed {
        "Expand Group"
    } else {
        "Collapse Group"
    };
    menu.append(&context_menu_rpc_button(
        collapse_title,
        app_state,
        &popover,
        collapse_method,
        json!({"group_id": row_model.target.as_str()}),
    ));

    let pin_method = if row_model.is_pinned {
        "workspace.group.unpin"
    } else {
        "workspace.group.pin"
    };
    let pin_title = if row_model.is_pinned {
        "Unpin Group"
    } else {
        "Pin Group"
    };
    menu.append(&context_menu_rpc_button(
        pin_title,
        app_state,
        &popover,
        pin_method,
        json!({"group_id": row_model.target.as_str()}),
    ));

    append_context_separator(&menu);
    menu.append(&context_menu_rpc_button(
        GROUP_NEW_WORKSPACE_LABEL,
        app_state,
        &popover,
        "workspace.group.new_workspace",
        json!({"group_id": row_model.target.as_str()}),
    ));

    append_workspace_group_configured_context_menu_items(&menu, app_state, &popover, row_model);

    append_context_separator(&menu);
    menu.append(&context_menu_rpc_button(
        GROUP_EDIT_CONFIG_LABEL,
        app_state,
        &popover,
        "settings.open",
        json!({"target": "settingsJSON"}),
    ));
    menu.append(&context_menu_rpc_button(
        GROUP_DOCS_LABEL,
        app_state,
        &popover,
        "browser.open_split",
        json!({"url": "https://cmux.com/docs/workspace-groups", "focus": true}),
    ));

    append_context_separator(&menu);
    menu.append(&context_menu_rpc_button(
        "Ungroup (Keep Workspaces)",
        app_state,
        &popover,
        "workspace.group.ungroup",
        json!({"group_id": row_model.target.as_str()}),
    ));
    append_workspace_group_delete_controls(&menu, app_state, &popover, row_model);

    popover.set_child(Some(&menu));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let popover = popover.downgrade();
    let entry = entry.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        let (Some(popover), Some(entry)) = (popover.upgrade(), entry.upgrade()) else {
            return;
        };
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
    button.add_controller(gesture);
}

fn append_workspace_group_delete_controls(
    menu: &gtk::Box,
    app_state: &Arc<Mutex<AppState>>,
    popover: &gtk::Popover,
    row_model: &GtkWorkspaceSidebarRow,
) {
    let confirm = gtk::CheckButton::with_label(GROUP_DELETE_CONFIRM_LABEL);
    confirm.add_css_class("cmux-context-item");
    confirm.set_focusable(false);
    menu.append(&confirm);

    let delete = context_menu_rpc_button(
        GROUP_DELETE_LABEL,
        app_state,
        popover,
        "workspace.group.delete",
        json!({"group_id": row_model.target.as_str()}),
    );
    delete.set_sensitive(false);
    {
        let delete = delete.clone();
        confirm.connect_toggled(move |confirm| {
            delete.set_sensitive(confirm.is_active());
        });
    }
    menu.append(&delete);
}

fn attach_workspace_group_add_context_menu_for(
    button: &gtk::Button,
    app_state: &Arc<Mutex<AppState>>,
    row_model: &GtkWorkspaceSidebarRow,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    parent_context_popover(&popover, button);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 6);
    menu.add_css_class("cmux-context-menu");
    menu.append(&context_menu_rpc_button(
        GROUP_NEW_WORKSPACE_LABEL,
        app_state,
        &popover,
        "workspace.group.new_workspace",
        json!({"group_id": row_model.target.as_str()}),
    ));
    append_workspace_group_configured_context_menu_items(&menu, app_state, &popover, row_model);

    append_context_separator(&menu);
    menu.append(&context_menu_rpc_button(
        GROUP_EDIT_CONFIG_LABEL,
        app_state,
        &popover,
        "settings.open",
        json!({"target": "settingsJSON"}),
    ));
    menu.append(&context_menu_rpc_button(
        GROUP_DOCS_LABEL,
        app_state,
        &popover,
        "browser.open_split",
        json!({"url": "https://cmux.com/docs/workspace-groups", "focus": true}),
    ));

    popover.set_child(Some(&menu));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let popover = popover.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        let Some(popover) = popover.upgrade() else {
            return;
        };
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    });
    button.add_controller(gesture);
}

fn append_workspace_group_configured_context_menu_items(
    menu: &gtk::Box,
    app_state: &Arc<Mutex<AppState>>,
    popover: &gtk::Popover,
    row_model: &GtkWorkspaceSidebarRow,
) {
    if row_model.configured_context_menu_entries.is_empty() {
        return;
    }

    append_context_separator(menu);
    for item in &row_model.configured_context_menu_entries {
        match item {
            GtkWorkspaceGroupConfiguredMenuEntry::Separator => {
                append_context_separator(menu);
            }
            GtkWorkspaceGroupConfiguredMenuEntry::Action(action) => {
                if let Some((method, params)) =
                    workspace_group_configured_action_request(row_model, action)
                {
                    let item =
                        context_menu_rpc_button(&action.title, app_state, popover, method, params);
                    if let Some(tooltip) = action.tooltip.as_deref() {
                        item.set_tooltip_text(Some(tooltip));
                    }
                    menu.append(&item);
                } else {
                    let item = disabled_context_menu_button(
                        &action.title,
                        "Configured group action is not executable in GTK yet",
                    );
                    menu.append(&item);
                }
            }
        }
    }
}

fn context_menu_rpc_button(
    title: &str,
    app_state: &Arc<Mutex<AppState>>,
    popover: &gtk::Popover,
    method: &'static str,
    params: Value,
) -> gtk::Button {
    let button = gtk::Button::with_label(title);
    button.add_css_class("cmux-context-item");
    button.set_focusable(false);
    let app_state = Arc::clone(app_state);
    let popover = popover.downgrade();
    button.connect_clicked(move |_| {
        if call_app(&app_state, method, params.clone()) {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        }
    });
    button
}

fn disabled_context_menu_button(title: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(title);
    button.add_css_class("cmux-context-item");
    button.set_focusable(false);
    button.set_sensitive(false);
    button.set_tooltip_text(Some(tooltip));
    button
}

fn append_context_separator(menu: &gtk::Box) {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    menu.append(&separator);
}

fn surface_id_or_ref(value: &Value) -> Option<String> {
    value
        .get("surface_id")
        .or_else(|| value.get("id"))
        .or_else(|| value.get("surface_ref"))
        .or_else(|| value.get("ref"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn pane_id_or_ref(value: &Value) -> Option<String> {
    value
        .get("pane_id")
        .or_else(|| value.get("pane_ref"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn status_text(status: &Value) -> String {
    let key = value_str(status, "key", "status");
    let value = value_str(status, "value", "");
    let icon = value_str(status, "icon", "");
    if icon.is_empty() {
        format!("{key}: {value}")
    } else {
        format!("{icon} {key}: {value}")
    }
}

fn notification_detail(notification: &Value) -> String {
    [
        value_str(notification, "subtitle", ""),
        value_str(notification, "body", ""),
        value_str(notification, "surface_ref", ""),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" - ")
}

fn notification_focus_params(notification: &Value) -> Value {
    let mut params = serde_json::Map::new();
    if let Some(workspace) = workspace_id_or_ref(notification) {
        params.insert("workspace_id".to_string(), json!(workspace));
    }
    if let Some(surface) = surface_id_or_ref(notification) {
        params.insert("surface_id".to_string(), json!(surface));
    }
    Value::Object(params)
}

fn notification_mark_params(notification: &Value) -> Option<Value> {
    let id = notification
        .get("id")
        .or_else(|| notification.get("notification_id"))
        .and_then(Value::as_str)?;
    Some(json!({"notification_id": id}))
}

fn attach_notification_context_menu(
    button: &gtk::Button,
    app_state: &Arc<Mutex<AppState>>,
    notification: &Value,
) {
    let Some(params) = notification_mark_params(notification) else {
        return;
    };

    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    parent_context_popover(&popover, button);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 4);
    menu.add_css_class("cmux-context-menu");
    for (title, method) in [
        ("Mark as Read", "notification.mark_read"),
        ("Mark as Unread", "notification.mark_unread"),
    ] {
        let item = gtk::Button::with_label(title);
        item.add_css_class("cmux-context-item");
        let app_state = Arc::clone(app_state);
        let params = params.clone();
        let popover = popover.downgrade();
        item.connect_clicked(move |_| {
            call_app(&app_state, method, params.clone());
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
        menu.append(&item);
    }
    popover.set_child(Some(&menu));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let popover = popover.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        let Some(popover) = popover.upgrade() else {
            return;
        };
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    });
    button.add_controller(gesture);
}

fn trim_preview(text: &str) -> String {
    let mut lines = text
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(12)
        .collect::<Vec<_>>();
    lines.reverse();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_custom_sidebar_helpers_map_style_and_icons() {
        assert_eq!(custom_sidebar_css_color("#aabbcc"), Some("#aabbcc"));
        assert_eq!(custom_sidebar_css_color("accent"), Some("#4aa3c7"));
        assert_eq!(custom_sidebar_css_color("unknown"), None);
        assert_eq!(custom_sidebar_css_weight("semibold"), Some("600"));
        assert_eq!(
            custom_sidebar_font_size(&json!({"font": "headline"})),
            Some(17.0)
        );
        assert_eq!(custom_sidebar_icon_name("folder.fill"), "folder-symbolic");
        assert_eq!(
            custom_sidebar_icon_name("not-a-symbol"),
            "image-missing-symbolic"
        );
    }

    #[test]
    fn gtk_snapshot_rebuild_key_tracks_custom_sidebar_document_and_selection() {
        let base = json!({
            "custom_sidebar": {
                "selected_provider_id": "cmux.sidebar.custom.status",
                "state": "ready",
                "document": {"version": 1, "root": {"type": "text", "text": "One"}}
            }
        });
        let mut changed_document = base.clone();
        changed_document["custom_sidebar"]["document"]["root"]["text"] = json!("Two");
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&changed_document)
        );

        let mut changed_provider = base.clone();
        changed_provider["custom_sidebar"]["selected_provider_id"] =
            json!("cmux.sidebar.workspaces");
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&changed_provider)
        );
    }

    #[test]
    fn gtk_snapshot_region_keys_isolate_independent_updates() {
        let base = json!({
            "workspaces": [{
                "workspace_id": "w1",
                "workspace_ref": "workspace:1",
                "title": "Workspace",
                "selected": true,
                "pinned": false
            }],
            "workspace_groups": [],
            "custom_sidebar": {"selected_provider_id": "cmux.sidebar.workspaces"},
            "surface_views": [{
                "surface_id": "s1",
                "pane_id": "p1",
                "workspace_id": "w1",
                "kind": "browser",
                "visible": true,
                "browser": {"url": "https://one.test"},
                "tabs": [{"surface_id": "s1", "selected": true}]
            }],
            "surfaces": [{
                "id": "s1",
                "surface_ref": "surface:1",
                "type": "browser",
                "title": "Browser"
            }],
            "sidebar": {
                "cwd": "/tmp/project",
                "progress": "none",
                "statuses": [],
                "logs": []
            },
            "right_sidebar": {
                "visible": true,
                "mode": "files",
                "feed_items": [{"id": "feed-1"}]
            },
            "notifications": [{"id": "notification-1"}],
            "canvas": {"mode": "splits", "panes": []},
            "config": {"reload_generation": 1, "app": {}}
        });
        let keys = snapshot_region_rebuild_keys(&base);

        let mut inactive_feed_changed = base.clone();
        inactive_feed_changed["right_sidebar"]["feed_items"] =
            json!([{"id": "feed-1"}, {"id": "feed-2"}]);
        inactive_feed_changed["notifications"] =
            json!([{"id": "notification-1"}, {"id": "notification-2"}]);
        assert_eq!(
            keys,
            snapshot_region_rebuild_keys(&inactive_feed_changed),
            "inactive feed data must not rebuild any GTK region"
        );

        let mut feed_base = base.clone();
        feed_base["right_sidebar"]["mode"] = json!("feed");
        let feed_keys = snapshot_region_rebuild_keys(&feed_base);
        let mut feed_changed = feed_base.clone();
        feed_changed["right_sidebar"]["feed_items"] = json!([{"id": "feed-1"}, {"id": "feed-2"}]);
        let changed_feed_keys = snapshot_region_rebuild_keys(&feed_changed);
        assert_eq!(feed_keys.left, changed_feed_keys.left);
        assert_eq!(feed_keys.main, changed_feed_keys.main);
        assert_ne!(feed_keys.right, changed_feed_keys.right);

        let mut workspace_changed = base.clone();
        workspace_changed["workspaces"][0]["selected"] = json!(false);
        let changed_workspace_keys = snapshot_region_rebuild_keys(&workspace_changed);
        assert_ne!(keys.left, changed_workspace_keys.left);
        assert_ne!(keys.main, changed_workspace_keys.main);
        assert_eq!(keys.right, changed_workspace_keys.right);

        let mut browser_changed = base.clone();
        browser_changed["surface_views"][0]["browser"]["url"] = json!("https://two.test");
        let changed_browser_keys = snapshot_region_rebuild_keys(&browser_changed);
        assert_eq!(keys.left, changed_browser_keys.left);
        assert_ne!(keys.main, changed_browser_keys.main);
        assert_eq!(keys.right, changed_browser_keys.right);
    }

    #[test]
    fn gtk_snapshot_region_main_key_retains_tab_only_fast_path() {
        let base = json!({
            "surface_views": [{
                "surface_id": "s1",
                "pane_id": "p1",
                "kind": "terminal",
                "visible": true,
                "tabs": [{"surface_id": "s1", "selected": true}]
            }],
            "canvas": {
                "mode": "splits",
                "panes": [{
                    "pane_id": "p1",
                    "surface_ids": ["s1"],
                    "surface_refs": ["surface:1"],
                    "width": 800.0
                }]
            }
        });
        let keys = snapshot_region_rebuild_keys(&base);
        let mut tabs_changed = base.clone();
        tabs_changed["surface_views"][0]["tabs"] = json!([
            {"surface_id": "s1", "selected": true},
            {"surface_id": "s2", "selected": false}
        ]);
        tabs_changed["canvas"]["panes"][0]["surface_ids"] = json!(["s1", "s2"]);
        tabs_changed["canvas"]["panes"][0]["surface_refs"] = json!(["surface:1", "surface:2"]);
        let changed = snapshot_region_rebuild_keys(&tabs_changed);

        assert_ne!(keys.main, changed.main);
        assert_eq!(keys.main_without_tabs, changed.main_without_tabs);
        assert_eq!(keys.left, changed.left);
        assert_eq!(keys.right, changed.right);
    }

    #[test]
    fn gtk_next_overlay_changes_do_not_rebuild_native_surface_tree() {
        let base = json!({
            "surface_views": [{
                "surface_id": "s1",
                "pane_id": "p1",
                "kind": "terminal",
                "visible": true,
                "tabs": [{"surface_id": "s1", "selected": true}]
            }],
            "command_palette": {"visible": false},
            "shortcut_help": {"visible": false}
        });
        let keys = snapshot_region_rebuild_keys_for_mode(&base, GtkUiMode::Next);
        let mut changed = base.clone();
        changed["command_palette"] = json!({
            "visible": true,
            "query": "split",
            "selected_index": 1
        });
        let changed_keys = snapshot_region_rebuild_keys_for_mode(&changed, GtkUiMode::Next);
        assert_eq!(keys.main, changed_keys.main);
        assert_eq!(keys.main_without_tabs, changed_keys.main_without_tabs);
        assert_ne!(
            shell::overlay_rebuild_key(&base),
            shell::overlay_rebuild_key(&changed)
        );
    }

    #[test]
    fn gtk_custom_sidebar_cmux_action_dispatches_to_app_methods() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let first = call_app_value(&app_state, "workspace.current", json!({}))
            .expect("current workspace")["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(call_app(
            &app_state,
            "workspace.create",
            json!({"title": "Second", "focus": false})
        ));
        dispatch_custom_sidebar_action(
            &app_state,
            "cmux.sidebar.custom.status",
            &json!({"type": "workspace.next"}),
        );
        let selected = call_app_value(&app_state, "workspace.current", json!({}))
            .expect("selected workspace")["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(selected, first);
    }

    #[test]
    fn gtk_custom_sidebar_reorder_builds_parameterized_move_request() {
        let payload = custom_sidebar_reorder_payload(&json!({
            "reorder": {
                "method": "workspace.reorder",
                "idParameter": "workspace_id",
                "itemId": "workspace-a",
                "index": 0
            }
        }))
        .expect("reorder payload");
        let (method, params) = custom_sidebar_reorder_request(&payload, "workspace.reorder", 3)
            .expect("reorder request");
        assert_eq!(method, "workspace.reorder");
        assert_eq!(params["workspace_id"], "workspace-a");
        assert_eq!(params["index"], 3);
    }

    #[test]
    fn gtk_custom_sidebar_numeric_controls_preserve_state_types_and_precision() {
        assert_eq!(custom_sidebar_control_digits(1.0), 0);
        assert_eq!(custom_sidebar_control_digits(0.5), 1);
        assert_eq!(custom_sidebar_control_digits(0.125), 3);
        assert_eq!(custom_sidebar_control_digits(0.000_001), 6);
        assert_eq!(custom_sidebar_numeric_state_value(3.6, true), json!(4));
        assert_eq!(custom_sidebar_numeric_state_value(0.75, false), json!(0.75));
    }

    #[test]
    fn gtk_custom_sidebar_submit_events_inherit_and_keep_binding_context() {
        let inherited = vec![json!({"id": "submit:parent"})];
        let events = custom_sidebar_submit_events(
            &json!({"onSubmit": [{"id": "submit:field"}]}),
            &inherited,
        );
        assert_eq!(
            events,
            vec![
                json!({"id": "submit:parent"}),
                json!({"id": "submit:field"})
            ]
        );
        assert_eq!(
            custom_sidebar_submit_params(
                "cmux.sidebar.custom.form",
                "submit:field",
                "name",
                "Linux",
            ),
            json!({
                "provider_id": "cmux.sidebar.custom.form",
                "event_id": "submit:field",
                "key": "name",
                "value": "Linux"
            })
        );
    }

    #[test]
    fn gtk_snapshot_rebuild_key_ignores_volatile_terminal_state() {
        let base = json!({
            "workspaces": [{"workspace_id": "w1", "selected": true}],
            "surface_views": [{
                "surface_id": "s1",
                "pane_id": "p1",
                "kind": "terminal",
                "visible": true,
                "focused": true,
                "title": "shell",
                "preview": "one"
            }]
        });
        let mut changed = base.clone();
        changed["surface_views"][0]["focused"] = json!(false);
        changed["surface_views"][0]["title"] = json!("build");
        changed["surface_views"][0]["preview"] = json!("two");

        assert_eq!(snapshot_rebuild_key(&base), snapshot_rebuild_key(&changed));
    }

    #[test]
    fn gtk_snapshot_rebuild_key_tracks_text_box_structure_not_live_text() {
        let base = json!({
            "surface_views": [{
                "surface_id": "s1",
                "kind": "terminal",
                "text_box": {
                    "active": true,
                    "focus": "textBox",
                    "text": "one",
                    "attachments": [],
                    "focus_generation": 1,
                    "file_picker_generation": 0,
                    "max_lines": 10
                }
            }]
        });
        let mut live_text = base.clone();
        live_text["surface_views"][0]["text_box"]["text"] = json!("two");
        assert_eq!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&live_text)
        );

        let mut attachment = base.clone();
        attachment["surface_views"][0]["text_box"]["attachments"] =
            json!([{"id": "a1", "displayName": "notes.txt"}]);
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&attachment)
        );
        let mut hidden = base.clone();
        hidden["surface_views"][0]["text_box"]["active"] = json!(false);
        assert_ne!(snapshot_rebuild_key(&base), snapshot_rebuild_key(&hidden));
    }

    #[test]
    fn gtk_snapshot_rebuild_key_tracks_layout_and_browser_content() {
        let base = json!({
            "workspaces": [{"workspace_id": "w1", "selected": true}],
            "surface_views": [{
                "surface_id": "s1",
                "pane_id": "p1",
                "kind": "browser",
                "visible": true,
                "browser": {"url": "https://one.test"}
            }]
        });
        let mut browser_changed = base.clone();
        browser_changed["surface_views"][0]["browser"]["url"] = json!("https://two.test");
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&browser_changed)
        );

        let mut settings_changed = base.clone();
        settings_changed["surface_views"][0]["kind"] = json!("settings");
        settings_changed["surface_views"][0]["settings"] = json!({"target": "terminal"});
        let settings_key = snapshot_rebuild_key(&settings_changed);
        settings_changed["surface_views"][0]["settings"]["target"] = json!("browser");
        assert_ne!(settings_key, snapshot_rebuild_key(&settings_changed));

        let mut document_changed = base.clone();
        document_changed["surface_views"][0]["kind"] = json!("markdown");
        document_changed["surface_views"][0]["document"] =
            json!({"kind": "markdown", "content": "one"});
        let document_key = snapshot_rebuild_key(&document_changed);
        document_changed["surface_views"][0]["document"]["content"] = json!("two");
        assert_ne!(document_key, snapshot_rebuild_key(&document_changed));

        let mut agent_changed = base.clone();
        agent_changed["surface_views"][0]["kind"] = json!("agent-session");
        agent_changed["surface_views"][0]["agent_session"] =
            json!({"provider_id": "codex", "status": "idle"});
        let agent_key = snapshot_rebuild_key(&agent_changed);
        agent_changed["surface_views"][0]["agent_session"]["draft_text"] =
            json!("typed but unsent");
        assert_eq!(agent_key, snapshot_rebuild_key(&agent_changed));
        agent_changed["surface_views"][0]["agent_session"]["pending_attachments"] =
            json!([{"id": "a1", "label": "notes.txt", "path": "/tmp/notes.txt"}]);
        assert_ne!(agent_key, snapshot_rebuild_key(&agent_changed));
        agent_changed["surface_views"][0]["agent_session"]["pending_attachments"] = json!([]);
        agent_changed["surface_views"][0]["agent_session"]["status"] = json!("running");
        assert_ne!(agent_key, snapshot_rebuild_key(&agent_changed));

        let mut hibernated = base.clone();
        hibernated["surface_views"][0]["hibernated"] = json!(true);
        hibernated["surface_views"][0]["agent_hibernation"] = json!({
            "hibernated_at_ms": 10_000,
            "last_activity_ms": 5_000
        });
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&hibernated)
        );

        let mut layout_changed = base.clone();
        layout_changed["surface_views"][0]["pane_id"] = json!("p2");
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&layout_changed)
        );

        let mut tabs_changed = base.clone();
        tabs_changed["surface_views"][0]["tabs"] = json!([{
            "surface_id": "s1",
            "title": "Browser",
            "kind": "browser",
            "selected": true
        }, {
            "surface_id": "s2",
            "title": "Shell",
            "kind": "terminal",
            "selected": false
        }]);
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&tabs_changed)
        );
        assert_eq!(
            snapshot_rebuild_key_without_tabs(&base),
            snapshot_rebuild_key_without_tabs(&tabs_changed)
        );

        let mut canvas_tabs_changed = base.clone();
        canvas_tabs_changed["canvas"] = json!({
            "mode": "splits",
            "panes": [{
                "pane_ref": "pane:1",
                "surface_ref": "surface:1",
                "selected_surface_ref": "surface:1",
                "surface_ids": ["surface-id-1", "surface-id-2"],
                "surface_refs": ["surface:1", "surface:2"],
                "x": 0.0,
                "y": 0.0,
                "width": 800.0,
                "height": 600.0
            }]
        });
        let mut canvas_tabs_base = canvas_tabs_changed.clone();
        canvas_tabs_base["canvas"]["panes"][0]["surface_ids"] = json!(["surface-id-1"]);
        canvas_tabs_base["canvas"]["panes"][0]["surface_refs"] = json!(["surface:1"]);
        assert_eq!(
            snapshot_rebuild_key_without_tabs(&canvas_tabs_base),
            snapshot_rebuild_key_without_tabs(&canvas_tabs_changed)
        );
        canvas_tabs_changed["canvas"]["panes"][0]["width"] = json!(900.0);
        assert_ne!(
            snapshot_rebuild_key_without_tabs(&canvas_tabs_base),
            snapshot_rebuild_key_without_tabs(&canvas_tabs_changed)
        );

        let mut sidebar_changed = base.clone();
        sidebar_changed["right_sidebar"] = json!({"visible": false, "mode": "find"});
        assert_ne!(
            snapshot_rebuild_key(&base),
            snapshot_rebuild_key(&sidebar_changed)
        );
    }

    fn gtk_tab_test_snapshot(selected_surface: &str, preview: &str) -> Value {
        let tabs = ["surface-a", "surface-b"]
            .into_iter()
            .map(|surface_id| {
                json!({
                    "surface_id": surface_id,
                    "title": if surface_id == "surface-a" { "Alpha" } else { "Beta" },
                    "kind": "terminal",
                    "selected": surface_id == selected_surface,
                    "pinned": false,
                    "unread": false
                })
            })
            .collect::<Vec<_>>();
        json!({
            "focused": {
                "workspace_id": "workspace-a",
                "pane_id": "pane-a",
                "surface_id": selected_surface
            },
            "workspaces": [{
                "workspace_id": "workspace-a",
                "workspace_ref": "workspace:1",
                "title": "Workspace",
                "selected": true,
                "pinned": false
            }],
            "surface_views": [{
                "surface_id": selected_surface,
                "surface_ref": selected_surface,
                "workspace_id": "workspace-a",
                "pane_id": "pane-a",
                "kind": "terminal",
                "title": if selected_surface == "surface-a" { "Alpha" } else { "Beta" },
                "visible": true,
                "focused": true,
                "frame": {"x": 0.0, "y": 0.0, "width": 800.0, "height": 600.0},
                "tabs": tabs,
                "preview": preview
            }],
            "window_surfaces": [{
                "surface_id": "surface-a",
                "type": "terminal"
            }, {
                "surface_id": "surface-b",
                "type": "terminal"
            }],
            "canvas": {"mode": "splits", "panes": []},
            "right_sidebar": {"visible": false, "mode": "files"},
            "config": {"reload_generation": 0, "app": {}}
        })
    }

    fn gtk_tab_button_with_tooltip(root: &gtk::Widget, tooltip: &str) -> Option<gtk::Button> {
        if let Ok(button) = root.clone().downcast::<gtk::Button>() {
            if button.tooltip_text().as_deref() == Some(tooltip) {
                return Some(button);
            }
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(button) = gtk_tab_button_with_tooltip(&widget, tooltip) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn assert_gtk_pane_tab_reconciliation_preserves_widgets_and_scroll_position() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let first = gtk_tab_test_snapshot("surface-a", "alpha");
        let first_view = &first["surface_views"][0];
        let strip = pane_tab_strip(first_view, &app_state, None).expect("pane tab strip");
        let strip_widget = strip.clone().upcast::<gtk::Widget>();
        let scroller = widget_descendant_with_css_class(&strip_widget, "cmux-pane-tab-scroll")
            .expect("tab scroller")
            .downcast::<gtk::ScrolledWindow>()
            .expect("tab scroller type");
        let alpha = gtk_tab_button_with_tooltip(&strip_widget, "Alpha").expect("alpha tab");
        let adjustment = scroller.hadjustment();
        adjustment.configure(24.0, 0.0, 200.0, 1.0, 10.0, 50.0);

        let second = gtk_tab_test_snapshot("surface-b", "beta");
        populate_pane_tab_strip(&strip, &second["surface_views"][0], &app_state, None);

        let current_scroller =
            widget_descendant_with_css_class(&strip_widget, "cmux-pane-tab-scroll")
                .expect("current tab scroller")
                .downcast::<gtk::ScrolledWindow>()
                .expect("current tab scroller type");
        let current_alpha =
            gtk_tab_button_with_tooltip(&strip_widget, "Alpha").expect("current alpha tab");
        assert_eq!(
            scroller, current_scroller,
            "tab scroller must remain mounted"
        );
        assert_eq!(
            alpha, current_alpha,
            "unchanged tabs must retain their widgets"
        );
        assert_eq!(current_scroller.hadjustment(), adjustment);
        assert!((current_scroller.hadjustment().value() - 24.0).abs() < f64::EPSILON);
    }

    fn gtk_count_widgets_with_css_class(root: &gtk::Widget, class: &str) -> usize {
        let mut count = usize::from(root.has_css_class(class));
        let mut child = root.first_child();
        while let Some(widget) = child {
            count += gtk_count_widgets_with_css_class(&widget, class);
            child = widget.next_sibling();
        }
        count
    }

    fn gtk_run_main_loop_for(duration: Duration) {
        let main_loop = glib::MainLoop::new(None, false);
        let quit_loop = main_loop.clone();
        glib::timeout_add_local_once(duration, move || quit_loop.quit());
        main_loop.run();
    }

    fn gtk_test_local_refresh(
        application: &gtk::Application,
        app_state: &Arc<Mutex<AppState>>,
    ) -> GtkLocalRefresh {
        GtkLocalRefresh::new(
            application,
            app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &Rc::new(RefCell::new(HashMap::new())),
            &Rc::new(RefCell::new(None)),
            &Rc::new(RefCell::new(None)),
            &Rc::new(RefCell::new(GtkGlobalVisibilityState::default())),
        )
    }

    fn assert_gtk_tab_create_focus_and_close_refresh_before_fallback_poll() {
        let application = gtk::Application::builder()
            .application_id("ai.manaflow.cmux.tests.tab-refresh")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("register app");
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let hosts = Rc::new(RefCell::new(HashMap::new()));
        let desktop_notifications = Rc::new(RefCell::new(None));
        let presented_model_window = Rc::new(RefCell::new(None));
        let global_visibility = Rc::new(RefCell::new(GtkGlobalVisibilityState::default()));
        let local_refresh = GtkLocalRefresh::new(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &hosts,
            &desktop_notifications,
            &presented_model_window,
            &global_visibility,
        );
        assert!(sync_gtk_window_hosts(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &hosts,
            &desktop_notifications,
            &presented_model_window,
            &global_visibility,
            &local_refresh,
        ));
        let window = hosts
            .borrow()
            .values()
            .next()
            .expect("GTK window host")
            .window
            .clone();
        let root = window.child().expect("GTK window content");
        let first_tab = widget_descendant_with_css_class(&root, "cmux-pane-tab")
            .expect("initial pane tab")
            .downcast::<gtk::Button>()
            .expect("initial tab button");
        let add = gtk_tab_button_with_tooltip(&root, "New Terminal Tab").expect("add tab button");
        add.emit_clicked();
        gtk_run_main_loop_for(Duration::from_millis(100));
        assert_eq!(
            gtk_count_widgets_with_css_class(&root, "cmux-pane-tab"),
            2,
            "GTK-created tabs must reconcile before the 500ms fallback poll"
        );

        first_tab.emit_clicked();
        gtk_run_main_loop_for(Duration::from_millis(100));
        assert!(
            first_tab.has_css_class("cmux-pane-tab-selected"),
            "GTK-focused tabs must reconcile before the fallback poll"
        );

        let close = gtk_tab_button_with_tooltip(&root, "Close Tab").expect("close tab button");
        close.emit_clicked();
        gtk_run_main_loop_for(Duration::from_millis(100));
        assert_eq!(
            gtk_count_widgets_with_css_class(&root, "cmux-pane-tab"),
            1,
            "GTK-closed tabs must reconcile before the fallback poll"
        );
        for host in hosts.borrow_mut().values_mut() {
            host.window.destroy();
        }
    }

    fn assert_gtk_selected_tab_changes_keep_main_tree_mounted_and_content_nonblank() {
        let application = gtk::Application::builder()
            .application_id("ai.manaflow.cmux.tests.tab-responsiveness")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("register app");
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let row = json!({
            "window_id": "window-a",
            "title": "Tab responsiveness",
            "selected": true,
            "fullscreen": false
        });
        let first = gtk_tab_test_snapshot("surface-a", "alpha content");
        let local_refresh = gtk_test_local_refresh(&application, &app_state);
        let mut host = create_gtk_window_host(
            &application,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            "window-a",
            &row,
            &first,
            &local_refresh,
        );
        let mounted_main = host
            .snapshot_view
            .main_slot
            .first_child()
            .expect("mounted main tree");

        let second = gtk_tab_test_snapshot("surface-b", "beta content");
        refresh_gtk_window_host(
            &mut host,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &row,
            &second,
            &local_refresh,
        );
        assert_eq!(
            host.snapshot_view.main_slot.first_child().as_ref(),
            Some(&mounted_main),
            "selecting a tab must not replace the whole main tree"
        );
        let preview =
            widget_descendant_with_css_class(&host.snapshot_view.root, "cmux-terminal-preview")
                .expect("selected tab content")
                .downcast::<gtk::Label>()
                .expect("GTK preview label");
        assert_eq!(preview.text(), "beta content");

        refresh_gtk_window_host(
            &mut host,
            &app_state,
            GtkRendererMode::Gtk,
            GtkUiMode::Next,
            &row,
            &first,
            &local_refresh,
        );
        assert_eq!(
            host.snapshot_view.main_slot.first_child().as_ref(),
            Some(&mounted_main),
            "repeated tab changes must keep the main tree mounted"
        );
        let preview =
            widget_descendant_with_css_class(&host.snapshot_view.root, "cmux-terminal-preview")
                .expect("restored tab content")
                .downcast::<gtk::Label>()
                .expect("GTK preview label");
        assert_eq!(preview.text(), "alpha content");
        host.window.destroy();
    }

    #[test]
    fn gtk_pane_tabs_extract_state_and_same_pane_create_params() {
        let view = json!({
            "workspace_id": "workspace-a",
            "pane_id": "pane-a",
            "tabs": [{
                "surface_id": "surface-a",
                "title": "Shell",
                "kind": "terminal",
                "selected": true,
                "pinned": false,
                "unread": false
            }, {
                "surface_ref": "surface:2",
                "title": "Docs",
                "kind": "browser",
                "selected": false,
                "pinned": true,
                "unread": true
            }]
        });

        let tabs = pane_tabs(&view);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].surface_id, "surface-a");
        assert!(tabs[0].selected);
        assert_eq!(tabs[1].surface_id, "surface:2");
        assert_eq!(tabs[1].kind, "browser");
        assert!(tabs[1].pinned);
        assert!(tabs[1].unread);
        assert_eq!(pane_tab_icon("terminal"), "utilities-terminal-symbolic");
        assert_eq!(pane_tab_icon("browser"), "web-browser-symbolic");

        let params = pane_new_terminal_params(&view).unwrap();
        assert_eq!(params["workspace_id"], "workspace-a");
        assert_eq!(params["pane_id"], "pane-a");
        assert_eq!(params["type"], "terminal");
        assert_eq!(params["focus"], true);
    }

    #[test]
    fn gtk_terminal_search_state_formats_live_match_position() {
        let view = json!({
            "surface_id": "surface-a",
            "terminal_search": {
                "active": true,
                "query": "needle",
                "total": 3,
                "selected": 1
            }
        });
        let state = terminal_search_state(&view).expect("active terminal search");
        assert_eq!(state.surface_id, "surface-a");
        assert_eq!(state.query, "needle");
        assert_eq!(state.total, Some(3));
        assert_eq!(state.selected, Some(1));
        assert_eq!(terminal_search_count_text(&state), "2/3");

        let mut pending = state.clone();
        pending.total = None;
        pending.selected = None;
        assert_eq!(terminal_search_count_text(&pending), "Searching...");

        let mut empty = state;
        empty.total = Some(0);
        empty.selected = Some(0);
        assert_eq!(terminal_search_count_text(&empty), "0/0");
        assert!(terminal_search_state(&json!({
            "surface_id": "surface-a",
            "terminal_search": null
        }))
        .is_none());
    }

    #[test]
    fn gtk_snapshot_rebuild_key_tracks_search_visibility_not_live_results() {
        let active = json!({
            "surface_views": [{
                "surface_id": "surface-a",
                "kind": "terminal",
                "terminal_search": {
                    "active": true,
                    "query": "one",
                    "total": 2,
                    "selected": 0
                }
            }]
        });
        let mut results_changed = active.clone();
        results_changed["surface_views"][0]["terminal_search"]["query"] = json!("two");
        results_changed["surface_views"][0]["terminal_search"]["total"] = json!(7);
        results_changed["surface_views"][0]["terminal_search"]["selected"] = json!(3);
        assert_eq!(
            snapshot_rebuild_key(&active),
            snapshot_rebuild_key(&results_changed)
        );

        let mut closed = active.clone();
        closed["surface_views"][0]["terminal_search"] = Value::Null;
        assert_ne!(snapshot_rebuild_key(&active), snapshot_rebuild_key(&closed));
    }

    #[test]
    fn gtk_native_surface_cache_keys_accept_tree_inventory_rows() {
        assert_eq!(
            ghostty_surface_cache_key(&json!({"id": "terminal-a", "type": "terminal"})),
            Some("terminal-a".to_string())
        );
        assert_eq!(
            ghostty_surface_cache_key(&json!({"id": "browser-a", "type": "browser"})),
            None
        );
    }

    #[test]
    fn gtk_right_sidebar_state_maps_visibility_and_vault_alias() {
        assert!(right_sidebar_visible(&json!({})));
        assert_eq!(right_sidebar_mode(&json!({})), "files");
        let snapshot = json!({
            "right_sidebar": {"visible": false, "mode": "vault"}
        });
        assert!(!right_sidebar_visible(&snapshot));
        assert_eq!(right_sidebar_mode(&snapshot), "sessions");
        assert_eq!(right_sidebar_mode_label("sessions"), "Vault");
        assert_eq!(right_sidebar_focus_generation(&snapshot), 0);
        assert_eq!(
            right_sidebar_focus_generation(&json!({
                "right_sidebar": {"focus_generation": 7}
            })),
            7
        );
    }

    #[test]
    fn gtk_browser_shortcut_results_require_action_and_surface() {
        assert!(browser_shortcut_result(&json!({
            "browser_shortcut_action": "reload",
            "surface_id": "surface-1"
        })));
        assert!(!browser_shortcut_result(&json!({
            "browser_shortcut_action": "reload"
        })));
        assert!(!browser_shortcut_result(&json!({
            "surface_id": "surface-1"
        })));
    }

    #[test]
    fn gtk_clipboard_shortcut_result_requires_copy_action_and_text() {
        let result = json!({
            "clipboard_action": "copy",
            "clipboard_text": "workspace_ref=workspace:1\nworkspace_id=abc"
        });
        assert_eq!(
            clipboard_text_from_shortcut_result(&result),
            Some("workspace_ref=workspace:1\nworkspace_id=abc")
        );
        assert_eq!(
            clipboard_text_from_shortcut_result(&json!({
                "clipboard_action": "paste",
                "clipboard_text": "ignored"
            })),
            None
        );
        assert_eq!(
            clipboard_text_from_shortcut_result(&json!({
                "clipboard_action": "copy"
            })),
            None
        );
    }

    #[test]
    fn gtk_sidebar_file_entries_are_bounded_and_skip_heavy_recursive_directories() {
        let root = tempfile::tempdir().expect("sidebar root");
        fs::write(root.path().join("README.md"), "readme").expect("readme");
        fs::create_dir(root.path().join("src")).expect("src");
        fs::write(root.path().join("src/main.rs"), "fn main() {}").expect("main");
        fs::create_dir(root.path().join("target")).expect("target");
        fs::write(root.path().join("target/output"), "ignored").expect("target output");

        let direct = sidebar_file_entries(root.path(), false, 20);
        assert!(direct.iter().any(|entry| entry.label == "README.md"));
        assert!(direct
            .iter()
            .any(|entry| entry.label == "src" && entry.is_directory));
        assert!(!direct.iter().any(|entry| entry.label == "src/main.rs"));

        let recursive = sidebar_file_entries(root.path(), true, 20);
        assert!(recursive.iter().any(|entry| entry.label == "src/main.rs"));
        assert!(!recursive.iter().any(|entry| entry.label == "target/output"));
        assert!(sidebar_file_entries(root.path(), true, 2).len() <= 2);
    }

    #[test]
    fn gtk_split_layout_preserves_nested_t_split_geometry() {
        let views = vec![
            json!({
                "surface_id": "left",
                "visible": true,
                "frame": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 600.0}
            }),
            json!({
                "surface_id": "top-right",
                "visible": true,
                "frame": {"x": 400.0, "y": 0.0, "width": 400.0, "height": 300.0}
            }),
            json!({
                "surface_id": "bottom-right",
                "visible": true,
                "frame": {"x": 400.0, "y": 300.0, "width": 400.0, "height": 300.0}
            }),
        ];

        let layout = surface_split_layout(&views).expect("split layout");
        let GtkSplitLayout::Split {
            axis,
            divider,
            leading,
            trailing,
            ..
        } = layout
        else {
            panic!("expected horizontal root split");
        };
        assert_eq!(axis, GtkSplitAxis::Horizontal);
        assert_eq!(divider, 400);
        assert!(matches!(
            *leading,
            GtkSplitLayout::Leaf { view_index: 0, .. }
        ));
        let GtkSplitLayout::Split {
            axis,
            divider,
            leading,
            trailing,
            ..
        } = *trailing
        else {
            panic!("expected vertical right subtree");
        };
        assert_eq!(axis, GtkSplitAxis::Vertical);
        assert_eq!(divider, 300);
        assert!(matches!(
            *leading,
            GtkSplitLayout::Leaf { view_index: 1, .. }
        ));
        assert!(matches!(
            *trailing,
            GtkSplitLayout::Leaf { view_index: 2, .. }
        ));
    }

    #[test]
    fn gtk_split_layout_keeps_resized_divider_and_ignores_hidden_frames() {
        let views = vec![
            json!({
                "visible": true,
                "frame": {"x": 10.0, "y": 20.0, "width": 480.0, "height": 600.0}
            }),
            json!({
                "visible": true,
                "frame": {"x": 490.0, "y": 20.0, "width": 320.0, "height": 600.0}
            }),
            json!({"visible": false, "frame": null}),
        ];

        let layout = surface_split_layout(&views).expect("split layout");
        let GtkSplitLayout::Split {
            axis,
            bounds,
            divider,
            ..
        } = layout
        else {
            panic!("expected resized horizontal split");
        };
        assert_eq!(axis, GtkSplitAxis::Horizontal);
        assert_eq!(bounds.left, 10);
        assert_eq!(bounds.right, 810);
        assert_eq!(divider, 490);

        let malformed = vec![json!({"visible": true, "frame": {"width": 20.0}})];
        assert_eq!(surface_split_layout(&malformed), None);
    }

    #[test]
    fn gtk_canvas_minimap_projects_content_viewport_and_letterboxing() {
        let snapshot = GtkCanvasMinimapSnapshot::new(
            vec![(
                "pane-a".to_string(),
                GtkCanvasFrame {
                    x: -200.0,
                    y: 100.0,
                    width: 400.0,
                    height: 300.0,
                },
                true,
            )],
            GtkCanvasFrame {
                x: 900.0,
                y: 500.0,
                width: 400.0,
                height: 300.0,
            },
        );
        assert_eq!(
            snapshot.navigation_bounds,
            GtkCanvasFrame {
                x: -200.0,
                y: 100.0,
                width: 1500.0,
                height: 700.0,
            }
        );
        assert!(snapshot.should_show());

        let wide = GtkCanvasMinimapSnapshot::new(
            vec![(
                "pane-a".to_string(),
                GtkCanvasFrame {
                    x: 0.0,
                    y: 0.0,
                    width: 200.0,
                    height: 100.0,
                },
                false,
            )],
            GtkCanvasFrame {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 100.0,
            },
        );
        let drawing = GtkCanvasFrame {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        assert_eq!(
            wide.projection(drawing),
            GtkCanvasMinimapProjection {
                scale: 0.5,
                origin_x: 0.0,
                origin_y: 25.0,
            }
        );
        assert_eq!(
            wide.projected_navigation_bounds(drawing),
            GtkCanvasFrame {
                x: 0.0,
                y: 25.0,
                width: 100.0,
                height: 50.0,
            }
        );
        assert_eq!(wide.canvas_point(50.0, 50.0, drawing), (100.0, 50.0));
        assert!(!wide.should_show());
    }

    #[test]
    fn gtk_canvas_minimap_visibility_matches_macos_rules() {
        let pane = GtkCanvasFrame {
            x: 40.0,
            y: 40.0,
            width: 120.0,
            height: 90.0,
        };
        let visible = GtkCanvasFrame {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        assert!(
            !GtkCanvasMinimapSnapshot::new(vec![("pane-a".to_string(), pane, false)], visible,)
                .should_show()
        );
        assert!(GtkCanvasMinimapSnapshot::new(
            vec![
                ("pane-a".to_string(), pane, false),
                ("pane-b".to_string(), pane, true),
            ],
            visible,
        )
        .should_show());
        assert!(!GtkCanvasMinimapSnapshot::new(
            vec![
                ("pane-a".to_string(), pane, false),
                ("pane-b".to_string(), pane, true),
            ],
            GtkCanvasFrame {
                width: 0.0,
                height: 0.0,
                ..visible
            },
        )
        .should_show());
    }

    #[test]
    fn gtk_canvas_lifecycle_uses_half_viewport_render_margin() {
        let placement =
            |surface: &str, x: f64, y: f64, width: f64, height: f64| GtkCanvasPlacement {
                view_index: 0,
                pane_id: format!("pane-{surface}"),
                surface_target: surface.to_string(),
                focused: surface == "visible",
                logical_x: x,
                logical_y: y,
                logical_width: width,
                logical_height: height,
                scale: 1.0,
                x,
                y,
                width,
                height,
            };
        let placements = vec![
            placement("visible", 100.0, 80.0, 200.0, 120.0),
            placement("margin", 599.0, 100.0, 100.0, 100.0),
            placement("touching", 600.0, 100.0, 100.0, 100.0),
            placement("far", 900.0, 700.0, 100.0, 100.0),
        ];
        let rendering = canvas_rendering_targets(
            &placements,
            GtkCanvasFrame {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
            },
            0.5,
        );
        assert!(rendering.contains("visible"));
        assert!(rendering.contains("margin"));
        assert!(!rendering.contains("touching"));
        assert!(!rendering.contains("far"));
    }

    #[test]
    fn gtk_canvas_modifier_scroll_clamps_pan_and_anchors_zoom_to_pointer() {
        assert_eq!(
            canvas_adjustment_after_scroll(100.0, 1.0, 0.0, 500.0, 200.0),
            148.0
        );
        assert_eq!(
            canvas_adjustment_after_scroll(290.0, 1.0, 0.0, 500.0, 200.0),
            300.0
        );
        assert_eq!(
            canvas_adjustment_after_scroll(10.0, -1.0, 0.0, 500.0, 200.0),
            0.0
        );
        assert_eq!(canvas_scroll_pixels(f64::NAN), 0.0);

        let center =
            canvas_zoom_toward_pointer((500.0, 300.0), 1.0, 2.0, (800.0, 600.0), (100.0, 150.0));
        assert_eq!(center, (350.0, 225.0));
        let old_anchor = (500.0 - 400.0 + 100.0, 300.0 - 300.0 + 150.0);
        let new_anchor = (center.0 - 200.0 + 50.0, center.1 - 150.0 + 75.0);
        assert_eq!(old_anchor, new_anchor);
        assert_eq!(
            canvas_zoom_toward_pointer((500.0, 300.0), 1.0, 2.0, (800.0, 600.0), (400.0, 300.0),),
            (500.0, 300.0)
        );
    }

    #[test]
    fn gtk_canvas_layout_positions_scales_and_centers_panes() {
        let snapshot = json!({
            "canvas": {
                "workspace_ref": "workspace:1",
                "mode": "canvas",
                "magnification": 2.0,
                "viewport_center": {"x": 200.0, "y": 100.0},
                "panes": [
                    {
                        "pane_id": "pane-a",
                        "x": -100.0,
                        "y": 50.0,
                        "width": 400.0,
                        "height": 300.0
                    },
                    {
                        "pane_id": "pane-b",
                        "x": 500.0,
                        "y": -50.0,
                        "width": 240.0,
                        "height": 180.0
                    }
                ]
            }
        });
        let views = vec![
            json!({"pane_id": "pane-a", "surface_ref": "surface:1", "visible": true}),
            json!({"pane_id": "pane-b", "surface_ref": "surface:2", "visible": true}),
            json!({"pane_id": "pane-hidden", "visible": false}),
        ];

        assert!(canvas_mode(&snapshot));
        let layout = gtk_canvas_layout(&snapshot, &views).expect("canvas layout");
        assert_eq!(layout.width, 1776.0);
        assert_eq!(layout.height, 996.0);
        assert_eq!(layout.viewport_x, 648.0);
        assert_eq!(layout.viewport_y, 448.0);
        assert_eq!(layout.workspace_target, "workspace:1");
        assert_eq!(layout.scale, 2.0);
        assert_eq!(layout.logical_origin_x, -100.0);
        assert_eq!(layout.logical_origin_y, -100.0);
        assert_eq!(layout.metrics, GtkCanvasMetrics::default());
        assert_eq!(layout.placements.len(), 2);
        assert_eq!(
            layout.placements[0],
            GtkCanvasPlacement {
                view_index: 0,
                pane_id: "pane-a".to_string(),
                surface_target: "surface:1".to_string(),
                focused: false,
                logical_x: -100.0,
                logical_y: 50.0,
                logical_width: 400.0,
                logical_height: 300.0,
                scale: 2.0,
                x: 48.0,
                y: 348.0,
                width: 800.0,
                height: 600.0,
            }
        );
        assert_eq!(
            layout.placements[1],
            GtkCanvasPlacement {
                view_index: 1,
                pane_id: "pane-b".to_string(),
                surface_target: "surface:2".to_string(),
                focused: false,
                logical_x: 500.0,
                logical_y: -50.0,
                logical_width: 240.0,
                logical_height: 180.0,
                scale: 2.0,
                x: 1248.0,
                y: 148.0,
                width: 480.0,
                height: 360.0,
            }
        );
        assert_eq!(
            canvas_test_interaction_frame_params(
                &layout.placements[0],
                GtkCanvasDragRegion::Move,
                80.0,
                -40.0,
            ),
            json!({
                "surface_id": "surface:1",
                "x": -60.0,
                "y": 30.0,
                "width": 400.0,
                "height": 300.0
            })
        );
        assert_eq!(
            canvas_viewport_center(&layout, 348.0, 248.0, 600.0, 400.0),
            (200.0, 100.0)
        );

        let split_snapshot = json!({"canvas": {"mode": "splits"}});
        assert!(!canvas_mode(&split_snapshot));
        assert_eq!(gtk_canvas_layout(&split_snapshot, &views), None);
    }

    #[test]
    fn gtk_canvas_layout_inserts_panes_back_to_front_by_z_index() {
        let snapshot = json!({
            "canvas": {
                "workspace_ref": "workspace:1",
                "mode": "canvas",
                "panes": [
                    {"pane_id": "pane-front", "x": 20, "y": 20, "width": 300, "height": 200, "z_index": 1},
                    {"pane_id": "pane-back", "x": 0, "y": 0, "width": 300, "height": 200, "z_index": 0}
                ]
            }
        });
        let views = vec![
            json!({"pane_id": "pane-front", "surface_ref": "surface:front"}),
            json!({"pane_id": "pane-back", "surface_ref": "surface:back"}),
        ];

        let layout = gtk_canvas_layout(&snapshot, &views).expect("canvas layout");
        assert_eq!(layout.placements[0].pane_id, "pane-back");
        assert_eq!(layout.placements[1].pane_id, "pane-front");
        assert_eq!(layout.placements[0].view_index, 1);
        assert_eq!(layout.placements[1].view_index, 0);
    }

    #[test]
    fn gtk_canvas_resize_hit_regions_and_geometry_match_canvas_metrics() {
        let placement = GtkCanvasPlacement {
            view_index: 0,
            pane_id: "pane-a".to_string(),
            surface_target: "surface:1".to_string(),
            focused: true,
            logical_x: -100.0,
            logical_y: 50.0,
            logical_width: 400.0,
            logical_height: 300.0,
            scale: 2.0,
            x: 48.0,
            y: 348.0,
            width: 800.0,
            height: 600.0,
        };
        let left = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            left: true,
            ..Default::default()
        });
        let top_left = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            left: true,
            top: true,
            ..Default::default()
        });
        let top_right = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            right: true,
            top: true,
            ..Default::default()
        });
        let bottom_right = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            right: true,
            bottom: true,
            ..Default::default()
        });

        assert_eq!(canvas_drag_region(3.0, 100.0, 800.0, 600.0), Some(left));
        assert_eq!(canvas_drag_region(3.0, 3.0, 800.0, 600.0), Some(top_left));
        assert_eq!(
            canvas_drag_region(797.0, 3.0, 800.0, 600.0),
            Some(top_right)
        );
        assert_eq!(
            canvas_drag_region(400.0, 50.0, 800.0, 600.0),
            Some(GtkCanvasDragRegion::Move)
        );
        assert_eq!(canvas_drag_region(400.0, 150.0, 800.0, 600.0), None);
        assert_eq!(canvas_drag_region(f64::NAN, 3.0, 800.0, 600.0), None);
        assert_eq!(canvas_drag_cursor_name(left), Some("ew-resize"));
        assert_eq!(canvas_drag_cursor_name(top_left), Some("nwse-resize"));
        assert_eq!(canvas_drag_cursor_name(top_right), Some("nesw-resize"));

        assert_eq!(
            canvas_test_interaction_frame(&placement, bottom_right, 100.0, -40.0),
            GtkCanvasFrame {
                x: -100.0,
                y: 50.0,
                width: 450.0,
                height: 280.0,
            }
        );
        let clamped = canvas_test_interaction_frame(&placement, top_left, 1000.0, 1000.0);
        assert_eq!(
            clamped,
            GtkCanvasFrame {
                x: 100.0,
                y: 230.0,
                width: 200.0,
                height: 120.0,
            }
        );
        assert_eq!(
            canvas_rendered_frame(&placement, clamped),
            (448.0, 708.0, 400, 240)
        );
        assert_eq!(
            canvas_test_interaction_frame_params(&placement, bottom_right, 100.0, -40.0),
            json!({
                "surface_id": "surface:1",
                "x": -100.0,
                "y": 50.0,
                "width": 450.0,
                "height": 280.0
            })
        );
    }

    #[test]
    fn gtk_canvas_snap_engine_matches_edge_gap_center_and_resize_rules() {
        let metrics = GtkCanvasMetrics::default();
        let neighbor = GtkCanvasFrame {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        };
        let moved = canvas_snap_result(
            GtkCanvasFrame {
                x: 5.0,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            },
            GtkCanvasDragRegion::Move,
            &[neighbor],
            metrics,
            true,
        );
        assert_eq!(moved.frame.x, 0.0);
        assert_eq!(moved.frame.y, 400.0);
        assert_eq!(moved.guides.len(), 1);
        assert_eq!(moved.guides[0].axis, GtkCanvasGuideAxis::Vertical);
        assert_eq!(moved.guides[0].position, 0.0);
        assert_eq!(moved.guides[0].span_start, 0.0);
        assert_eq!(moved.guides[0].span_end, 600.0);

        let gap = canvas_snap_result(
            GtkCanvasFrame {
                x: 312.0,
                y: 500.0,
                width: 300.0,
                height: 200.0,
            },
            GtkCanvasDragRegion::Move,
            &[neighbor],
            metrics,
            true,
        );
        assert_eq!(gap.frame.x, 316.0);
        assert_eq!(gap.guides[0].position, 316.0);

        let both = canvas_snap_result(
            GtkCanvasFrame {
                x: 314.0,
                y: 3.0,
                width: 300.0,
                height: 200.0,
            },
            GtkCanvasDragRegion::Move,
            &[neighbor],
            metrics,
            true,
        );
        assert_eq!((both.frame.x, both.frame.y), (316.0, 0.0));
        assert_eq!(both.guides.len(), 2);

        let beyond = GtkCanvasFrame {
            x: 9.0,
            y: 400.0,
            width: 300.0,
            height: 200.0,
        };
        assert_eq!(
            canvas_snap_result(
                beyond,
                GtkCanvasDragRegion::Move,
                &[neighbor],
                metrics,
                true,
            ),
            GtkCanvasSnapResult {
                frame: beyond,
                guides: Vec::new(),
            }
        );
        assert_eq!(
            canvas_snap_result(
                GtkCanvasFrame { x: 8.0, ..beyond },
                GtkCanvasDragRegion::Move,
                &[neighbor],
                metrics,
                true,
            )
            .frame
            .x,
            0.0
        );
        assert_eq!(
            canvas_snap_result(
                GtkCanvasFrame { x: 5.0, ..beyond },
                GtkCanvasDragRegion::Move,
                &[neighbor],
                metrics,
                false,
            )
            .frame
            .x,
            5.0
        );

        let right = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            right: true,
            ..Default::default()
        });
        let resized = canvas_snap_result(
            GtkCanvasFrame {
                x: 0.0,
                y: 400.0,
                width: 295.0,
                height: 200.0,
            },
            right,
            &[neighbor],
            metrics,
            true,
        );
        assert_eq!(resized.frame.width, 300.0);
        assert_eq!(resized.guides[0].position, 300.0);

        let left = GtkCanvasDragRegion::Resize(GtkCanvasResizeEdges {
            left: true,
            ..Default::default()
        });
        let gap_resize = canvas_snap_result(
            GtkCanvasFrame {
                x: 320.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
            },
            left,
            &[neighbor],
            metrics,
            true,
        );
        assert_eq!(gap_resize.frame.x, 316.0);
        assert_eq!(gap_resize.frame.x + gap_resize.frame.width, 620.0);

        let clamp_neighbor = GtkCanvasFrame {
            x: 395.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let clamped = canvas_snap_result(
            GtkCanvasFrame {
                x: 400.0,
                y: 0.0,
                width: 150.0,
                height: 200.0,
            },
            left,
            &[clamp_neighbor],
            metrics,
            true,
        );
        assert_eq!(clamped.frame.x, 350.0);
        assert_eq!(clamped.frame.width, 200.0);
        assert!(clamped.guides.is_empty());
    }

    #[test]
    fn gtk_key_mapping_covers_terminal_input() {
        let empty = gdk::ModifierType::empty();
        assert_eq!(
            terminal_input_for_key(gdk::Key::Return, empty),
            Some(TerminalInput::Key("enter".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(gdk::Key::Left, empty),
            Some(TerminalInput::Key("left".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(gdk::Key::Delete, empty),
            Some(TerminalInput::Key("delete".to_string()))
        );

        let a = gdk::Key::from_name("a").expect("a key");
        assert_eq!(
            terminal_input_for_key(a, empty),
            Some(TerminalInput::Text("a".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(a, gdk::ModifierType::CONTROL_MASK),
            Some(TerminalInput::Key("ctrl-a".to_string()))
        );
        let v = gdk::Key::from_name("V").expect("shifted v key");
        assert_eq!(
            terminal_input_for_key(
                v,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
            ),
            Some(TerminalInput::Key("ctrl-shift-v".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(gdk::Key::Return, gdk::ModifierType::SHIFT_MASK),
            Some(TerminalInput::Key("shift-enter".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(a, gdk::ModifierType::ALT_MASK),
            Some(TerminalInput::Text("\x1ba".to_string()))
        );
        assert_eq!(
            terminal_input_for_key(a, gdk::ModifierType::SUPER_MASK),
            None
        );
    }

    #[test]
    fn gtk_control_activation_keys_avoid_terminal_routing() {
        let empty = gdk::ModifierType::empty();
        for key in [
            gdk::Key::Return,
            gdk::Key::KP_Enter,
            gdk::Key::space,
            gdk::Key::KP_Space,
        ] {
            assert!(control_activation_key_should_propagate(true, key, empty));
            assert!(!control_activation_key_should_propagate(false, key, empty));
        }
        assert!(!control_activation_key_should_propagate(
            true,
            gdk::Key::Return,
            gdk::ModifierType::CONTROL_MASK
        ));
        assert!(!control_activation_key_should_propagate(
            true,
            gdk::Key::Escape,
            empty
        ));
    }

    #[test]
    fn stale_browser_widget_does_not_own_input_after_model_focus_moves() {
        assert!(browser_widget_owns_model_focus(
            "browser-surface",
            Some("browser-surface")
        ));
        assert!(!browser_widget_owns_model_focus(
            "browser-surface",
            Some("terminal-surface")
        ));
        assert!(!browser_widget_owns_model_focus("browser-surface", None));
    }

    #[test]
    fn gtk_super_shortcuts_map_to_app_shortcut_combos() {
        let c = gdk::Key::from_name("c").expect("c key");
        assert_eq!(
            app_shortcut_combo_for_key(
                c,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("cmd+shift+c")
        );
        assert_eq!(
            app_shortcut_combo_for_key(
                c,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::CONTROL_MASK
            )
            .as_deref(),
            Some("cmd+ctrl+c")
        );
        let equal = gdk::Key::from_name("equal").expect("equal key");
        assert_eq!(
            app_shortcut_combo_for_key(
                equal,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::ALT_MASK
            )
            .as_deref(),
            Some("cmd+opt+=")
        );
        let minus = gdk::Key::from_name("minus").expect("minus key");
        assert_eq!(
            app_shortcut_combo_for_key(
                minus,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::ALT_MASK
            )
            .as_deref(),
            Some("cmd+opt+-")
        );
        assert_eq!(
            app_shortcut_combo_for_key(
                gdk::Key::Left,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::ALT_MASK
            )
            .as_deref(),
            Some("cmd+opt+left")
        );
        assert_eq!(
            app_shortcut_combo_for_key(gdk::Key::Return, gdk::ModifierType::META_MASK).as_deref(),
            Some("cmd+enter")
        );
        let left_bracket = gdk::Key::from_name("bracketleft").expect("left bracket key");
        assert_eq!(
            app_shortcut_combo_for_key(left_bracket, gdk::ModifierType::SUPER_MASK).as_deref(),
            Some("cmd+[")
        );
        let right_bracket = gdk::Key::from_name("bracketright").expect("right bracket key");
        assert_eq!(
            app_shortcut_combo_for_key(right_bracket, gdk::ModifierType::SUPER_MASK).as_deref(),
            Some("cmd+]")
        );
        let left_brace = gdk::Key::from_name("braceleft").expect("left brace key");
        assert_eq!(
            app_shortcut_combo_for_key(
                left_brace,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("cmd+shift+{")
        );
        let right_brace = gdk::Key::from_name("braceright").expect("right brace key");
        assert_eq!(
            app_shortcut_combo_for_key(
                right_brace,
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("cmd+shift+}")
        );
        let exclamation = gdk::Key::from_name("exclam").expect("exclamation key");
        assert_eq!(
            app_shortcut_combo_for_key(
                exclamation,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("ctrl+shift+1")
        );
        let digit = gdk::Key::from_name("2").expect("digit key");
        assert_eq!(
            app_shortcut_combo_for_key(
                digit,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("ctrl+2")
        );
        assert_eq!(
            app_shortcut_combo_for_key(c, gdk::ModifierType::empty()),
            None
        );
        assert_eq!(
            app_shortcut_combo_for_key(c, gdk::ModifierType::CONTROL_MASK).as_deref(),
            Some("ctrl+c")
        );
        assert_eq!(
            app_shortcut_combo_for_key(c, gdk::ModifierType::ALT_MASK).as_deref(),
            Some("opt+c")
        );
        assert_eq!(
            app_shortcut_combo_for_key(gdk::Key::Page_Up, gdk::ModifierType::CONTROL_MASK)
                .as_deref(),
            Some("ctrl+pageup")
        );
        assert_eq!(
            app_shortcut_combo_for_key(gdk::Key::KP_Page_Down, gdk::ModifierType::CONTROL_MASK)
                .as_deref(),
            Some("ctrl+pagedown")
        );
    }

    #[test]
    fn gtk_omnibar_preserves_directional_pane_shortcuts() {
        let h = gdk::Key::from_name("H").expect("shifted h key");
        assert_eq!(
            omnibar_pane_focus_combo(
                h,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
            )
            .as_deref(),
            Some("ctrl+shift+h")
        );
        assert_eq!(
            omnibar_pane_focus_combo(h, gdk::ModifierType::CONTROL_MASK),
            None
        );
    }

    #[test]
    fn gtk_allocation_maps_to_terminal_grid() {
        assert_eq!(pane_allocation_from_pixels(0, 24), None);
        assert_eq!(
            terminal_grid_for_allocation(GtkPaneAllocation {
                width: 1000,
                height: 640
            }),
            (100, 32)
        );
        assert_eq!(
            terminal_grid_for_allocation(GtkPaneAllocation {
                width: 1,
                height: 1
            }),
            (1, 1)
        );
        assert_eq!(
            pane_id_or_ref(&json!({
                "pane_id": "pane-uuid",
                "pane_ref": "pane:4"
            }))
            .as_deref(),
            Some("pane-uuid")
        );
        assert_eq!(
            pane_id_or_ref(&json!({
                "pane_ref": "pane:5"
            }))
            .as_deref(),
            Some("pane:5")
        );
    }

    #[test]
    fn gtk_ghostty_surface_cache_key_prefers_stable_surface_id() {
        let terminal = json!({
            "kind": "terminal",
            "surface_id": "terminal-uuid",
            "surface_ref": "surface:4"
        });
        assert_eq!(
            ghostty_surface_cache_key(&terminal).as_deref(),
            Some("terminal-uuid")
        );

        let terminal_ref_only = json!({
            "kind": "terminal",
            "surface_ref": "surface:5"
        });
        assert_eq!(
            ghostty_surface_cache_key(&terminal_ref_only).as_deref(),
            Some("surface:5")
        );

        let hibernated = json!({
            "kind": "terminal",
            "surface_id": "hibernated-terminal",
            "hibernated": true,
            "agent_hibernation": {"hibernated_at_ms": 1}
        });
        assert_eq!(ghostty_surface_cache_key(&hibernated), None);

        let browser = json!({
            "kind": "browser",
            "surface_id": "browser-uuid"
        });
        assert_eq!(ghostty_surface_cache_key(&browser), None);
    }

    #[test]
    fn gtk_ghostty_surface_options_forward_initial_input() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let view = json!({
            "kind": "terminal",
            "current_directory": "/tmp/cmux-workspace",
            "terminal_command": "printf ready",
            "terminal_initial_input": "  echo boot\n",
            "terminal_restore_output": "saved output\n",
            "terminal_font_size": 18.5,
            "terminal_wait_after_command": true,
            "terminal_env": {
                "CMUX_SURFACE_ID": "surface-a"
            },
            "surface_id": "terminal-uuid",
            "surface_ref": "surface:4",
            "focused": true,
            "terminal_scrollbar": {
                "total": 500,
                "offset": 120,
                "len": 40
            }
        });

        let options = ghostty_surface_options(&view, &app_state, 7);

        assert_eq!(
            options.working_directory.as_deref(),
            Some("/tmp/cmux-workspace")
        );
        assert_eq!(options.command.as_deref(), Some("printf ready"));
        assert_eq!(options.initial_input.as_deref(), Some("  echo boot\n"));
        assert_eq!(options.initial_output.as_deref(), Some("saved output\n"));
        assert_eq!(options.font_size, Some(18.5));
        assert!(options.wait_after_command);
        assert_eq!(options.env.len(), 1);
        assert!(options.focused);
        assert_eq!(
            options.scrollbar,
            Some(crate::gtk_ghostty::GhosttyScrollbarState {
                total: 500,
                offset: 120,
                len: 40
            })
        );
        assert_eq!(options.config_reload_generation, 7);
        assert_eq!(options.close_surface_id.as_deref(), Some("terminal-uuid"));
    }

    #[test]
    fn gtk_config_reload_generation_reads_renderer_snapshot() {
        assert_eq!(
            config_reload_generation(&json!({
                "config": {
                    "reload_generation": 4
                }
            })),
            4
        );
        assert_eq!(
            config_reload_generation(&json!({
                "config": {
                    "config_reload_generation": 5
                }
            })),
            5
        );
        assert_eq!(config_reload_generation(&json!({})), 0);
    }

    #[test]
    fn gtk_chrome_helpers_extract_snapshot_rows() {
        let workspace = json!({
            "title": "Ops",
            "workspace_ref": "workspace:2",
            "selected": true,
            "custom_title": true,
            "pinned": true,
            "unread": true
        });
        let workspace_id = workspace_id_or_ref(&workspace).expect("workspace ref");
        let rename_params =
            workspace_rename_params(&workspace_id, "  Ops Renamed  ").expect("rename params");
        assert_eq!(rename_params["workspace_id"], "workspace:2");
        assert_eq!(rename_params["title"], "Ops Renamed");
        assert!(workspace_rename_params(&workspace_id, "   ").is_none());
        let action_params =
            workspace_action_params(&workspace_id, "mark-read").expect("workspace action params");
        assert_eq!(action_params["workspace_id"], "workspace:2");
        assert_eq!(action_params["action"], "mark-read");
        assert!(workspace_action_params("   ", "pin").is_none());
        let new_group_params = workspace_new_group_params(&workspace_id).expect("new group params");
        assert_eq!(
            new_group_params["child_workspace_ids"],
            json!(["workspace:2"])
        );
        let remove_group_params =
            workspace_remove_from_group_params(&workspace_id).expect("remove group params");
        assert_eq!(remove_group_params["workspace_id"], "workspace:2");
        let move_group_params = workspace_move_to_group_params(&workspace_id, "workspace_group:4")
            .expect("move group params");
        assert_eq!(move_group_params["workspace_id"], "workspace:2");
        assert_eq!(move_group_params["group_id"], "workspace_group:4");
        assert!(workspace_new_group_params("   ").is_none());
        assert!(workspace_remove_from_group_params("   ").is_none());
        assert!(workspace_move_to_group_params("   ", "workspace_group:4").is_none());
        assert!(workspace_move_to_group_params("workspace:2", "   ").is_none());
        let group_rename_params =
            workspace_group_rename_params("workspace_group:2", "  Infra Agents  ")
                .expect("group rename params");
        assert_eq!(group_rename_params["group_id"], "workspace_group:2");
        assert_eq!(group_rename_params["name"], "Infra Agents");
        assert!(workspace_group_rename_params("workspace_group:2", "   ").is_none());
        assert!(workspace_close_button_visible(&workspace, 2));
        assert!(!workspace_close_button_visible(&workspace, 1));
        assert!(!workspace_close_button_visible(
            &json!({"workspace_ref": "workspace:3", "selected": false}),
            2
        ));
        let workspace_row = workspace_sidebar_model(&workspace, 2, false, None, Vec::new());
        assert!(workspace_row.custom_title);
        assert!(workspace_row.is_pinned);
        assert!(workspace_row.unread);
        let workspace_actions = workspace_context_action_specs(&workspace_row);
        assert!(workspace_actions.contains(&(WORKSPACE_CLEAR_NAME_LABEL, "clear-name")));
        assert!(workspace_actions.contains(&("Unpin Workspace", "unpin")));
        assert!(workspace_actions.contains(&("Mark Workspace as Read", "mark-read")));
        assert!(workspace_actions.contains(&("Close Other Workspaces", "close-others")));
        let plain_row = workspace_sidebar_model(
            &json!({"workspace_ref": "workspace:3"}),
            2,
            false,
            None,
            Vec::new(),
        );
        let plain_actions = workspace_context_action_specs(&plain_row);
        assert!(plain_actions.contains(&("Pin Workspace", "pin")));
        assert!(plain_actions.contains(&("Mark Workspace as Unread", "mark-unread")));
        assert!(!plain_actions
            .iter()
            .any(|(title, _)| *title == WORKSPACE_CLEAR_NAME_LABEL));

        let status = json!({"key": "build", "value": "passing", "icon": "ok"});
        assert_eq!(status_text(&status), "ok build: passing");

        let notification = json!({
            "id": "notification-1",
            "title": "done",
            "subtitle": "build",
            "body": "tests passed",
            "workspace_id": "workspace-uuid",
            "workspace_ref": "workspace:2",
            "surface_id": "surface-uuid",
            "surface_ref": "surface:4"
        });
        assert_eq!(
            notification_detail(&notification),
            "build - tests passed - surface:4"
        );
        let params = notification_focus_params(&notification);
        assert_eq!(params["workspace_id"], "workspace-uuid");
        assert_eq!(params["surface_id"], "surface-uuid");
        let fallback_params = notification_focus_params(&json!({
            "workspace_ref": "workspace:2",
            "surface_ref": "surface:4"
        }));
        assert_eq!(fallback_params["workspace_id"], "workspace:2");
        assert_eq!(fallback_params["surface_id"], "surface:4");
        let mark_params = notification_mark_params(&notification).expect("notification id");
        assert_eq!(mark_params["notification_id"], "notification-1");
    }

    #[test]
    fn desktop_notifications_deliver_new_unread_once_and_withdraw_when_handled() {
        let initial = json!({
            "notifications": [{"id": "existing", "title": "Before launch", "read": false}]
        });
        let mut tracker = DesktopNotificationTracker::from_snapshot(&initial);
        assert_eq!(
            tracker.update(&initial),
            DesktopNotificationDelta::default()
        );

        let created = json!({
            "notifications": [
                {"id": "new", "title": "Done", "subtitle": "Build", "body": "Tests passed", "read": false},
                {"id": "existing", "title": "Before launch", "read": false}
            ]
        });
        let delta = tracker.update(&created);
        assert_eq!(delta.deliver.len(), 1);
        assert_eq!(notification_id(&delta.deliver[0]), Some("new"));
        assert!(delta.withdraw.is_empty());
        assert_eq!(
            desktop_notification_body(&delta.deliver[0]),
            "Build - Tests passed"
        );
        assert_eq!(
            tracker.update(&created),
            DesktopNotificationDelta::default()
        );

        let handled = json!({
            "notifications": [
                {"id": "new", "title": "Done", "read": true},
                {"id": "existing", "title": "Before launch", "read": false}
            ]
        });
        assert_eq!(
            tracker.update(&handled),
            DesktopNotificationDelta {
                deliver: Vec::new(),
                withdraw: vec!["new".to_string()]
            }
        );
    }

    #[test]
    fn gtk_workspace_sidebar_rows_render_group_headers_and_members() {
        let snapshot = json!({
            "workspaces": [
                {
                    "workspace_id": "anchor",
                    "workspace_ref": "workspace:1",
                    "title": "Anchor",
                    "selected": false,
                    "group_id": "group-1",
                    "is_group_anchor": true
                },
                {
                    "workspace_id": "child",
                    "workspace_ref": "workspace:2",
                    "title": "Child",
                    "selected": true,
                    "custom_color": "#1565C0",
                    "group_id": "group-1",
                    "is_group_anchor": false
                },
                {
                    "workspace_id": "outside",
                    "workspace_ref": "workspace:3",
                    "title": "Outside",
                    "selected": false
                }
            ],
            "workspace_groups": [{
                "group_id": "group-1",
                "group_ref": "workspace_group:1",
                "name": "Project",
                "anchor_workspace_id": "anchor",
                "is_collapsed": false,
                "is_pinned": false,
                "member_count": 2,
                "effective_icon_symbol": "leaf.fill",
                "effective_color": "#7A4FD8"
            }]
        });

        let rows = workspace_sidebar_rows(&snapshot);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].kind, GtkWorkspaceSidebarRowKind::GroupHeader);
        assert_eq!(rows[0].target, "workspace_group:1");
        assert_eq!(rows[0].title, "Project");
        assert_eq!(rows[0].subtitle, "workspace_group:1 · 2 workspaces");
        assert_eq!(rows[0].icon_symbol, "leaf.fill");
        assert_eq!(rows[0].tint_hex.as_deref(), Some("#7A4FD8"));
        assert!(!rows[0].selected);
        assert!(!rows[0].is_pinned);
        assert!(rows[0].configured_context_menu_entries.is_empty());
        assert_eq!(rows[1].kind, GtkWorkspaceSidebarRowKind::Workspace);
        assert_eq!(rows[1].target, "child");
        assert_eq!(rows[1].title, "Child");
        assert!(rows[1].selected);
        assert_eq!(rows[1].tint_hex.as_deref(), Some("#1565C0"));
        assert!(rows[1].close_visible);
        assert!(rows[1].indented);
        assert_eq!(rows[1].group_target.as_deref(), Some("workspace_group:1"));
        assert_eq!(
            rows[1].available_group_targets,
            vec![GtkWorkspaceGroupMenuTarget {
                target: "workspace_group:1".to_string(),
                title: "Project".to_string(),
            }]
        );
        assert_eq!(rows[2].target, "outside");
        assert!(!rows[2].indented);
        assert_eq!(rows[2].group_target, None);
        assert_eq!(rows[2].available_group_targets.len(), 1);
    }

    #[test]
    fn gtk_workspace_color_helpers_normalize_render_styles() {
        assert_eq!(
            hex_color_with_alpha("#1565C0", 0.5).as_deref(),
            Some("rgba(21, 101, 192, 0.500)")
        );
        assert_eq!(hex_color_with_alpha("invalid", 0.5), None);
        assert!(workspace_color_is_builtin("Blue"));
        assert!(!workspace_color_is_builtin("Team Blue"));
    }

    #[test]
    fn gtk_workspace_drag_drop_preserves_partitions_and_supports_group_center_drop() {
        let rows = workspace_sidebar_rows(&json!({
            "workspaces": [
                {
                    "workspace_id": "anchor",
                    "workspace_ref": "workspace:1",
                    "title": "Anchor",
                    "group_id": "group-1",
                    "is_group_anchor": true
                },
                {
                    "workspace_id": "child-a",
                    "workspace_ref": "workspace:2",
                    "title": "Child A",
                    "group_id": "group-1"
                },
                {
                    "workspace_id": "child-b",
                    "workspace_ref": "workspace:3",
                    "title": "Child B",
                    "group_id": "group-1"
                },
                {
                    "workspace_id": "outside",
                    "workspace_ref": "workspace:4",
                    "title": "Outside"
                },
                {
                    "workspace_id": "pinned",
                    "workspace_ref": "workspace:5",
                    "title": "Pinned",
                    "pinned": true
                }
            ],
            "workspace_groups": [{
                "group_id": "group-1",
                "group_ref": "workspace_group:1",
                "name": "Project",
                "anchor_workspace_id": "anchor",
                "member_count": 3
            }]
        }));
        let group = &rows[0];
        let child_a = &rows[1];
        let child_b = &rows[2];
        let outside = &rows[3];
        let pinned = &rows[4];

        let payload = workspace_drag_payload(child_b).expect("workspace drag payload");
        assert_eq!(
            payload,
            GtkWorkspaceDragPayload::Workspace {
                workspace_target: "child-b".to_string(),
                group_target: Some("workspace_group:1".to_string()),
                pinned: false
            }
        );

        let (method, params) =
            workspace_drop_request(&payload, child_a, 2.0, 40.0).expect("group reorder");
        assert_eq!(method, "workspace.reorder");
        assert_eq!(params["workspace_id"], "child-b");
        assert_eq!(params["before_workspace_id"], "child-a");
        let (_, params) =
            workspace_drop_request(&payload, child_a, 38.0, 40.0).expect("after reorder");
        assert_eq!(params["workspace_id"], "child-b");
        assert_eq!(params["after_workspace_id"], "child-a");
        assert!(workspace_drop_request(&payload, outside, 2.0, 40.0).is_none());

        let outside_payload = workspace_drag_payload(outside).expect("outside payload");
        let (method, params) =
            workspace_drop_request(&outside_payload, group, 20.0, 40.0).expect("group add");
        assert_eq!(method, "workspace.group.add");
        assert_eq!(params["workspace_id"], "outside");
        assert_eq!(params["group_id"], "workspace_group:1");
        assert!(workspace_drop_request(&outside_payload, group, 2.0, 40.0).is_none());

        let pinned_payload = workspace_drag_payload(pinned).expect("pinned payload");
        assert!(workspace_drop_request(&pinned_payload, group, 20.0, 40.0).is_none());
        assert!(workspace_drop_request(&pinned_payload, outside, 2.0, 40.0).is_none());

        let group_payload = workspace_drag_payload(group).expect("group payload");
        assert_eq!(
            group_payload,
            GtkWorkspaceDragPayload::Group {
                group_target: "workspace_group:1".to_string(),
                pinned: false
            }
        );
        let (method, params) = workspace_drop_request(&group_payload, outside, 2.0, 40.0)
            .expect("move group before workspace");
        assert_eq!(method, "workspace.group.move");
        assert_eq!(params["group_id"], "workspace_group:1");
        assert_eq!(params["before_workspace_id"], "outside");
        let (_, params) = workspace_drop_request(&group_payload, outside, 38.0, 40.0)
            .expect("move group after workspace");
        assert_eq!(params["after_workspace_id"], "outside");
        assert!(workspace_drop_request(&group_payload, child_a, 2.0, 40.0).is_none());
    }

    #[test]
    fn gtk_workspace_sidebar_rows_extract_group_context_menu_metadata() {
        let snapshot = json!({
            "workspaces": [{
                "workspace_id": "anchor",
                "workspace_ref": "workspace:1",
                "title": "Anchor",
                "selected": true,
                "group_id": "group-1",
                "is_group_anchor": true
            }],
            "workspace_groups": [{
                "group_id": "group-1",
                "group_ref": "workspace_group:1",
                "name": "Configured",
                "anchor_workspace_id": "anchor",
                "is_pinned": true,
                "configured_context_menu_items": [
                    {
                        "type": "action",
                        "action_id": "cmux.newWorkspace",
                        "title": "New Workspace",
                        "tooltip": "Create one",
                        "icon_symbol": "plus.square",
                        "action": {"kind": "builtin", "builtin": "cmux.newWorkspace"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.cloudvm",
                        "title": "Start Cloud VM",
                        "action": {"kind": "builtin", "builtin": "cmux.cloudvm"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.newTerminal",
                        "title": "New Terminal Tab",
                        "action": {"kind": "builtin", "builtin": "cmux.newTerminal"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.newBrowser",
                        "title": "New Browser Tab",
                        "action": {"kind": "builtin", "builtin": "cmux.newBrowser"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.splitRight",
                        "title": "Split Right",
                        "action": {"kind": "builtin", "builtin": "cmux.splitRight"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.splitDown",
                        "title": "Split Down",
                        "action": {"kind": "builtin", "builtin": "cmux.splitDown"}
                    },
                    {"type": "separator", "id": "separator.1"},
                    {
                        "type": "action",
                        "action_id": "worktree",
                        "title": "New Worktree Here",
                        "tooltip": "Run worktree action",
                        "icon_symbol": "leaf.arrow.circlepath",
                        "action": {"kind": "command", "command": "cmux workspace create"}
                    },
                    {
                        "type": "action",
                        "action_id": "codex",
                        "title": "Codex",
                        "action": {"kind": "agent", "agent": "codex", "args": "--sandbox read-only"}
                    },
                    {
                        "type": "action",
                        "action_id": "cmux.config.command.Review%20Notes",
                        "title": "Review Notes",
                        "action": {
                            "kind": "workspace_command",
                            "command_name": "Review Notes",
                            "workspace": {
                                "name": "Review Notes",
                                "cwd": "/tmp/review",
                                "color": "purple",
                                "env": {"REVIEW_MODE": "1"},
                                "layout": {
                                    "pane": {
                                        "surfaces": [{
                                            "type": "terminal",
                                            "name": "Review",
                                            "command": "printf review"
                                        }]
                                    }
                                }
                            }
                        }
                    }
                ]
            }]
        });

        let rows = workspace_sidebar_rows(&snapshot);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(row.is_pinned);
        assert_eq!(row.anchor_workspace_target, "anchor");
        assert_eq!(row.configured_context_menu_entries.len(), 10);

        let GtkWorkspaceGroupConfiguredMenuEntry::Action(new_workspace) =
            &row.configured_context_menu_entries[0]
        else {
            panic!("first configured item should be an action");
        };
        assert_eq!(new_workspace.title, "New Workspace");
        assert_eq!(new_workspace.tooltip.as_deref(), Some("Create one"));
        assert_eq!(new_workspace.icon_symbol.as_deref(), Some("plus.square"));
        assert_eq!(new_workspace.action_id, "cmux.newWorkspace");
        assert_eq!(new_workspace.action_kind, "builtin");
        assert_eq!(new_workspace.builtin.as_deref(), Some("cmux.newWorkspace"));
        let (method, params) = workspace_group_configured_action_request(row, new_workspace)
            .expect("new workspace request");
        assert_eq!(method, "workspace.group.new_workspace");
        assert_eq!(params["group_id"], "workspace_group:1");

        let GtkWorkspaceGroupConfiguredMenuEntry::Action(cloud_vm) =
            &row.configured_context_menu_entries[1]
        else {
            panic!("cloud vm item should be an action");
        };
        let (method, params) =
            workspace_group_configured_action_request(row, cloud_vm).expect("cloud vm request");
        assert_eq!(method, "vm.create");
        assert_eq!(params["group_id"], "workspace_group:1");
        assert_eq!(params["source"], "workspace_group_context_menu");
        assert_eq!(params["action_id"], "cmux.cloudvm");
        assert!(params["idempotency_key"]
            .as_str()
            .is_some_and(|value| value.starts_with("gtk-workspace-group-")));

        for (index, expected_method, expected_type, expected_direction) in [
            (2, "surface.create", "terminal", None),
            (3, "surface.create", "browser", None),
            (4, "surface.split", "terminal", Some("right")),
            (5, "surface.split", "terminal", Some("down")),
        ] {
            let GtkWorkspaceGroupConfiguredMenuEntry::Action(action) =
                &row.configured_context_menu_entries[index]
            else {
                panic!("configured item {index} should be an action");
            };
            let (method, params) = workspace_group_configured_action_request(row, action)
                .expect("supported builtin request");
            assert_eq!(method, expected_method);
            assert_eq!(params["workspace_id"], "anchor");
            assert_eq!(params["type"], expected_type);
            assert_eq!(params["focus"], true);
            if let Some(direction) = expected_direction {
                assert_eq!(params["direction"], direction);
            } else {
                assert!(params.get("direction").is_none());
            }
        }

        assert_eq!(
            row.configured_context_menu_entries[6],
            GtkWorkspaceGroupConfiguredMenuEntry::Separator
        );
        let GtkWorkspaceGroupConfiguredMenuEntry::Action(worktree) =
            &row.configured_context_menu_entries[7]
        else {
            panic!("worktree configured item should be an action");
        };
        assert_eq!(worktree.title, "New Worktree Here");
        let (method, params) =
            workspace_group_configured_action_request(row, worktree).expect("worktree request");
        assert_eq!(method, "surface.create");
        assert_eq!(params["workspace_id"], "anchor");
        assert_eq!(params["type"], "terminal");
        assert_eq!(params["title"], "New Worktree Here");
        assert_eq!(params["command"], "cmux workspace create");
        assert_eq!(params["focus"], true);

        let GtkWorkspaceGroupConfiguredMenuEntry::Action(agent) =
            &row.configured_context_menu_entries[8]
        else {
            panic!("agent configured item should be an action");
        };
        let (method, params) =
            workspace_group_configured_action_request(row, agent).expect("agent request");
        assert_eq!(method, "surface.create");
        assert_eq!(params["workspace_id"], "anchor");
        assert_eq!(params["command"], "codex --sandbox read-only");
        assert_eq!(params["title"], "Codex");

        let GtkWorkspaceGroupConfiguredMenuEntry::Action(review) =
            &row.configured_context_menu_entries[9]
        else {
            panic!("workspace command item should be an action");
        };
        let (method, params) =
            workspace_group_configured_action_request(row, review).expect("workspace request");
        assert_eq!(method, "workspace.group.new_workspace");
        assert_eq!(params["group_id"], "workspace_group:1");
        assert_eq!(params["title"], "Review Notes");
        assert_eq!(params["cwd"], "/tmp/review");
        assert_eq!(params["color"], "purple");
        assert_eq!(params["workspace_env"]["REVIEW_MODE"], "1");
        assert_eq!(
            params["layout"]["pane"]["surfaces"][0]["command"],
            "printf review"
        );
    }

    #[test]
    fn gtk_workspace_sidebar_rows_hide_collapsed_group_members() {
        let snapshot = json!({
            "workspaces": [
                {
                    "workspace_id": "anchor",
                    "workspace_ref": "workspace:1",
                    "title": "Anchor",
                    "selected": false,
                    "group_id": "group-1",
                    "is_group_anchor": true
                },
                {
                    "workspace_id": "child",
                    "workspace_ref": "workspace:2",
                    "title": "Child",
                    "selected": true,
                    "group_id": "group-1"
                },
                {
                    "workspace_id": "outside",
                    "workspace_ref": "workspace:3",
                    "title": "Outside",
                    "selected": false
                }
            ],
            "workspace_groups": [{
                "group_id": "group-1",
                "group_ref": "workspace_group:1",
                "name": "Collapsed",
                "anchor_workspace_id": "anchor",
                "is_collapsed": true,
                "member_workspace_ids": ["anchor", "child"]
            }]
        });

        let rows = workspace_sidebar_rows(&snapshot);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, GtkWorkspaceSidebarRowKind::GroupHeader);
        assert_eq!(rows[0].title, "Collapsed");
        assert_eq!(rows[0].subtitle, "workspace_group:1 · 2 workspaces");
        assert!(rows[0].collapsed);
        assert!(rows[0].selected);
        assert_eq!(rows[1].target, "outside");
    }

    #[test]
    fn gtk_workspace_group_title_markup_uses_valid_tint_only() {
        let mut row = GtkWorkspaceSidebarRow {
            kind: GtkWorkspaceSidebarRowKind::GroupHeader,
            target: "workspace_group:1".to_string(),
            anchor_workspace_target: "workspace:1".to_string(),
            group_target: None,
            available_group_targets: Vec::new(),
            title: "A < B".to_string(),
            subtitle: String::new(),
            selected: false,
            multi_selected: false,
            close_visible: false,
            indented: false,
            collapsed: false,
            custom_title: false,
            is_pinned: false,
            unread: false,
            icon_symbol: "folder.fill".to_string(),
            tint_hex: Some("#abcdef".to_string()),
            description: None,
            cwd: None,
            git_branch: None,
            git_dirty: false,
            latest_notification_text: None,
            ssh_target: None,
            listening_ports: Vec::new(),
            pull_request_urls: Vec::new(),
            status_entries: Vec::new(),
            metadata_blocks: Vec::new(),
            progress: None,
            latest_log: None,
            configured_context_menu_entries: Vec::new(),
        };
        assert_eq!(
            workspace_group_title_markup(&row),
            "<span foreground=\"#abcdef\">folder.fill  A &lt; B</span>"
        );
        row.tint_hex = Some("not-a-color".to_string());
        assert_eq!(workspace_group_title_markup(&row), "folder.fill  A &lt; B");
    }

    #[test]
    fn gtk_workspace_sidebar_details_follow_visibility_and_layout_settings() {
        let row = workspace_sidebar_model(
            &json!({
                "workspace_ref": "workspace:2",
                "title": "Linux Port",
                "description": "Ship\nsidebar parity",
                "cwd": "/home/user/project/cmux",
                "git_branch": "feature/linux",
                "git_dirty": true,
                "latest_notification_text": "Agent needs input",
                "ssh_target": "dev@example.test",
                "listening_ports": [3000, 8080],
                "pull_request_urls": ["https://github.com/example/cmux/pull/42"],
                "status_entries": [{
                    "value": "passing",
                    "color": "#22AA66",
                    "url": "https://ci.example.test"
                }],
                "metadata_blocks": [{"markdown": "Review **ready**"}],
                "progress": {"value": 0.75, "label": "Porting"},
                "latest_log": {
                    "source": "linux",
                    "message": "sidebar refreshed"
                }
            }),
            2,
            false,
            None,
            Vec::new(),
        );
        let mut settings = config::SidebarSettings {
            branch_layout: config::SidebarBranchLayout::Inline,
            path_last_segment_only: true,
            ..config::SidebarSettings::default()
        };
        let details = workspace_sidebar_details(&row, &settings);
        assert_eq!(details.description.as_deref(), Some("Ship sidebar parity"));
        assert_eq!(
            details.branch_directory,
            vec!["feature/linux * · cmux".to_string()]
        );
        assert_eq!(details.notification.as_deref(), Some("Agent needs input"));
        assert_eq!(details.ssh_target.as_deref(), Some("dev@example.test"));
        assert_eq!(details.ports, vec![3000, 8080]);
        assert_eq!(details.pull_requests.len(), 1);
        assert_eq!(details.metadata[0].value, "passing");
        assert_eq!(details.metadata_blocks, vec!["Review **ready**"]);
        assert_eq!(details.progress.as_ref().unwrap().value, 0.75);
        assert_eq!(details.log.as_deref(), Some("linux: sidebar refreshed"));

        settings.hide_all_details = true;
        assert_eq!(
            workspace_sidebar_details(&row, &settings),
            GtkWorkspaceSidebarDetails::default()
        );
    }

    #[test]
    fn gtk_toolbar_places_browser_icon_before_plus_action() {
        assert_eq!(
            toolbar_primary_action_markers(),
            [BROWSER_TOOLBAR_ICON, NEW_WORKSPACE_TOOLBAR_LABEL]
        );
        assert_eq!(BROWSER_TOOLBAR_ICON, "web-browser-symbolic");
        assert_eq!(NEW_WORKSPACE_TOOLBAR_LABEL, "+");
        assert_eq!(GROUP_NEW_WORKSPACE_LABEL, "New Workspace in Group");
        assert_eq!(GROUP_EDIT_CONFIG_LABEL, "Edit Group Config");
        assert_eq!(GROUP_DOCS_LABEL, "Open Workspace Groups Docs");
        assert_eq!(GROUP_DELETE_LABEL, "Delete Group (Close Workspaces)");
        assert_eq!(GROUP_DELETE_CONFIRM_LABEL, "Confirm close workspaces");
        assert_eq!(WORKSPACE_NEW_GROUP_LABEL, "New Group from Workspace");
        assert_eq!(WORKSPACE_REMOVE_FROM_GROUP_LABEL, "Remove from Group");
        assert_eq!(WORKSPACE_MOVE_TO_GROUP_PREFIX, "Move to");
    }

    #[test]
    fn gtk_application_id_matches_desktop_launcher_id() {
        assert_eq!(GTK_APPLICATION_ID, "ai.manaflow.cmux");
    }

    #[test]
    fn gtk_application_uniqueness_follows_launcher_ownership() {
        assert_eq!(gtk_application_flags(true), gio::ApplicationFlags::empty());
        assert_eq!(
            gtk_application_flags(false),
            gio::ApplicationFlags::NON_UNIQUE
        );
    }

    #[test]
    fn gtk_toolbar_new_workspace_request_targets_selected_group() {
        let plain_snapshot = json!({
            "workspaces": [{
                "workspace_ref": "workspace:1",
                "title": "Plain",
                "selected": true
            }]
        });
        let (method, params) = new_workspace_request_for_snapshot(&plain_snapshot);
        assert_eq!(method, "workspace.create");
        assert_eq!(params["title"], "Workspace");
        assert_eq!(params["focus"], true);
        assert_eq!(params["placement"], "afterCurrent");
        assert_eq!(params["inherit_working_directory"], true);

        let configured_plain_snapshot = json!({
            "config": {
                "app": {
                    "newWorkspacePlacement": "end",
                    "workspaceInheritWorkingDirectory": false
                }
            },
            "workspaces": [{
                "workspace_ref": "workspace:1",
                "title": "Plain",
                "selected": true
            }]
        });
        let (method, params) = new_workspace_request_for_snapshot(&configured_plain_snapshot);
        assert_eq!(method, "workspace.create");
        assert_eq!(params["placement"], "end");
        assert_eq!(params["inherit_working_directory"], false);

        let grouped_snapshot = json!({
            "workspaces": [{
                "workspace_ref": "workspace:2",
                "title": "Grouped",
                "selected": true,
                "group_ref": "workspace_group:1",
                "group_id": "group-1"
            }]
        });
        let (method, params) = new_workspace_request_for_snapshot(&grouped_snapshot);
        assert_eq!(method, "workspace.group.new_workspace");
        assert_eq!(params["group_id"], "workspace_group:1");
        assert_eq!(params["focus"], true);
        assert_eq!(params["placement"], "afterCurrent");
        assert_eq!(params["placement_reference"], "current_workspace");

        let grouped_snapshot_without_ref = json!({
            "workspaces": [{
                "workspace_ref": "workspace:3",
                "title": "Grouped",
                "selected": true,
                "group_id": "group-2"
            }]
        });
        let (method, params) = new_workspace_request_for_snapshot(&grouped_snapshot_without_ref);
        assert_eq!(method, "workspace.group.new_workspace");
        assert_eq!(params["group_id"], "group-2");
    }

    #[test]
    fn gtk_terminal_cmd_click_link_helpers_route_http_urls() {
        assert_eq!(
            first_terminal_link("see (https://example.test/path?q=1).").as_deref(),
            Some("https://example.test/path?q=1")
        );
        assert_eq!(
            first_terminal_link("plain text http://localhost:3000/app, next").as_deref(),
            Some("http://localhost:3000/app")
        );
        assert_eq!(first_terminal_link("no browser target here"), None);
        let params = terminal_link_browser_open_params(
            &json!({
                "workspace_id": "workspace-uuid",
                "workspace_ref": "workspace:2",
                "surface_id": "surface-uuid",
                "surface_ref": "surface:4"
            }),
            "https://example.test",
        )
        .expect("browser open params");
        assert_eq!(params["workspace_id"], "workspace-uuid");
        assert_eq!(params["surface_id"], "surface-uuid");
        assert_eq!(params["url"], "https://example.test");
        assert_eq!(params["focus"], true);
        let fallback_params = terminal_link_browser_open_params(
            &json!({
                "workspace_ref": "workspace:2",
                "surface_ref": "surface:4"
            }),
            "https://example.test",
        )
        .expect("fallback browser open params");
        assert_eq!(fallback_params["workspace_id"], "workspace:2");
        assert_eq!(fallback_params["surface_id"], "surface:4");
        assert!(cmd_click_modifier_active(gdk::ModifierType::SUPER_MASK));
        assert!(cmd_click_modifier_active(gdk::ModifierType::META_MASK));
        assert!(!cmd_click_modifier_active(gdk::ModifierType::SHIFT_MASK));
    }

    #[test]
    fn gtk_workspace_sidebar_click_modifiers_map_to_selection_contract() {
        assert_eq!(
            workspace_sidebar_select_params("workspace-uuid", gdk::ModifierType::empty()),
            json!({
                "workspace_id": "workspace-uuid",
                "toggle": false,
                "range": false,
                "extend": false
            })
        );
        assert_eq!(
            workspace_sidebar_select_params(
                "workspace-uuid",
                gdk::ModifierType::SUPER_MASK | gdk::ModifierType::SHIFT_MASK,
            ),
            json!({
                "workspace_id": "workspace-uuid",
                "toggle": false,
                "range": true,
                "extend": true
            })
        );
        assert_eq!(
            workspace_sidebar_select_params("workspace-uuid", gdk::ModifierType::CONTROL_MASK)
                ["toggle"],
            true
        );
    }

    #[test]
    fn gtk_surface_context_menu_actions_target_surface_action() {
        let view = json!({
            "workspace_id": "workspace-uuid",
            "workspace_ref": "workspace:2",
            "window_ref": "window:1",
            "surface_id": "surface-uuid",
            "surface_ref": "surface:4",
            "title": "Logs",
            "kind": "browser",
            "custom_title": true,
            "pinned": true,
            "unread": true
        });

        let rename = surface_rename_params(&view, "  Renamed Logs  ").expect("rename params");
        assert_eq!(rename["workspace_id"], "workspace-uuid");
        assert_eq!(rename["window_id"], "window:1");
        assert_eq!(rename["surface_id"], "surface-uuid");
        assert_eq!(rename["action"], "rename");
        assert_eq!(rename["title"], "Renamed Logs");
        assert!(surface_rename_params(&view, "   ").is_none());

        let close_right =
            surface_action_params(&view, "close-right", &[]).expect("close-right params");
        assert_eq!(close_right["workspace_id"], "workspace-uuid");
        assert_eq!(close_right["surface_id"], "surface-uuid");
        assert_eq!(close_right["action"], "close-right");

        let actions = surface_context_action_specs(&view);
        assert!(actions.contains(&(SURFACE_CLEAR_NAME_LABEL, "clear-name")));
        assert!(actions.contains(&("Unpin Tab", "unpin")));
        assert!(actions.contains(&("Mark Tab as Read", "mark-read")));
        assert!(actions.contains(&("Reload Tab", "reload")));
        assert!(actions.contains(&("Duplicate Tab", "duplicate")));
        assert!(actions.contains(&(SURFACE_DETACH_LABEL, "move-to-new-workspace")));

        assert_eq!(
            surface_context_action_params(&view, "new-browser-right").unwrap()["focus"],
            true
        );
        assert!(surface_context_action_params(&view, "pin").unwrap()["focus"].is_null());

        let terminal = json!({
            "surface_ref": "surface:5",
            "workspace_ref": "workspace:3",
            "kind": "terminal",
            "pinned": false,
            "unread": false
        });
        let terminal_params =
            surface_action_params(&terminal, "pin", &[]).expect("terminal action params");
        assert_eq!(terminal_params["workspace_id"], "workspace:3");
        assert_eq!(terminal_params["surface_id"], "surface:5");
        let terminal_actions = surface_context_action_specs(&terminal);
        assert!(terminal_actions.contains(&("Pin Tab", "pin")));
        assert!(terminal_actions.contains(&("Mark Tab as Unread", "mark-unread")));
        assert!(!terminal_actions
            .iter()
            .any(|(_, action)| *action == "reload"));
        assert!(surface_action_params(&json!({}), "pin", &[]).is_none());
    }

    #[test]
    fn gtk_terminal_display_prefers_ghostty_vt_cells() {
        let view = json!({
            "preview": "fallback",
            "ghostty_vt": {
                "row_count": 3,
                "rows_data": [
                    {"y": 0, "cells": [
                        {"x": 0, "text": "H"},
                        {"x": 1, "text": "i"}
                    ]},
                    {"y": 1, "cells": [
                        {"x": 3, "text": "Z"}
                    ]}
                ]
            }
        });
        assert_eq!(
            terminal_display(&view)
                .map(|display| display.text)
                .as_deref(),
            Some("Hi\n   Z")
        );
    }

    #[test]
    fn gtk_terminal_display_builds_markup_for_styled_cells() {
        let view = json!({
            "preview": "fallback",
            "ghostty_vt": {
                "row_count": 1,
                "rows_data": [
                    {"y": 0, "cells": [
                        {"x": 0, "text": "<", "style": {
                            "fg": {"r": 1, "g": 2, "b": 3},
                            "bg": {"r": 4, "g": 5, "b": 6},
                            "bold": true,
                            "italic": true,
                            "underline": true,
                            "strikethrough": true
                        }},
                        {"x": 1, "text": "&", "style": {}}
                    ]}
                ]
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "<&");
        assert_eq!(
            display.markup.as_deref(),
            Some("<span foreground=\"#010203\" background=\"#040506\" weight=\"bold\" style=\"italic\" underline=\"single\" strikethrough=\"true\">&lt;</span>&amp;")
        );
    }

    #[test]
    fn gtk_terminal_display_marks_ghostty_vt_cursor_cell() {
        let view = json!({
            "preview": "fallback",
            "ghostty_vt": {
                "row_count": 1,
                "cursor": {"visible": true, "in_viewport": true, "x": 1, "y": 0},
                "rows_data": [
                    {"y": 0, "cells": [
                        {"x": 0, "text": "A"},
                        {"x": 1, "text": "B", "style": {
                            "fg": {"r": 1, "g": 2, "b": 3},
                            "bg": {"r": 4, "g": 5, "b": 6}
                        }}
                    ]}
                ]
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "AB");
        assert_eq!(
            display.markup.as_deref(),
            Some("A<span foreground=\"#070809\" background=\"#4aa3c7\">B</span>")
        );
    }

    #[test]
    fn gtk_terminal_display_marks_ghostty_vt_cursor_padding() {
        let view = json!({
            "preview": "fallback",
            "ghostty_vt": {
                "row_count": 1,
                "cursor": {"visible": true, "in_viewport": true, "x": 2, "y": 0},
                "rows_data": [
                    {"y": 0, "cells": [
                        {"x": 0, "text": "A"}
                    ]}
                ]
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "A  ");
        assert_eq!(
            display.markup.as_deref(),
            Some("A <span foreground=\"#070809\" background=\"#4aa3c7\"> </span>")
        );
    }

    #[test]
    fn gtk_terminal_display_keeps_blank_ghostty_vt_cursor_frame() {
        let view = json!({
            "preview": "fallback",
            "ghostty_vt": {
                "row_count": 1,
                "cursor": {"visible": true, "in_viewport": true, "x": 0, "y": 0},
                "rows_data": []
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, " ");
        assert_eq!(
            display.markup.as_deref(),
            Some("<span foreground=\"#070809\" background=\"#4aa3c7\"> </span>")
        );
    }

    #[test]
    fn gtk_terminal_display_uses_render_grid_before_preview() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 3,
                "cursor": {"row": 1, "column": 2, "visible": true},
                "row_spans": [
                    {"row": 0, "column": 0, "text": "A"},
                    {"row": 1, "column": 2, "text": "<"}
                ]
            }
        });
        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "A\n  <");
        assert_eq!(
            display.markup.as_deref(),
            Some("A\n  <span foreground=\"#070809\" background=\"#4aa3c7\">&lt;</span>")
        );
    }

    #[test]
    fn gtk_terminal_display_keeps_blank_render_grid_cursor_frame() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 1,
                "cursor": {"row": 0, "column": 0, "visible": true},
                "row_spans": []
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, " ");
        assert_eq!(
            display.markup.as_deref(),
            Some("<span foreground=\"#070809\" background=\"#4aa3c7\"> </span>")
        );
    }

    #[test]
    fn gtk_terminal_display_marks_render_grid_underline_cursor_cell() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 1,
                "cursor": {
                    "row": 0,
                    "column": 1,
                    "visible": true,
                    "style": "underline",
                    "blinking": true
                },
                "row_spans": [
                    {"row": 0, "column": 0, "text": "A<"}
                ]
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "A<");
        assert_eq!(
            display.markup.as_deref(),
            Some("A<span foreground=\"#73c7df\" underline=\"single\">&lt;</span>")
        );
    }

    #[test]
    fn gtk_terminal_display_marks_render_grid_bar_cursor_padding() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 1,
                "cursor": {"row": 0, "column": 2, "visible": true, "style": "bar"},
                "row_spans": [
                    {"row": 0, "column": 0, "text": "A"}
                ]
            }
        });

        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "A  ");
        assert_eq!(
            display.markup.as_deref(),
            Some("A <span foreground=\"#4aa3c7\">|</span> ")
        );
    }

    #[test]
    fn gtk_terminal_display_applies_render_grid_styles() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 1,
                "styles": [
                    {"id": 0},
                    {
                        "id": 7,
                        "fg": {"r": 8, "g": 9, "b": 10},
                        "bg": {"r": 11, "g": 12, "b": 13},
                        "bold": true,
                        "italic": true,
                        "underline": true,
                        "strikethrough": true
                    }
                ],
                "row_spans": [
                    {"row": 0, "column": 0, "style_id": 7, "text": "<&"}
                ]
            }
        });
        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "<&");
        assert_eq!(
            display.markup.as_deref(),
            Some("<span foreground=\"#08090a\" background=\"#0b0c0d\" weight=\"bold\" style=\"italic\" underline=\"single\" strikethrough=\"true\">&lt;</span><span foreground=\"#08090a\" background=\"#0b0c0d\" weight=\"bold\" style=\"italic\" underline=\"single\" strikethrough=\"true\">&amp;</span>")
        );
    }

    #[test]
    fn gtk_terminal_display_applies_render_grid_terminal_style_flags() {
        let view = json!({
            "preview": "fallback",
            "render_grid": {
                "format": "cmux.render-grid.v1",
                "rows": 1,
                "styles": [
                    {
                        "id": 1,
                        "fg": {"r": 1, "g": 2, "b": 3},
                        "bg": {"r": 4, "g": 5, "b": 6},
                        "inverse": true
                    },
                    {"id": 2, "selected": true},
                    {
                        "id": 3,
                        "bg": {"r": 16, "g": 17, "b": 18},
                        "invisible": true
                    },
                    {"id": 4, "faint": true, "overline": true}
                ],
                "row_spans": [
                    {"row": 0, "column": 0, "style_id": 1, "text": "I"},
                    {"row": 0, "column": 1, "style_id": 2, "text": "S"},
                    {"row": 0, "column": 2, "style_id": 3, "text": "H"},
                    {"row": 0, "column": 3, "style_id": 4, "text": "F"}
                ]
            }
        });
        let display = terminal_display(&view).expect("terminal display");
        assert_eq!(display.text, "ISHF");
        assert_eq!(
            display.markup.as_deref(),
            Some("<span foreground=\"#040506\" background=\"#010203\">I</span><span background=\"#314a55\">S</span><span foreground=\"#101112\" background=\"#101112\">H</span><span weight=\"light\" overline=\"single\">F</span>")
        );
    }

    #[test]
    fn gtk_terminal_display_falls_back_to_preview_without_ghostty_cells() {
        let view = json!({"preview": "fallback"});
        assert_eq!(
            terminal_display(&view)
                .map(|display| display.text)
                .as_deref(),
            Some("fallback")
        );
    }

    #[test]
    fn gtk_terminal_status_formats_progress_and_command_result() {
        let view = json!({
            "terminal_progress": {
                "state": "set",
                "percent": 42
            },
            "terminal_key_sequence": {
                "active": true,
                "trigger": "unicode:U+0078 mods=shift+ctrl"
            },
            "terminal_key_table": "resize",
            "terminal_last_command": {
                "exit_code": 7,
                "duration_ms": 1250
            }
        });
        assert_eq!(
            terminal_status_display(&view).as_deref(),
            Some("Progress: 42% · Key sequence: unicode:U+0078 mods=shift+ctrl · Key table: resize · Last command failed (7) in 1.2s")
        );

        let view = json!({
            "terminal_progress": {
                "state": "indeterminate",
                "percent": null
            },
            "terminal_last_command": {
                "exit_code": null,
                "duration_ms": 500
            }
        });
        assert_eq!(
            terminal_status_display(&view).as_deref(),
            Some("Progress active · Last command finished in 500ms")
        );
    }

    #[test]
    fn gtk_terminal_status_formats_ghostty_runtime_state() {
        let view = json!({
            "terminal_readonly": true,
            "terminal_needs_confirm_quit": true,
            "terminal_mouse_captured": true,
            "terminal_has_selection": true,
            "terminal_mouse_over_link_url": "https://example.test/docs/with/a/very/long/path/that/should/be/truncated",
            "terminal_cursor_visible": false,
            "terminal_cursor_shape": "pointer",
            "terminal_config_reload_count": 2,
            "terminal_last_config_reload_soft": false
        });

        assert_eq!(
            terminal_status_display(&view).as_deref(),
            Some("Read-only · Close confirmation required · Mouse captured · Selection active · Link: https://example.test/docs/with/a/very/long/path/that/should... · Cursor hidden · Cursor: pointer · Config reload: hard #2")
        );
    }

    #[test]
    fn gtk_surface_preview_display_uses_browser_payload() {
        let view = json!({
            "kind": "browser",
            "title": "Tab Title",
            "url": "https://example.test",
            "preview": "Visible page text",
            "browser": {
                "title": "Page Title",
                "developer_tools_visible": true,
                "import_dialog_opened": true,
                "import_dialog_scope": "cookiesAndHistory"
            }
        });

        assert_eq!(
            surface_preview_display(&view).text,
            "Page Title\nhttps://example.test\nDeveloper tools open\nBrowser import dialog open (cookiesAndHistory)\nVisible page text"
        );
    }

    #[test]
    fn gtk_feedback_surface_requires_explicit_url_marker() {
        assert!(is_feedback_surface(&json!({
            "kind": "browser",
            "url": "data:text/html;charset=utf-8,feedback#cmux-feedback"
        })));
        assert!(!is_feedback_surface(&json!({
            "kind": "browser",
            "url": "data:text/html;charset=utf-8,feedback"
        })));
        assert!(!is_feedback_surface(&json!({
            "kind": "terminal",
            "url": "data:text/html;charset=utf-8,feedback#cmux-feedback"
        })));
    }

    #[test]
    fn gtk_surface_preview_display_uses_project_payload() {
        let view = json!({
            "kind": "project",
            "title": "cmux-port",
            "preview": "Project\nPath: /tmp/cmux-port\nTab: files",
            "project": {
                "project_url": "/tmp/cmux-port"
            }
        });

        assert_eq!(
            surface_preview_display(&view).text,
            "cmux-port\n/tmp/cmux-port\nProject\nPath: /tmp/cmux-port\nTab: files"
        );
    }

    #[test]
    fn gtk_terminal_preview_shows_loading_message_for_empty_loading_terminal() {
        let view = json!({
            "terminal_loading": true,
            "loading_message": "Loading terminal..."
        });
        assert_eq!(terminal_preview_display(&view).text, "Loading terminal...");
    }

    #[test]
    fn gtk_terminal_preview_keeps_blank_for_empty_non_loading_terminal() {
        let view = json!({});
        assert_eq!(terminal_preview_display(&view).text, " ");
    }

    #[test]
    fn gtk_shortcut_help_dismissal_routes_all_interactions_through_model_action() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        let window_id = call_app_value(&app_state, "help.shortcuts", json!({}))
            .expect("shortcut help state")["window_id"]
            .as_str()
            .expect("window id")
            .to_string();

        for interaction in [
            ShortcutHelpDismissInteraction::CloseButton,
            ShortcutHelpDismissInteraction::BackdropPress,
            ShortcutHelpDismissInteraction::PlainEscape,
        ] {
            assert!(call_app(
                &app_state,
                "help.shortcuts.toggle",
                json!({"window_id": window_id, "visible": true})
            ));
            assert!(handle_shortcut_help_dismissal(
                &app_state,
                &window_id,
                interaction
            ));
            assert_eq!(
                call_app_value(
                    &app_state,
                    "help.shortcuts",
                    json!({"window_id": window_id})
                )
                .expect("shortcut help state")["visible"],
                false
            );
        }

        assert!(call_app(
            &app_state,
            "help.shortcuts.toggle",
            json!({"window_id": window_id, "visible": true})
        ));
        assert!(!handle_shortcut_help_dismissal(
            &app_state,
            &window_id,
            ShortcutHelpDismissInteraction::PanelPress
        ));
        assert_eq!(
            call_app_value(
                &app_state,
                "help.shortcuts",
                json!({"window_id": window_id})
            )
            .expect("shortcut help state")["visible"],
            true
        );
    }

    #[test]
    fn gtk_shortcut_help_panel_requires_visible_state() {
        let hidden = json!({
            "shortcut_help": {
                "visible": false,
                "rows": []
            }
        });
        assert!(shortcut_help_panel(&hidden, None).is_none());

        let visible = json!({
            "shortcut_help": {
                "visible": true,
                "title": "Keyboard Shortcuts",
                "rows": [{
                    "title": "New Terminal",
                    "shortcut_hint": "⌘T",
                    "description": "Create a terminal"
                }]
            }
        });
        assert!(shortcut_help_visible(&visible));
    }

    #[test]
    fn gtk_runtime_widget_regressions() {
        if gtk::init().is_err() {
            return;
        }
        assert_gtk_pane_tab_reconciliation_preserves_widgets_and_scroll_position();
        assert_gtk_tab_create_focus_and_close_refresh_before_fallback_poll();
        assert_gtk_selected_tab_changes_keep_main_tree_mounted_and_content_nonblank();

        let rows = (0..40)
            .map(|index| {
                json!({
                    "title": format!("Shortcut {index}"),
                    "shortcut_label": format!("Super+{index}"),
                    "description": format!("Description {index}")
                })
            })
            .collect::<Vec<_>>();
        let snapshot = json!({
            "shortcut_help": {
                "visible": true,
                "title": "Keyboard Shortcuts",
                "rows": rows
            }
        });
        let panel = shortcut_help_panel(&snapshot, None).expect("shortcut help panel");
        let panel_widget = panel.clone().upcast::<gtk::Widget>();
        let header = widget_descendant_with_css_class(&panel_widget, "cmux-shortcut-help-header")
            .expect("fixed shortcut help header");
        let scroller = widget_descendant_with_css_class(&panel_widget, "cmux-shortcut-help-scroll")
            .expect("shortcut help row scroller")
            .downcast::<gtk::ScrolledWindow>()
            .expect("shortcut help rows use a scrolled window");
        let rows = widget_descendant_with_css_class(&panel_widget, "cmux-shortcut-help-rows")
            .expect("shortcut help rows container");

        assert_eq!(header.parent().as_ref(), Some(&panel_widget));
        assert_eq!(scroller.parent().as_ref(), Some(&panel_widget));
        assert!(widget_is_or_descendant_of(&rows, scroller.upcast_ref()));
        assert_eq!(scroller.hscrollbar_policy(), gtk::PolicyType::Never);
        assert_eq!(scroller.vscrollbar_policy(), gtk::PolicyType::Automatic);
        assert!(scroller.vexpands());
        assert!(scroller.propagates_natural_height());
        assert!(scroller.max_content_height() > 0);

        configure_shortcut_help_overlay_panel(&panel);
        let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
        backdrop.set_size_request(480, 240);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&backdrop));
        overlay.add_overlay(&panel);
        overlay.set_measure_overlay(&panel, false);
        let window = gtk::Window::builder()
            .default_width(480)
            .default_height(240)
            .resizable(false)
            .child(&overlay)
            .build();
        window.present();
        gtk_run_main_loop_for(Duration::from_millis(100));
        let adjustment = scroller.vadjustment();
        assert!(
            adjustment.upper() > adjustment.page_size(),
            "scroll range upper={} page_size={} window_height={}",
            adjustment.upper(),
            adjustment.page_size(),
            window.height()
        );
        let bottom = adjustment.upper() - adjustment.page_size();
        adjustment.set_value(bottom);
        assert!((adjustment.value() - bottom).abs() < f64::EPSILON);
        window.close();
    }

    #[test]
    fn gtk_shortcut_hints_use_linux_modifier_labels() {
        assert_eq!(linux_shortcut_hint_label("⌘T"), "Super+T");
        assert_eq!(linux_shortcut_hint_label("⇧⌘P"), "Shift+Super+P");
        assert_eq!(linux_shortcut_hint_label("⌥⌘T"), "Alt+Super+T");
        assert_eq!(linux_shortcut_hint_label("⌃⌘W"), "Ctrl+Super+W");
        assert_eq!(linux_shortcut_hint_label(""), "");
        assert_eq!(
            linux_shortcut_label(&json!({
                "shortcut_hint": "⌘T",
                "shortcut_label": "Super+T"
            })),
            "Super+T"
        );
        assert_eq!(
            linux_shortcut_label(&json!({
                "shortcut_hint": "⌘T"
            })),
            "Super+T"
        );
    }

    #[test]
    fn gtk_workspace_description_palette_uses_input_layout_and_routes_submit_keys() {
        assert!(command_palette_input_mode("workspace_description_input"));
        assert!(command_palette_input_mode("rename_input"));
        assert!(!command_palette_input_mode("commands"));
        assert_eq!(
            command_palette_submit_combo(gdk::Key::Return, gdk::ModifierType::empty()),
            Some("enter".to_string())
        );
        assert_eq!(
            command_palette_submit_combo(gdk::Key::KP_Enter, gdk::ModifierType::SHIFT_MASK),
            Some("shift+enter".to_string())
        );
        assert_eq!(
            command_palette_submit_combo(gdk::Key::Escape, gdk::ModifierType::empty()),
            None
        );
    }

    #[test]
    fn gtk_diff_controls_parse_files_match_search_and_compute_scroll_targets() {
        let source = "diff --git a/src/one.rs b/src/one.rs\n-old\n+new\n\
diff --git a/docs/two.md b/docs/two.md\n-before\n+after\n";
        let sections = native_diff_sections(source);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].path, "src/one.rs");
        assert_eq!(sections[1].path, "docs/two.md");
        assert!(sections[0].content.starts_with("diff --git a/src/one.rs"));
        let paths = sections
            .iter()
            .map(|section| section.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(native_diff_matching_sections(&paths, "TWO"), vec![1]);
        assert_eq!(native_diff_matching_sections(&paths, "src"), vec![0]);
        assert!(native_diff_matching_sections(&paths, "missing").is_empty());

        assert_eq!(
            diff_scroll_value("scroll_down", 40.0, 0.0, 500.0, 100.0, 20.0),
            Some(72.0)
        );
        assert_eq!(
            diff_scroll_value("scroll_up", 10.0, 0.0, 500.0, 100.0, 48.0),
            Some(0.0)
        );
        assert_eq!(
            diff_scroll_value("scroll_to_bottom", 10.0, 0.0, 500.0, 100.0, 20.0),
            Some(400.0)
        );
        assert_eq!(
            diff_scroll_value("scroll_to_top", 300.0, 5.0, 500.0, 100.0, 20.0),
            Some(5.0)
        );
        assert_eq!(
            diff_shortcut_combo_for_key(gdk::Key::j, gdk::ModifierType::empty()),
            Some("j".to_string())
        );
        assert_eq!(
            diff_shortcut_combo_for_key(gdk::Key::G, gdk::ModifierType::SHIFT_MASK),
            Some("shift+g".to_string())
        );
    }

    #[test]
    fn gtk_browser_focus_mode_requires_a_released_second_escape() {
        let start = Instant::now();
        let mut state = BrowserFocusEscapeState::default();

        assert_eq!(
            state.press("surface-browser", start),
            BrowserFocusEscapeDecision::Forward
        );
        assert_eq!(
            state.press("surface-browser", start + Duration::from_millis(10)),
            BrowserFocusEscapeDecision::Consume
        );
        state.release("surface-browser");
        assert_eq!(
            state.press("surface-browser", start + Duration::from_millis(100)),
            BrowserFocusEscapeDecision::Exit
        );

        assert_eq!(
            state.press("surface-browser", start + Duration::from_secs(2)),
            BrowserFocusEscapeDecision::Forward
        );
        state.release("surface-browser");
        assert_eq!(
            state.press("surface-browser", start + Duration::from_secs(4)),
            BrowserFocusEscapeDecision::Forward
        );
        state.release("other-surface");
        assert_eq!(
            state.press("surface-browser", start + Duration::from_millis(4010)),
            BrowserFocusEscapeDecision::Consume
        );
    }

    #[test]
    fn gtk_react_grab_runtime_uses_pinned_integrity_and_explicit_state() {
        let activate = react_grab_runtime_script(true);
        assert!(activate.contains("react-grab@0.1.29/dist/index.global.js"));
        assert!(activate.contains(REACT_GRAB_INTEGRITY));
        assert!(activate.contains("api.activate"));
        assert_eq!(
            react_grab_runtime_script(false),
            "window.__REACT_GRAB__?.deactivate()"
        );
    }

    #[test]
    fn gtk_global_search_escapes_browser_needles_and_finds_document_offsets() {
        let script = browser_global_search_script("needle'\"\\line");
        assert_eq!(
            script,
            "window.find(\"needle'\\\"\\\\line\", false, false, true, false, true, false)"
        );
        assert_eq!(
            case_insensitive_match_character_offsets("Alpha NEEDLE omega", "needle"),
            Some((6, 12))
        );
        assert_eq!(
            case_insensitive_match_character_offsets("Alpha omega", "needle"),
            None
        );
    }

    #[test]
    fn gtk_shortcut_editor_uses_linux_combo_spelling() {
        assert_eq!(linux_shortcut_combo_text("cmd+t"), "super+t");
        assert_eq!(
            linux_shortcut_combo_text("cmd+ctrl+opt+shift+left"),
            "super+ctrl+opt+shift+left"
        );
        assert_eq!(linux_shortcut_combo_text("ctrl+y"), "ctrl+y");
        assert_eq!(
            linux_shortcut_combo_text("cmd+ctrl+b cmd+shift+c"),
            "super+ctrl+b super+shift+c"
        );
        assert_eq!(linux_shortcut_combo_text(""), "");
        assert_eq!(
            shortcut_settings_detail(&json!({
                "description": "Create a terminal",
                "when": "terminalFocus && paneCount > 1"
            })),
            "Create a terminal\nWhen: terminalFocus && paneCount > 1"
        );
    }

    #[test]
    fn gtk_shortcut_focus_context_prioritizes_sidebar_and_derives_terminal() {
        let browser_context = shortcut_focus_context_from_flags(false, true, false);
        assert_eq!(browser_context["browserFocus"], true);
        assert_eq!(browser_context["sidebarFocus"], false);
        assert_eq!(browser_context["terminalFocus"], false);

        let sidebar_context = shortcut_focus_context_from_flags(true, true, true);
        assert_eq!(sidebar_context["sidebarFocus"], true);
        assert_eq!(sidebar_context["browserFocus"], false);
        assert_eq!(sidebar_context["terminalFocus"], false);

        let terminal_context = shortcut_focus_context_from_flags(false, false, false);
        assert_eq!(terminal_context["terminalFocus"], true);
    }

    #[test]
    fn gtk_routes_bare_second_stroke_while_shortcut_chord_is_pending() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        assert!(call_app(
            &app_state,
            "debug.shortcut.set",
            json!({"name": "new_terminal", "combo": "ctrl+b c"})
        ));
        assert!(call_app(
            &app_state,
            "debug.shortcut.simulate",
            json!({"combo": "ctrl+b"})
        ));
        assert!(shortcut_chord_pending(&app_state));
        assert_eq!(shortcut_key_name(gdk::Key::c).as_deref(), Some("c"));
        assert!(call_app(
            &app_state,
            "debug.shortcut.simulate",
            json!({"combo": "c"})
        ));
        assert!(!shortcut_chord_pending(&app_state));
    }

    #[test]
    fn gtk_palette_keys_map_to_shortcut_simulation_names() {
        assert_eq!(palette_shortcut_combo("up"), Some("up"));
        assert_eq!(palette_shortcut_combo("page-up"), Some("pageup"));
        assert_eq!(palette_shortcut_combo("ctrl-n"), None);
        assert_eq!(palette_shortcut_combo("ctrl-p"), None);
        assert_eq!(palette_shortcut_combo("left"), None);
    }

    #[test]
    fn gtk_dispatch_routes_backspace_to_visible_palette() {
        let app_state = Arc::new(Mutex::new(
            AppState::with_paths(None, None).expect("app state"),
        ));
        assert!(call_app(
            &app_state,
            "debug.command_palette.toggle",
            json!({})
        ));
        assert!(dispatch_terminal_input(
            &app_state,
            TerminalInput::Text("abc".to_string())
        ));
        assert!(dispatch_terminal_input(
            &app_state,
            TerminalInput::Key("backspace".to_string())
        ));

        let mut app = app_state.lock().expect("app state lock");
        let results = app
            .handle("debug.command_palette.results", &json!({}))
            .expect("palette results");
        assert_eq!(results["query"], "ab");
    }

    #[test]
    fn gtk_resume_prompt_rows_only_include_pending_signed_commands() {
        let prompts = resume_command_prompts(&json!({
            "surface_views": [
                {
                    "surface_id": "surface-prompt",
                    "resume_restore_state": "prompt",
                    "resume_binding": {
                        "command": "codex resume session-1",
                        "cwd": "/tmp/project"
                    }
                },
                {
                    "surface_id": "surface-manual",
                    "resume_restore_state": "manual",
                    "resume_binding": {"command": "printf manual"}
                }
            ]
        }));
        assert_eq!(
            prompts,
            vec![GtkResumeCommandPrompt {
                surface_id: "surface-prompt".to_string(),
                command: "codex resume session-1".to_string(),
                cwd: Some("/tmp/project".to_string())
            }]
        );
    }

    #[test]
    fn gtk_close_confirmation_prompts_parse_renderer_requests() {
        assert_eq!(
            close_confirmation_prompts(&json!({
                "close_confirmations": [
                    {
                        "id": "close-1",
                        "title": "Close tab?",
                        "message": "The terminal is still running.",
                        "accept_label": "Close Tab",
                        "cancel_label": "Keep Open"
                    },
                    {"title": "Missing id"}
                ]
            })),
            vec![GtkCloseConfirmationPrompt {
                id: "close-1".to_string(),
                title: "Close tab?".to_string(),
                message: "The terminal is still running.".to_string(),
                accept_label: "Close Tab".to_string(),
                cancel_label: "Keep Open".to_string(),
            }]
        );
    }

    #[test]
    fn gtk_pane_tab_close_button_remains_visible_for_single_tab_unless_hidden() {
        assert!(pane_tab_close_button_visible(1, false));
        assert!(pane_tab_close_button_visible(2, false));
        assert!(!pane_tab_close_button_visible(1, true));
        assert!(!pane_tab_close_button_visible(2, true));
    }

    #[test]
    fn gtk_browser_profile_choices_ignore_incomplete_records() {
        assert_eq!(
            browser_profile_choices(&json!({
                "profiles": [
                    {"id": "default-id", "name": "Default"},
                    {"id": "work-id", "name": "Work"},
                    {"id": "", "name": "Empty ID"},
                    {"id": "missing-name"},
                    null
                ]
            })),
            vec![
                ("default-id".to_string(), "Default".to_string()),
                ("work-id".to_string(), "Work".to_string())
            ]
        );
    }

    #[test]
    fn gtk_browser_host_recreation_is_limited_to_profile_identity_changes() {
        let state = ui::browser_navigation_state(&json!({
            "kind": "browser",
            "surface_id": "surface-id",
            "browser": {
                "profile_id": "work",
                "profile_data_generation": 7,
                "url": "https://example.test"
            }
        }))
        .unwrap();
        assert!(!browser_surface_requires_recreation("work", 7, &state));
        assert!(browser_surface_requires_recreation("personal", 7, &state));
        assert!(browser_surface_requires_recreation("work", 8, &state));
    }

    #[test]
    fn gtk_browser_omnibar_suggestions_ignore_incomplete_records() {
        assert_eq!(
            browser_omnibar_suggestions(&json!({
                "suggestions": [
                    {
                        "kind": "search",
                        "completion": "cmux linux",
                        "url": "https://duckduckgo.com/?q=cmux%20linux",
                        "title": "Search DuckDuckGo for cmux linux"
                    },
                    {
                        "kind": "history",
                        "url": "https://docs.example.test",
                        "title": "Docs"
                    },
                    {
                        "kind": "switch_tab",
                        "url": "https://guide.example.test",
                        "title": "Guide",
                        "badge": "Switch to tab",
                        "surface_id": "surface-id"
                    },
                    {"kind": "history", "url": ""},
                    {"kind": "history"}
                ]
            })),
            vec![
                BrowserOmnibarSuggestion {
                    kind: "search".to_string(),
                    completion: "cmux linux".to_string(),
                    url: "https://duckduckgo.com/?q=cmux%20linux".to_string(),
                    title: "Search DuckDuckGo for cmux linux".to_string(),
                    badge: None,
                    surface_id: None,
                },
                BrowserOmnibarSuggestion {
                    kind: "history".to_string(),
                    completion: "https://docs.example.test".to_string(),
                    url: "https://docs.example.test".to_string(),
                    title: "Docs".to_string(),
                    badge: None,
                    surface_id: None,
                },
                BrowserOmnibarSuggestion {
                    kind: "switch_tab".to_string(),
                    completion: "https://guide.example.test".to_string(),
                    url: "https://guide.example.test".to_string(),
                    title: "Guide".to_string(),
                    badge: Some("Switch to tab".to_string()),
                    surface_id: Some("surface-id".to_string()),
                }
            ]
        );
        assert_eq!(browser_omnibar_display_text("about:blank"), "");
        assert_eq!(
            browser_omnibar_display_text("https://example.test"),
            "https://example.test"
        );
    }
}
