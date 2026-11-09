//! A save's check that every expression stays an expression (and every
//! literal a literal) looks each one up once, however many the document
//! writes: a Terraform file of thousands of references paid for each one
//! a pass over all the others.

use super::expressions::changed_expression;
use crate::editor::test_timing::best_of;

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
    let cost = |services: usize| {
        let (previous, patched) = (document(services, "web"), document(services, "web7"));
        let (took, changed) = best_of(3, || changed_expression(&previous, &patched));
        assert_eq!(changed, None, "{services} services");
        took.as_secs_f64()
    };
    let (few, many) = (cost(1_000), cost(8_000));
    // Eight times the document: about eight times the cost (sixty-four when
    // each expression is looked for among all the others).
    assert!(many < 14.0 * few, "1,000 services {few:.4}s, 8,000 {many:.4}s");
}
