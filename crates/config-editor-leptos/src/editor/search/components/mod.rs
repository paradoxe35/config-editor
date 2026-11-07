//! The views of find and replace and go to line.

mod find_widget;
mod go_to_line;
mod search_highlights;

pub(in crate::editor) use find_widget::FindWidget;
pub(in crate::editor) use go_to_line::GoToLineWidget;
pub(in crate::editor) use search_highlights::SearchHighlights;
