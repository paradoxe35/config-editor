//! The documents view, one component per part: the tab strip and its tabs,
//! and one panel (with its editor) per document.

mod panel;
mod tab;
mod tab_strip;

pub(super) use panel::DocumentPanel;
pub(super) use tab::DocumentTab;
pub(super) use tab_strip::TabStrip;
