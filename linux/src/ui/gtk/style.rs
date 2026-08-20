use anyhow::{anyhow, Result};
use gtk::gio;
use gtk::glib;
use gtk4 as gtk;
use std::cell::Cell;
use std::sync::OnceLock;

const STYLESHEET_RESOURCE: &str = "/ai/manaflow/cmux/ui/css/cmux.css";

static RESOURCE_REGISTRATION: OnceLock<Result<(), String>> = OnceLock::new();

thread_local! {
    static CSS_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn install() -> Result<()> {
    register_resources()?;
    CSS_INSTALLED.with(|installed| {
        if installed.replace(true) {
            return Ok(());
        }
        let display = gtk::gdk::Display::default()
            .ok_or_else(|| anyhow!("GTK display is unavailable while installing cmux styles"))?;
        let provider = gtk::CssProvider::new();
        provider.load_from_resource(STYLESHEET_RESOURCE);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        Ok(())
    })
}

pub(super) fn register_resources() -> Result<()> {
    RESOURCE_REGISTRATION
        .get_or_init(|| {
            let bytes = glib::Bytes::from_static(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/cmux-ui.gresource"
            )));
            let resource = gio::Resource::from_data(&bytes).map_err(|err| err.to_string())?;
            gio::resources_register(&resource);
            Ok(())
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLESHEET_NAMES: &[&str] = &[
        "tokens.css",
        "legacy.css",
        "next.css",
        "parity-sidebar.css",
        "parity-panes.css",
        "parity-panels.css",
        "parity-overlays.css",
    ];

    const PARITY_STYLESHEET_NAMES: &[&str] = &[
        "parity-sidebar.css",
        "parity-panes.css",
        "parity-panels.css",
        "parity-overlays.css",
    ];

    fn resource_text(path: &str) -> String {
        let data = gio::resources_lookup_data(path, gio::ResourceLookupFlags::NONE).unwrap();
        std::str::from_utf8(data.as_ref()).unwrap().to_owned()
    }

    #[test]
    fn compiled_gtk_stylesheet_imports_registered_layers() {
        register_resources().unwrap();
        let stylesheet = resource_text(STYLESHEET_RESOURCE);

        for name in STYLESHEET_NAMES {
            assert!(
                stylesheet.contains(&format!("@import url(\"{name}\");")),
                "compiled GTK stylesheet does not import {name}"
            );
            let path = format!("/ai/manaflow/cmux/ui/css/{name}");
            assert!(
                !resource_text(&path).trim().is_empty(),
                "compiled GTK resource is empty: {name}"
            );
        }
    }

    #[test]
    fn parity_stylesheet_resources_are_registered_and_scoped() {
        register_resources().unwrap();

        for name in PARITY_STYLESHEET_NAMES {
            let path = format!("/ai/manaflow/cmux/ui/css/{name}");
            let stylesheet = resource_text(&path);
            assert!(
                stylesheet.contains(".cmux-ui-next"),
                "{name} must remain scoped to next mode"
            );
        }
    }
}
