pub(super) const HEADER_HEIGHT: i32 = 30;
pub(super) const SIDEBAR_WIDTH: i32 = 240;
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
    fn sidebar_widths_reserve_terminal_space_and_preserve_preferences() {
        assert_eq!(sidebar_widths(500, 600, 1200, true, false), (400, 440));
        assert_eq!(sidebar_widths(500, 600, 1200, false, false), (400, 600));
        assert_eq!(sidebar_widths(340, 410, 800, true, true), (266, 410));
        assert_eq!(sidebar_widths(340, 410, 1600, true, false), (340, 410));
        assert_eq!(sidebar_widths(340, 410, 0, true, false), (340, 410));
    }

    #[test]
    fn compact_layout_uses_logical_window_width() {
        assert!(!compact_layout_for_width(COMPACT_BREAKPOINT));
        assert!(compact_layout_for_width(COMPACT_BREAKPOINT - 1));
        assert!(!compact_layout_for_width(0));
    }
}

pub(super) const MIN_TERMINAL_WIDTH: i32 = 360;
pub(super) const MIN_RIGHT_SIDEBAR_WIDTH: i32 = 276;

pub(super) fn sidebar_widths(
    preferred_left: i32,
    preferred_right: i32,
    window_width: i32,
    left_visible: bool,
    compact: bool,
) -> (i32, i32) {
    if window_width <= 0 {
        return (preferred_left, preferred_right);
    }
    let left = preferred_left.clamp(SIDEBAR_WIDTH, (window_width / 3).max(SIDEBAR_WIDTH));
    let right_max = if compact {
        window_width - 48
    } else {
        window_width - if left_visible { left } else { 0 } - MIN_TERMINAL_WIDTH
    };
    let right = preferred_right.clamp(
        MIN_RIGHT_SIDEBAR_WIDTH,
        right_max.max(MIN_RIGHT_SIDEBAR_WIDTH),
    );
    (left, right)
}
