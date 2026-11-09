//! Moving a host's ranges from the text they were given for to the
//! document (`src/remap.rs`, wasm32-only in the library) run natively: its
//! tests are in that file.

#[path = "../src/text_lines.rs"]
mod text_lines;

// Its diagnostics and lists are for the library.
#[allow(dead_code)]
#[path = "../src/remap.rs"]
mod remap;
