//! A listed document's ranges as its text is taken for an echo or not
//! (`src/documents_view/echo_ranges.rs`, wasm32-only in the library) run
//! natively: its tests are beside that file.

#[path = "../src/text_lines.rs"]
mod text_lines;

// Its diagnostics and lists are for the library.
#[allow(dead_code)]
#[path = "../src/remap.rs"]
mod remap;

// Its lists of ranges given are for the library.
#[allow(dead_code)]
#[path = "../src/given_ranges.rs"]
mod given_ranges;

#[path = "../src/documents_view/echo_ranges.rs"]
mod echo_ranges;
