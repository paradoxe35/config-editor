//! Suggestions at places only HCL's syntax has (block labels), against
//! the tunnel schema. The suggestion promises every language shares are in
//! `completion_matrix`.
//!
//! `|` marks the caret.

use super::super::Language;
use super::super::harness::completion::{offered};
use super::super::harness::report;

/// Where a name is not a property (an HCL block label, quoted or not),
/// nothing is offered.
#[test]
fn block_labels_get_no_property_suggestions() {
    let cases = [
        "services \"|\" {\n}\n",
        "services |",
        "services we|",
        "services w| {\n}\n",
        "clients m|",
        "clients \"a\" \"|\" {\n}\n",
    ];
    let mut failures = Vec::new();
    for marked in cases {
        let labels = offered(Language::HCL, marked);
        if !labels.is_empty() {
            failures.push(format!("{marked:?} offers {labels:?}"));
        }
    }
    report(&failures);
}
