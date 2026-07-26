use anyhow::{anyhow, Result};

pub(super) const UI_MODE_ENV: &str = "CMUX_LINUX_UI";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GtkUiMode {
    #[default]
    Legacy,
    Next,
}

impl GtkUiMode {
    pub(super) fn from_env() -> Result<Self> {
        Self::parse(std::env::var(UI_MODE_ENV).ok().as_deref())
    }

    fn parse(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("legacy") => Ok(Self::Legacy),
            Some("next") => Ok(Self::Next),
            Some(value) => Err(anyhow!(
                "{UI_MODE_ENV} requires legacy or next (got: {value})"
            )),
        }
    }

    pub(super) fn root_css_class(self) -> &'static str {
        match self {
            Self::Legacy => "cmux-ui-legacy",
            Self::Next => "cmux-ui-next",
        }
    }

    pub(super) fn is_next(self) -> bool {
        self == Self::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_ui_mode_defaults_to_legacy() {
        assert_eq!(GtkUiMode::parse(None).unwrap(), GtkUiMode::Legacy);
        assert_eq!(GtkUiMode::parse(Some("")).unwrap(), GtkUiMode::Legacy);
    }

    #[test]
    fn gtk_ui_mode_accepts_known_modes() {
        assert_eq!(GtkUiMode::parse(Some("legacy")).unwrap(), GtkUiMode::Legacy);
        assert_eq!(GtkUiMode::parse(Some("next")).unwrap(), GtkUiMode::Next);
    }

    #[test]
    fn gtk_ui_mode_rejects_unknown_modes() {
        let error = GtkUiMode::parse(Some("future")).unwrap_err().to_string();
        assert!(error.contains(UI_MODE_ENV));
        assert!(error.contains("legacy or next"));
    }
}
