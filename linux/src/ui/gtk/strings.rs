use super::style;
use gtk::gio;
use gtk4 as gtk;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const EN_RESOURCE: &str = "/ai/manaflow/cmux/ui/strings/en.json";
const JA_RESOURCE: &str = "/ai/manaflow/cmux/ui/strings/ja.json";

static EN_STRINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
static JA_STRINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

pub(super) fn text(key: &str) -> String {
    let catalog = if current_locale_is_japanese() {
        JA_STRINGS.get_or_init(|| load_catalog(JA_RESOURCE))
    } else {
        EN_STRINGS.get_or_init(|| load_catalog(EN_RESOURCE))
    };
    catalog
        .get(key)
        .cloned()
        .or_else(|| {
            EN_STRINGS
                .get_or_init(|| load_catalog(EN_RESOURCE))
                .get(key)
                .cloned()
        })
        .unwrap_or_else(|| key.to_string())
}

fn current_locale_is_japanese() -> bool {
    let locales = ["LC_ALL", "LC_MESSAGES", "LANG"].map(std::env::var);
    first_locale_is_japanese(locales.iter().filter_map(|locale| locale.as_deref().ok()))
}

fn first_locale_is_japanese<'a>(locales: impl IntoIterator<Item = &'a str>) -> bool {
    locales
        .into_iter()
        .map(str::trim)
        .find(|locale| !locale.is_empty())
        .is_some_and(|locale| locale.to_ascii_lowercase().starts_with("ja"))
}

fn load_catalog(path: &str) -> BTreeMap<String, String> {
    if style::register_resources().is_err() {
        return BTreeMap::new();
    }
    let Ok(data) = gio::resources_lookup_data(path, gio::ResourceLookupFlags::NONE) else {
        return BTreeMap::new();
    };
    serde_json::from_slice(data.as_ref()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_locale_selection_skips_empty_higher_priority_values() {
        assert!(first_locale_is_japanese(["", "ja_JP.UTF-8", "en_US.UTF-8"]));
        assert!(!first_locale_is_japanese(["", "", "en_US.UTF-8"]));
    }

    #[test]
    fn gtk_string_catalogs_have_matching_nonempty_keys() {
        let english = load_catalog(EN_RESOURCE);
        let japanese = load_catalog(JA_RESOURCE);
        assert!(!english.is_empty());
        assert_eq!(
            english.keys().collect::<Vec<_>>(),
            japanese.keys().collect::<Vec<_>>()
        );
        assert!(english.values().all(|value| !value.trim().is_empty()));
        assert!(japanese.values().all(|value| !value.trim().is_empty()));
    }
}
