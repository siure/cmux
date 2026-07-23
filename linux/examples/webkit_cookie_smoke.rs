#[allow(dead_code)]
#[path = "../src/gtk_webkit.rs"]
mod gtk_webkit;

use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const URL: &str = "https://cmux-cookie-smoke.test/account";
const NAME: &str = "cmux_smoke";

fn main() {
    if let Err(err) = run() {
        eprintln!("webkit cookie smoke failed: {err}");
        std::process::exit(1);
    }
    println!("webkit cookie smoke passed");
}

fn run() -> Result<(), String> {
    gtk_webkit::configure_environment();
    gtk4::init().map_err(|err| format!("initialize GTK: {err}"))?;
    let view = gtk_webkit::GtkWebKitView::new("webkit-cookie-smoke", 0)?;
    let peer = gtk_webkit::GtkWebKitView::new("webkit-cookie-smoke", 0)?;
    let isolated = gtk_webkit::GtkWebKitView::new("webkit-cookie-smoke-isolated", 0)?;
    let main_loop = glib::MainLoop::new(None, false);
    let outcome = Rc::new(RefCell::new(None::<Result<(), String>>));

    view.clear_cookies(URL, Some(NAME))?;
    let view_for_set = view.clone();
    let peer_for_read = peer.clone();
    let isolated_for_read = isolated.clone();
    let loop_for_set = main_loop.clone();
    let outcome_for_set = Rc::clone(&outcome);
    glib::timeout_add_local_once(Duration::from_millis(300), move || {
        if let Err(err) = view_for_set.set_cookie(URL, NAME, "ready", None, None, None) {
            complete(&outcome_for_set, &loop_for_set, Err(err));
            return;
        }
        let loop_for_read = loop_for_set.clone();
        let outcome_for_read = Rc::clone(&outcome_for_set);
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let peer_for_clear = peer_for_read.clone();
            let source_for_verify = view_for_set.clone();
            let isolated_for_verify = isolated_for_read.clone();
            let loop_for_peer = loop_for_read.clone();
            let outcome_for_peer = Rc::clone(&outcome_for_read);
            if let Err(err) = peer_for_read.get_cookies(URL, move |result| {
                let found = result.and_then(|cookies| {
                    cookies
                        .iter()
                        .any(|cookie| cookie.name == NAME && cookie.value == "ready")
                        .then_some(())
                        .ok_or_else(|| "set cookie was not returned by WebKit".to_string())
                });
                if let Err(err) = found {
                    complete(&outcome_for_peer, &loop_for_peer, Err(err));
                    return;
                }
                let loop_for_isolated = loop_for_peer.clone();
                let outcome_for_isolated = Rc::clone(&outcome_for_peer);
                if let Err(err) = isolated_for_verify.get_cookies(URL, move |result| {
                    let isolated = result.and_then(|cookies| {
                        (!cookies.iter().any(|cookie| cookie.name == NAME))
                            .then_some(())
                            .ok_or_else(|| {
                                "cookie leaked into a different WebKit profile".to_string()
                            })
                    });
                    if let Err(err) = isolated {
                        complete(&outcome_for_isolated, &loop_for_isolated, Err(err));
                        return;
                    }
                    if let Err(err) = peer_for_clear.clear_cookies(URL, Some(NAME)) {
                        complete(&outcome_for_isolated, &loop_for_isolated, Err(err));
                        return;
                    }
                    let loop_for_verify = loop_for_isolated.clone();
                    let outcome_for_verify = Rc::clone(&outcome_for_isolated);
                    glib::timeout_add_local_once(Duration::from_millis(500), move || {
                        let loop_for_done = loop_for_verify.clone();
                        let outcome_for_done = Rc::clone(&outcome_for_verify);
                        if let Err(err) = source_for_verify.get_cookies(URL, move |result| {
                            let result = result.and_then(|cookies| {
                                (!cookies.iter().any(|cookie| cookie.name == NAME))
                                    .then_some(())
                                    .ok_or_else(|| {
                                        "cleared cookie was still returned by WebKit".to_string()
                                    })
                            });
                            complete(&outcome_for_done, &loop_for_done, result);
                        }) {
                            complete(&outcome_for_verify, &loop_for_verify, Err(err));
                        }
                    });
                }) {
                    complete(&outcome_for_peer, &loop_for_peer, Err(err));
                }
            }) {
                complete(&outcome_for_read, &loop_for_read, Err(err));
            }
        });
    });

    let loop_for_timeout = main_loop.clone();
    let outcome_for_timeout = Rc::clone(&outcome);
    let timeout_source = glib::timeout_add_local_once(Duration::from_secs(10), move || {
        complete(
            &outcome_for_timeout,
            &loop_for_timeout,
            Err("WebKit cookie smoke timed out".to_string()),
        );
    });
    main_loop.run();
    timeout_source.remove();
    let result = outcome
        .borrow_mut()
        .take()
        .ok_or_else(|| "cookie smoke produced no outcome".to_string())?;
    drop(view);
    drop(peer);
    drop(isolated);
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
    result
}

fn complete(
    outcome: &Rc<RefCell<Option<Result<(), String>>>>,
    main_loop: &glib::MainLoop,
    result: Result<(), String>,
) {
    if outcome.borrow().is_none() {
        *outcome.borrow_mut() = Some(result);
        main_loop.quit();
    }
}
