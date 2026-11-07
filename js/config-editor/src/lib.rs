//! Browser bindings for `config-editor-leptos`, published to npm as
//! `@pngwasi/config-editor`. The editor itself is the crate's Leptos
//! component, mounted into any element; JavaScript sees a plain class and
//! functions. Framework wrappers (`@pngwasi/config-editor-react`, …) build on
//! this package.
//!
//! - `editor`: the `ConfigEditor` class; `mount` wires the component to it.
//! - `documents_view`: the `ConfigDocuments` class (several documents as
//!   tabs).
//! - `headless`, `documents`, `catalog`: the functions that need no editor
//!   (checks and diffs; parsing, paths and folds; accepted names).
//! - `languages`: language names and `registerLanguage`.
//! - `options`, `values`, `host_diagnostics`, `protect`: reading what JavaScript passes
//!   and shaping what it gets back.
//! - `typings`: the TypeScript declarations wasm-bindgen cannot derive.
//! - `alloc_stats` (feature `alloc-stats`): heap statistics for hunting
//!   leaks.
#![cfg(target_arch = "wasm32")]

#[cfg(feature = "alloc-stats")]
mod alloc_stats;
mod catalog;
mod documents;
mod documents_view;
mod editor;
mod headless;
mod host_diagnostics;
mod languages;
mod mount;
mod options;
mod protect;
mod typings;
mod values;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}
