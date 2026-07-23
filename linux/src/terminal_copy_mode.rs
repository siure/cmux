const MAX_COUNT: usize = 9_999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyModeMove {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    BeginningOfLine,
    EndOfLine,
}

impl CopyModeMove {
    pub(crate) fn binding_name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Home => "home",
            Self::End => "end",
            Self::BeginningOfLine => "beginning_of_line",
            Self::EndOfLine => "end_of_line",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyModeAction {
    Exit,
    StartSelection,
    ClearSelection,
    CopyAndExit,
    CopyLineAndExit,
    ScrollLines(i32),
    ScrollPage(i32),
    ScrollHalfPage(i32),
    ScrollToTop,
    ScrollToBottom,
    JumpToPrompt(i32),
    StartSearch,
    SearchNext,
    SearchPrevious,
    AdjustSelection(CopyModeMove),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyModeKey {
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    PageUp,
    PageDown,
    Home,
    End,
    Character(char),
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopyModeModifiers {
    pub(crate) super_key: bool,
    pub(crate) shift: bool,
    pub(crate) control: bool,
    pub(crate) alt: bool,
}

impl CopyModeModifiers {
    pub(crate) fn bypasses_copy_mode(self) -> bool {
        self.super_key
    }

    fn plain(self) -> bool {
        !self.super_key && !self.shift && !self.control && !self.alt
    }

    fn shift_only(self) -> bool {
        !self.super_key && self.shift && !self.control && !self.alt
    }

    fn control_only(self) -> bool {
        !self.super_key && !self.shift && self.control && !self.alt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyModeResolution {
    Perform(CopyModeAction, usize),
    Consume,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopyModeInputState {
    count_prefix: Option<usize>,
    pending_yank_line: bool,
    pending_g: bool,
}

impl CopyModeInputState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn resolve(
        &mut self,
        key: CopyModeKey,
        modifiers: CopyModeModifiers,
        has_selection: bool,
    ) -> CopyModeResolution {
        if key == CopyModeKey::Escape {
            self.reset();
            return CopyModeResolution::Perform(CopyModeAction::Exit, 1);
        }

        let character = match key {
            CopyModeKey::Character(value) => value.to_ascii_lowercase(),
            _ => '\0',
        };

        if self.pending_yank_line {
            if character == 'y' && (modifiers.plain() || modifiers.shift_only()) {
                let count = clamp_count(self.count_prefix.unwrap_or(1));
                self.reset();
                return CopyModeResolution::Perform(CopyModeAction::CopyLineAndExit, count);
            }
            self.reset();
        }

        if self.pending_g {
            if character == 'g' && modifiers.plain() {
                let count = clamp_count(self.count_prefix.unwrap_or(1));
                let action = if has_selection {
                    CopyModeAction::AdjustSelection(CopyModeMove::Home)
                } else {
                    CopyModeAction::ScrollToTop
                };
                self.reset();
                return CopyModeResolution::Perform(action, count);
            }
            self.reset();
        }

        if modifiers.plain() && character.is_ascii_digit() {
            let digit = character.to_digit(10).unwrap_or_default() as usize;
            if digit != 0 || self.count_prefix.is_some() {
                self.count_prefix = Some(clamp_count(
                    self.count_prefix.unwrap_or_default() * 10 + digit,
                ));
                return CopyModeResolution::Consume;
            }
        }

        if !has_selection && character == 'y' && modifiers.plain() {
            self.pending_yank_line = true;
            return CopyModeResolution::Consume;
        }
        if character == 'g' && modifiers.plain() {
            self.pending_g = true;
            return CopyModeResolution::Consume;
        }

        let Some(action) = resolve_action(key, character, modifiers, has_selection) else {
            self.reset();
            return CopyModeResolution::Consume;
        };
        let count = clamp_count(self.count_prefix.unwrap_or(1));
        self.reset();
        CopyModeResolution::Perform(action, count)
    }
}

fn resolve_action(
    key: CopyModeKey,
    character: char,
    modifiers: CopyModeModifiers,
    has_selection: bool,
) -> Option<CopyModeAction> {
    let movement = match key {
        CopyModeKey::ArrowUp => Some(CopyModeMove::Up),
        CopyModeKey::ArrowDown => Some(CopyModeMove::Down),
        CopyModeKey::ArrowLeft => Some(CopyModeMove::Left),
        CopyModeKey::ArrowRight => Some(CopyModeMove::Right),
        _ => None,
    };
    if let Some(movement) = movement {
        return Some(CopyModeAction::AdjustSelection(movement));
    }
    match key {
        CopyModeKey::PageUp => {
            return Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageUp)
            } else {
                CopyModeAction::ScrollPage(-1)
            });
        }
        CopyModeKey::PageDown => {
            return Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageDown)
            } else {
                CopyModeAction::ScrollPage(1)
            });
        }
        CopyModeKey::Home => {
            return Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::Home)
            } else {
                CopyModeAction::ScrollToTop
            });
        }
        CopyModeKey::End => {
            return Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::End)
            } else {
                CopyModeAction::ScrollToBottom
            });
        }
        _ => {}
    }

    if modifiers.control_only() {
        return match character {
            'u' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageUp)
            } else {
                CopyModeAction::ScrollHalfPage(-1)
            }),
            'd' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageDown)
            } else {
                CopyModeAction::ScrollHalfPage(1)
            }),
            'b' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageUp)
            } else {
                CopyModeAction::ScrollPage(-1)
            }),
            'f' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::PageDown)
            } else {
                CopyModeAction::ScrollPage(1)
            }),
            'y' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::Up)
            } else {
                CopyModeAction::ScrollLines(-1)
            }),
            'e' => Some(if has_selection {
                CopyModeAction::AdjustSelection(CopyModeMove::Down)
            } else {
                CopyModeAction::ScrollLines(1)
            }),
            _ => None,
        };
    }
    if !modifiers.plain() && !modifiers.shift_only() {
        return None;
    }

    match character {
        'q' => Some(CopyModeAction::Exit),
        'v' => Some(if has_selection {
            CopyModeAction::ClearSelection
        } else {
            CopyModeAction::StartSelection
        }),
        'y' if has_selection => Some(CopyModeAction::CopyAndExit),
        'y' if modifiers.shift_only() => Some(CopyModeAction::CopyLineAndExit),
        'j' => Some(CopyModeAction::AdjustSelection(CopyModeMove::Down)),
        'k' => Some(CopyModeAction::AdjustSelection(CopyModeMove::Up)),
        'h' => Some(CopyModeAction::AdjustSelection(CopyModeMove::Left)),
        'l' => Some(CopyModeAction::AdjustSelection(CopyModeMove::Right)),
        'g' if modifiers.shift_only() => Some(if has_selection {
            CopyModeAction::AdjustSelection(CopyModeMove::End)
        } else {
            CopyModeAction::ScrollToBottom
        }),
        '0' | '^' => Some(CopyModeAction::AdjustSelection(
            CopyModeMove::BeginningOfLine,
        )),
        '$' => Some(CopyModeAction::AdjustSelection(CopyModeMove::EndOfLine)),
        '4' if modifiers.shift_only() => {
            Some(CopyModeAction::AdjustSelection(CopyModeMove::EndOfLine))
        }
        '{' | '[' if modifiers.shift_only() || character == '{' => {
            Some(CopyModeAction::JumpToPrompt(-1))
        }
        '}' | ']' if modifiers.shift_only() || character == '}' => {
            Some(CopyModeAction::JumpToPrompt(1))
        }
        '/' => Some(CopyModeAction::StartSearch),
        'n' if modifiers.shift_only() => Some(CopyModeAction::SearchPrevious),
        'n' => Some(CopyModeAction::SearchNext),
        _ => None,
    }
}

