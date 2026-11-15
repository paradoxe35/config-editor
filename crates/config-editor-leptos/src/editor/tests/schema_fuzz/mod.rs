//! JSON Schema features checked on random schemas and documents, in every
//! language and Edit-as view: schemas drawn from types, enums and consts,
//! `oneOf`/`anyOf`/`allOf`, `if`/`then`/`else`, `$ref` (recursive too),
//! pattern, additional and dependent properties, `prefixItems`/`items`/
//! `contains`, bounds, secrets and `readOnly`; documents drawn to match
//! them or not, written in each language.
//!
//! Each property draws its cases from a fixed seed (`SCHEMA_FUZZ_SEED`
//! changes it) and runs a modest number of them (`SCHEMA_FUZZ_CASES` runs
//! more, thousands at a time). A failing case is shrunk to a small one and
//! printed as a test to paste (`SCHEMA_FUZZ_FAILS` prints every failing
//! case too); the failures are grouped by class, and a
//! class a known finding explains is allowed by name (see [`known`]).

mod documents;
mod edit_as;
mod fill;
mod known;
mod probe;
mod random;
mod reference;
mod repros;
mod runner;
mod schema_text;
mod schemas;
mod secrets;
mod shrink;
mod suggestions;
mod validation;
mod work;
