//! Jumps: going to a place the user may not be looking at (a definition, a
//! use, a problem, a line, a change, the host's `reveal`). The target is
//! selected, its line centered when it is out of view or near an edge (see
//! `positioning::reveal_jump`), and briefly flashed so the eye finds it.
//! Typing, arrows and clicks scroll as little as they can instead.

use leptos::prelude::*;

use super::positioning::reveal_jump;
use super::state::EditorState;
use crate::editor::text::{TextRange, TextSize, slice};
use crate::reactive::later::{self, Timeout};

/// How long a jump's flash shows, in milliseconds: the `jump-flash` fade
/// in `css/base.css` lasts as long.
const FLASH_MS: i32 = 1_000;

/// A jump's flash: a range of the text it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Flash {
    /// The flashed bytes: their rows get a bar, a non-empty range an edge.
    pub(super) range: TextRange,
    /// The document version (see `DocState::version`) the range is of: an
    /// edit since hides it.
    pub(super) version: u64,
    /// Counts jumps, so a second jump to the same place starts the fade
    /// again.
    pub(super) serial: u64,
}

impl Flash {
    /// Whether the jump went to a place, not a range (only its row's bar
    /// shows).
    pub(super) fn is_place(&self) -> bool {
        self.range.is_empty()
    }
}

/// The current jump's flash and the timer that ends it.
#[derive(Clone, Copy)]
pub(super) struct JumpState {
    pub(super) flash: RwSignal<Option<Flash>>,
    timer: StoredValue<Timeout>,
}

impl JumpState {
    pub(super) fn new() -> Self {
        Self {
            flash: RwSignal::new(None),
            timer: StoredValue::new(Timeout::default()),
        }
    }
}

impl EditorState {
    /// Jumps to `line`:`column` (one-based, UTF-16 columns), the caret
    /// there and its line flashed. Returns the caret's byte offset.
    pub(super) fn jump_to(&self, line: usize, column: usize) -> Option<TextSize> {
        let offset = self.offset_of_position(line, column);
        self.jump_to_range(offset, offset)?;
        Some(offset)
    }

    /// Jumps to bytes `start..end`: selected, revealed and flashed.
    pub(super) fn jump_to_range(&self, start: TextSize, end: TextSize) -> Option<()> {
        let element = self.focus_selection(start, end, false)?;
        self.doc()
            .text
            .with_untracked(|text| reveal_jump(&element, text, start, 0.0));
        self.flash(start, end);
        Some(())
    }

    /// Flashes bytes `start..end` for [`FLASH_MS`].
    fn flash(&self, start: TextSize, end: TextSize) {
        let jump = self.jump();
        let version = self.doc().version.get_value();
        let range = self.doc().text.with_untracked(|text| {
            let end = end.min(TextSize::of(text));
            let start = start.min(end);
            // A range ending at a line's start does not reach that line.
            let end = if end > start && slice::line_start_of(text, end) == end {
                end - TextSize::from(1)
            } else {
                end
            };
            TextRange::new(start, end)
        });
        let serial = jump
            .flash
            .with_untracked(|flash| flash.map_or(0, |flash| flash.serial + 1));
        jump.flash.set(Some(Flash {
            range,
            version,
            serial,
        }));
        let flash = jump.flash;
        later::replace_in(
            jump.timer,
            later::after(flash, FLASH_MS, move |flash| {
                if flash.with_untracked(|shown| shown.is_some_and(|shown| shown.serial == serial)) {
                    flash.set(None);
                }
            }),
        );
    }

    /// The flash to draw: `None` once the text changed under it.
    pub(super) fn shown_flash(&self) -> Option<Flash> {
        let flash = self.jump().flash.get()?;
        (flash.version == self.doc().version.get_value()).then_some(flash)
    }
}
