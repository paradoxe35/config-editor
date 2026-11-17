//! A document's channel to its host (`src/host_channel.rs`, with the ranges
//! as given in `src/host_locks.rs`; wasm32-only in the library) run
//! natively: their tests are beside those files.

#[path = "../src/text_lines.rs"]
mod text_lines;

// Its diagnostics and lists are for the library.
#[allow(dead_code)]
#[path = "../src/remap.rs"]
mod remap;

// Its JSON print is for the library.
#[allow(dead_code)]
#[path = "../src/host_locks.rs"]
mod host_locks;

// Its revision and settle are for the library.
#[allow(dead_code)]
#[path = "../src/host_channel.rs"]
mod host_channel;
