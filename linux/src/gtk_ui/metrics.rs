pub(super) const HEADER_HEIGHT: i32 = 30;
pub(super) const SIDEBAR_WIDTH: i32 = 240;
pub(super) const COMPACT_SIDEBAR_WIDTH: i32 = 224;
pub(super) const COMPACT_BREAKPOINT: i32 = 1_100;
pub(super) const RIGHT_SIDEBAR_WIDTH: i32 = 288;
pub(super) const PANE_TAB_HEIGHT: i32 = 28;

pub(super) fn compact_layout_for_width(width: i32) -> bool {
    width > 0 && width < COMPACT_BREAKPOINT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_layout_uses_logical_window_width() {
        assert!(!compact_layout_for_width(COMPACT_BREAKPOINT));
        assert!(compact_layout_for_width(COMPACT_BREAKPOINT - 1));
        assert!(!compact_layout_for_width(0));
    }
}
