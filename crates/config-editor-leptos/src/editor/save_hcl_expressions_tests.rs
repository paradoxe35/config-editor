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

/// A Terraform module: a `jsonencode({ … })` policy over eight lines, a
/// heredoc, `for` expressions.
const MODULE: &str = include_str!("../../tests/fixtures/real/module.tf");

/// An edit inside an expression written over several lines (a
/// `jsonencode({ … })` policy) changes only that line: the view shows
/// the expression on one line, and the host keeps its own layout.
#[test]
fn an_edit_inside_a_multi_line_expression_changes_only_its_line() {
    let mut failures = Vec::new();
    for (word, with) in [
        ("s3:GetObject", "s3:GetObjectX"),
        ("2012-10-17", "2012-10-18"),
        ("\"*\"", "\"arn:x\""),
        ("[\"s3:GetObject\"]", "[\"s3:GetObject\", \"s3:PutObject\"]"),
    ] {
        from_every_view(
            &mut failures,
            &format!("policy: {word} -> {with}"),
            Language::HCL,
            MODULE,
            &replace("/output/policy/value", word, with),
            &MODULE.replacen(word, with, 1),
        );
    }
    report(failures);
}

/// An expression that is an entry of an object (`m = { k2 = merge(…) }`)
/// edited in a view is saved as one at the top is, beside entries of
/// every form (literals, templates, references, a heredoc), in small and
/// large objects; a save that waits names the edited entry.
#[test]
fn an_expression_in_an_object_entry_is_saved_like_one_at_the_top() {
    let object = |count: usize, heredoc: bool| -> String {
        let entries: Vec<String> = (0..count)
            .map(|index| match index % 4 {
                _ if heredoc && index == 5 => {
                    "    k5 = <<-EOT\n      multi ${var.q}\n      line\n    EOT".to_string()
                }
                0 => format!("    k{index} = var.a{index}"),
                1 => format!("    k{index} = \"${{var.x}}-{index}\""),
                2 => {
                    format!("    k{index} = merge(local.t, {{ Name = \"${{local.n}}-{index}\" }})")
                }
                _ => format!("    k{index} = \"lit{index}\""),
            })
            .collect();
        format!(
            "locals {{\n  m = {{\n{}\n  }}\n  after = 1\n}}\n",
            entries.join("\n")
        )
    };
    let mut failures = Vec::new();
    for (count, heredoc) in [(8, false), (20, true), (200, false), (200, true)] {
        let host = object(count, heredoc);
        for (key, word, with) in [
            ("k2", "-2\"", "-2X\""),
            ("k6", "local.t", "local.u"),
            ("k3", "lit3", "lit3X"),
            ("k1", "-1", "-1X"),
            ("k4", "var.a4", "var.b4"),
        ] {
            let pointer: &'static str = Box::leak(format!("/locals/m/{key}").into_boxed_str());
            let line = host
                .lines()
                .find(|line| line.starts_with(&format!("    {key} = ")))
                .unwrap();
            from_every_view(
                &mut failures,
                &format!("{count} entries, heredoc {heredoc}: {key}: {word} -> {with}"),
                Language::HCL,
                &host,
                &replace(pointer, word, with),
                &host.replacen(line, &line.replacen(word, with, 1), 1),
            );
        }
        for edit in ALL.into_iter().filter(|edit| *edit != Language::HCL) {
            match save_value(
                Language::HCL,
                edit,
                &host,
                &replace("/locals/m/k2", "-2\" })", "-2\""),
            ) {
                Err(error) if error.contains("/locals/m/k2") => {}
                other => failures.push(format!(
                    "{count} entries, k2 unclosed via {edit}: {other:?}"
                )),
            }
        }
    }
    // In an object in a list, beside another entry on its line.
    let host = "locals {\n  l = [\n    { k = merge(local.t, { Name = \"${local.n}-1\" }), j = 1 },\n    { k = \"x\", j = 2 },\n  ]\n}\n";
    from_every_view(
        &mut failures,
        "an object in a list",
        Language::HCL,
        host,
        &replace("/locals/l/0/k", "-1", "-1X"),
        &host.replacen("-1", "-1X", 1),
    );
    report(failures);
}

/// An expression moved from one block to another in a view (cut and
/// pasted, or moved by lines) stays an expression there, as one moved
/// within its block does: a reference, an interpolated string, a call, a
/// heredoc holding an interpolation (round 29 wrote each as the escaped
/// literal `"$${…}"`, the save reported done).
#[test]
fn an_expression_moved_to_another_block_stays_an_expression() {
    let mut failures = Vec::new();
    for written in [
        "var.z",
        "\"x-${var.y}\"",
        "lower(var.q)",
        "merge(local.t, { Name = \"${local.n}\" })",
        "<<-EOT\n    echo ${var.y}\n  EOT",
    ] {
        let host = format!("locals {{\n  n = 1\n  r = {written}\n}}\n\nother {{\n  k = 1\n}}\n");
        let expected =
            format!("locals {{\n  n = 1\n}}\n\nother {{\n  k = 1\n  r = {written}\n}}\n");
        for edit in ALL.into_iter().filter(|edit| *edit != Language::HCL) {
            let moved = |value: &mut Value| {
                let taken = value["locals"]
                    .as_object_mut()
                    .and_then(|locals| locals.shift_remove("r"))
                    .expect("r");
                value["other"]["r"] = taken;
            };
            match save_value(Language::HCL, edit, &host, &moved) {
                Ok(saved) if saved == expected => {}
                other => failures.push(format!("{written} via {edit}: {other:?}")),
            }
        }
    }
    report(failures);
}