fn clamp_count(count: usize) -> usize {
    count.clamp(1, MAX_COUNT)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopyModeCursor {
    pub(crate) row: i32,
    pub(crate) column: i32,
}

impl CopyModeCursor {
    pub(crate) fn clamp(&mut self, rows: i32, columns: i32) {
        self.row = self.row.clamp(0, rows.max(1) - 1);
        self.column = self.column.clamp(0, columns.max(1) - 1);
    }

    pub(crate) fn move_cursor(
        &mut self,
        movement: CopyModeMove,
        count: usize,
        rows: i32,
        columns: i32,
    ) -> i32 {
        let rows = rows.max(1);
        let columns = columns.max(1);
        let count = clamp_count(count) as i32;
        self.clamp(rows, columns);
        match movement {
            CopyModeMove::Left => self.column = (self.column - count).max(0),
            CopyModeMove::Right => self.column = (self.column + count).min(columns - 1),
            CopyModeMove::Up => return self.move_vertically(-count, rows),
            CopyModeMove::Down => return self.move_vertically(count, rows),
            CopyModeMove::PageUp => return self.move_vertically(-rows * count, rows),
            CopyModeMove::PageDown => return self.move_vertically(rows * count, rows),
            CopyModeMove::Home => {
                self.row = 0;
                self.column = 0;
            }
            CopyModeMove::End => {
                self.row = rows - 1;
                self.column = columns - 1;
            }
            CopyModeMove::BeginningOfLine => self.column = 0,
            CopyModeMove::EndOfLine => self.column = columns - 1,
        }
        0
    }

    pub(crate) fn shift_for_scroll(&mut self, line_delta: i32, rows: i32, columns: i32) {
        self.row -= line_delta;
        self.clamp(rows, columns);
    }

    fn move_vertically(&mut self, delta: i32, rows: i32) -> i32 {
        let target = self.row + delta;
        if target < 0 {
            self.row = 0;
            target
        } else if target >= rows {
            self.row = rows - 1;
            target - (rows - 1)
        } else {
            self.row = target;
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> CopyModeModifiers {
        CopyModeModifiers::default()
    }

    #[test]
    fn resolves_counts_and_two_key_commands() {
        let mut state = CopyModeInputState::default();
        assert_eq!(
            state.resolve(CopyModeKey::Character('3'), plain(), false),
            CopyModeResolution::Consume
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('j'), plain(), false),
            CopyModeResolution::Perform(CopyModeAction::AdjustSelection(CopyModeMove::Down), 3)
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('y'), plain(), false),
            CopyModeResolution::Consume
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('y'), plain(), false),
            CopyModeResolution::Perform(CopyModeAction::CopyLineAndExit, 1)
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('g'), plain(), false),
            CopyModeResolution::Consume
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('g'), plain(), false),
            CopyModeResolution::Perform(CopyModeAction::ScrollToTop, 1)
        );
    }

    #[test]
    fn visual_mode_changes_navigation_and_yank_semantics() {
        let mut state = CopyModeInputState::default();
        assert_eq!(
            state.resolve(CopyModeKey::PageDown, plain(), true),
            CopyModeResolution::Perform(CopyModeAction::AdjustSelection(CopyModeMove::PageDown), 1)
        );
        assert_eq!(
            state.resolve(CopyModeKey::Character('y'), plain(), true),
            CopyModeResolution::Perform(CopyModeAction::CopyAndExit, 1)
        );
    }

    #[test]
    fn cursor_reports_vertical_overflow_and_clamps_horizontal_motion() {
        let mut cursor = CopyModeCursor { row: 2, column: 3 };
        assert_eq!(cursor.move_cursor(CopyModeMove::Down, 3, 4, 5), 2);
        assert_eq!(cursor, CopyModeCursor { row: 3, column: 3 });
        cursor.shift_for_scroll(2, 4, 5);
        assert_eq!(cursor.row, 1);
        assert_eq!(cursor.move_cursor(CopyModeMove::Right, 9, 4, 5), 0);
        assert_eq!(cursor.column, 4);
    }

    #[test]
    fn super_shortcuts_bypass_modal_input() {
        assert!(CopyModeModifiers {
            super_key: true,
            ..CopyModeModifiers::default()
        }
        .bypasses_copy_mode());
    }
}
