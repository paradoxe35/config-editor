//! The schemas an object's keys are read by: its `propertyNames`, walked
//! as a value's schemas are (`$ref`, `allOf`, `anyOf`, `oneOf`), so the
//! keyword and the `enum`/`examples` there name keys as they name values.

use serde_json::Value;

use crate::editor::applies::Here;

/// Every schema a key of the object `object` (the schemas applying to
/// it) is read by: each branch of each of their `propertyNames`, once, in
/// order.
pub(crate) fn key_branches<'s>(object: &[&'s Value], root: &'s Value) -> Vec<&'s Value> {
    let mut branches: Vec<&'s Value> = Vec::new();
    for names in object
        .iter()
        .filter_map(|schema| schema.get("propertyNames"))
    {
        for branch in Here::of(names, root).branches() {
            if !branches.iter().any(|seen| std::ptr::eq(*seen, branch)) {
                branches.push(branch);
            }
        }
    }
    branches
}
