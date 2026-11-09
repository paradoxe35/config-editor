//! HCL expressions (calls holding templates, `for` expressions, objects
//! with templates in them) edited from every other view, where they read
//! as their `${…}` text: they stay expressions, only the edited text
//! inside changes, and a save that would write one as a literal string
//! (or a literal as an expression) waits.

use super::Language;
use super::save_matrix::{ALL, from_every_view, report, save_value};
use serde_json::Value;

const LOCALS: &str = r#"locals {
  a = merge(local.tags, { Name = "frontend" })
  b = var.enabled ? "up" : "down"
  c = file("${path.module}/init.sh")
  d = lower("ABC")
  e = [for s in var.list : upper(s)]
  f = "${var.x}-y"
  g = aws_instance.web[*].id
  h = "plain"
  i = "say \"hi\""
  j = { for k, v in var.m : k => "${v}-x" }
  k = merge(local.common_tags, { Name = "${local.name_prefix}-web-${count.index}" })
  l = "${var.a}" == "x" ? "${var.b}/y" : "z"
}
n = 1 # n
"#;

/// `word` replaced by `with` in the string at `pointer`.
fn replace(pointer: &'static str, word: &'static str, with: &'static str) -> impl Fn(&mut Value) {
    move |document| {
        let slot = document.pointer_mut(pointer).expect("the value");
        let text = slot.as_str().expect("a string").to_string();
        assert!(text.contains(word), "{pointer} holds {word:?}: {text:?}");
        *slot = text.replacen(word, with, 1).into();
    }
}

#[test]
fn an_expression_edited_in_a_view_stays_an_expression() {
    let mut failures = Vec::new();
    for (key, word, with) in [
        ("a", "frontend", "backend"),
        ("b", "down", "low"),
        ("c", "init.sh", "boot.sh"),
        ("c", "path.module", "path.root"),
        ("d", "ABC", "XYZ"),
        ("e", "upper", "lower"),
        ("e", "var.list", "var.items"),
        ("f", "-y", "-z"),
        ("g", "[*].id", "[*].arn"),
        ("h", "plain", "plain2"),
        ("i", "say", "tell"),
        ("j", "-x", "-y"),
        ("k", "-web-", "-app-"),
        ("l", "/y", "/w"),
    ] {
        let pointer: &'static str = Box::leak(format!("/locals/{key}").into_boxed_str());
        let line = LOCALS
            .lines()
            .find(|line| line.starts_with(&format!("  {key} = ")))
            .unwrap();
        from_every_view(
            &mut failures,
            &format!("{key}: {word} -> {with}"),
            Language::HCL,
            LOCALS,
            &replace(pointer, word, with),
            &LOCALS.replacen(line, &line.replacen(word, with, 1), 1),
        );
    }
    report(failures);
}

/// An expression edited into text that is no expression (a call left
/// unclosed) would be written as a literal string: the save waits, as it
/// does for a literal edited so it would be written as an expression.
#[test]
fn a_save_that_would_turn_an_expression_into_a_literal_waits() {
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::HCL) {
        for (pointer, word, with) in [
            ("/locals/c", "/init.sh\")", "/init.sh\""),
            ("/locals/e", "upper(s)]", "upper(s)"),
            ("/locals/k", "-web-", "-${web"),
        ] {
            match save_value(Language::HCL, edit, LOCALS, &replace(pointer, word, with)) {
                Err(error) if error.contains(pointer) => {}
                other => failures.push(format!("{pointer} via {edit}: {other:?}")),
            }
        }
    }
    report(failures);
}
