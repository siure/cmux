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

    #[test]
    fn compiled_gtk_stylesheet_is_registered() {
        register_resources().unwrap();
        let data = gio::resources_lookup_data(STYLESHEET_RESOURCE, gio::ResourceLookupFlags::NONE)
            .unwrap();
        let stylesheet = std::str::from_utf8(data.as_ref()).unwrap();
        assert!(stylesheet.contains("tokens.css"));
        assert!(stylesheet.contains("legacy.css"));
        assert!(stylesheet.contains("next.css"));
    }
}
