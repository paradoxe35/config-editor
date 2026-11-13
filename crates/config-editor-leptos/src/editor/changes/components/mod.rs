//! The views of the changes: the gutter markers, the preview they open and
//! the footer's count.

mod diff_peek;
mod markers;
mod status;
mod tab_order;

pub(in crate::editor) use diff_peek::DiffPeekPopup;
pub(in crate::editor) use markers::ChangeMarkers;
pub(in crate::editor) use status::ChangeStatus;
pub(in crate::editor) use tab_order::{tab_around_preview, tab_into_preview};
