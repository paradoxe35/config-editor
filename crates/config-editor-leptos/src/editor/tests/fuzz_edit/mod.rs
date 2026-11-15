//! Random (seeded, shrinking) tests of editing mechanics and conversion in
//! every language: undo and redo, the edit commands keeping documents
//! valid, conversion round trips, Replace all against a model, and folds,
//! find and go to line following edits. Each property runs a modest number
//! of cases (`FUZZ_EDIT_CASES` runs more, `FUZZ_EDIT_SEED` starts
//! elsewhere); failures are grouped into classes, each shrunk to its
//! smallest case and printed as a test to paste here. Classes of known
//! findings are listed in `known`.

mod actions;
mod commands;
mod convert;
mod docs;
mod folds;
mod hooks;
mod known;
mod replace;
mod repros;
mod rich;
mod runner;
mod undo;
mod valid;

// Pasted repros name the languages so.
#[allow(unused_imports, reason = "used by pasted repros")]
use crate::editor::languages::Language;
