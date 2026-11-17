//! Random (seeded, shrinking) tests of what the other random suites leave
//! out: hostile text in every language and the custom and declared ones
//! (random characters, near-valid documents broken on purpose, deep
//! nesting, very long lines, huge numbers, mixed line endings, a byte
//! order mark, control characters, thousands of small items) read every
//! way the editor reads a text (`reading`) and growing its work no faster
//! than the text (`linear`); the changes between two texts (`changes`);
//! the props a host changes on a mounted editor (`live`); and a file's
//! line endings and byte order mark through editing (`endings`).
//!
//! The editor has no format action, so there is no formatting property.
//!
//! Each property runs a modest number of cases (`FUZZ_HOSTILE_CASES` runs
//! more, `FUZZ_HOSTILE_SEED` starts elsewhere); failures are grouped into
//! classes, each shrunk to its smallest case and printed as a test to
//! paste in `repros`. Classes of known findings are listed in `known`.

mod changes;
mod endings;
mod known;
mod linear;
mod live;
mod reading;
mod repros;
mod runner;
mod texts;
