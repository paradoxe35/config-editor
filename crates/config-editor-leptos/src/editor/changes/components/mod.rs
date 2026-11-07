//! The views of the changes: the gutter markers, the preview they open and
//! the footer's count.

mod diff_peek;
mod markers;
mod status;

pub(in crate::editor) use diff_peek::DiffPeekPopup;
pub(in crate::editor) use markers::ChangeMarkers;
pub(in crate::editor) use status::ChangeStatus;
