//! A save's check that every expression stays an expression (and every
//! literal a literal) looks each one up once, however many the document
//! writes: a Terraform file of thousands of references paid for each one
//! a pass over all the others.

use super::expressions::changed_expression;
use crate::editor::work_count::{Work, counted};

/// `services` blocks, each writing two references and a literal.
fn document(services: usize, literal: &str) -> String {
    (0..services)
        .map(|index| {
            let name = if index == services / 2 { literal.to_string() } else { format!("web-{index}") };
            format!("svc_{index} {{\n  ami = var.ami_{index}\n  subnet = local.subnets[{index}]\n  name = \"{name}\"\n}}\n")
        })
        .collect()
}

#[test]
fn checking_a_save_for_changed_expressions_grows_with_the_document_not_its_square() {
    let lookups = |services: usize| {
        let (previous, patched) = (document(services, "web"), document(services, "web7"));
        let (changed, lookups) = counted(Work::ExpressionLookup, || changed_expression(&previous, &patched));
        assert_eq!(changed, None, "{services} services");
        lookups
    };
    let (few, many) = (lookups(1_000), lookups(8_000));
    // Eight times the document: eight times the lookups (each expression
    // looked for among all the others: sixty-four).
    assert!(few > 0, "the check looks expressions up");
    assert!(many <= 8 * few, "1,000 services {few} lookups, 8,000 {many}");
}
