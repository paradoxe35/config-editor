//! The headless tools' kept schema (`src/schema_cache.rs`, wasm32-only in
//! the library) run natively: its tests are in that file.

#[path = "../src/schema_cache.rs"]
mod schema_cache;
#[path = "../src/schema_spec.rs"]
mod schema_spec;
