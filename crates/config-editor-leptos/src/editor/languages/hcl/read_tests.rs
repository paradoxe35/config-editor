//! The reading of `super::read` is `hcl-rs`'s, in time growing with the
//! length of the text: the same data (or the same error) for every form of
//! body and value, objects of many entries parsed in groups included.

use super::HclBehavior;
use super::read::read;
use crate::editor::language::ConfigLanguage;
use crate::editor::test_timing::best_of;

/// `text` read as `hcl-rs` reads it, the heredocs and floats as the
/// editor reads them.
fn as_hcl_rs_reads(text: &str) -> Result<serde_json::Value, String> {
    super::heredoc::parse_body(text)
        .and_then(hcl::from_body::<serde_json::Value>)
        .map_err(|error| error.to_string())
}

/// Bodies of every form: attributes, blocks with and without labels,
/// repeated ones, values of every kind.
const BODIES: &[&str] = &[
    "",
    "a = 1\nb = \"x\"\nc = true\nd = null\ne = 1.5\nf = 1.0\ng = -3\nh = 1e3\n",
    "a = [1, \"two\", [3], { x = 4 }]\n",
    "o = { a = 1, \"b c\" = 2, d = { e = [1] } }\n",
    "o = {\n  a = 1\n  b = 2 # c\n  // d\n  e = 3,\n}\n",
    "b {\n  x = 1\n}\n",
    "b {\n  x = 1\n}\nb {\n  x = 2\n}\n",
    "r \"t\" \"n\" {\n  x = 1\n}\nr \"t\" \"m\" {\n  y = 2\n}\nr \"u\" \"n\" {}\n",
    "r \"t\" \"n\" {\n  x = 1\n}\nr \"t\" \"n\" {\n  x = 2\n}\n",
    "a = 1\na2 = 2\nb \"l\" {\n  inner {\n    deep = var.x\n  }\n  inner {\n    deep = 2\n  }\n}\n",
    "t = \"${var.a}-b\"\nu = \"$${not}\"\nf = upper(\"x\")\nc = var.a ? 1 : 2\nop = 1 + 2\nn = -var.x\nfr = [for s in var.l : upper(s)]\nfo = { for k, v in var.m : k => v }\np = (1)\nidx = var.l[0]\nsplat = var.l[*].id\n",
    "h = <<EOT\nline\n  two\nEOT\nhi = <<-EOT\n    in\n  EOT\nht = <<EOT\n${var.x}\nEOT\n",
    "k = { (var.x) = 1, \"a\" = 2, a = 3 }\n",
    "w = 18446744073709551616\nbig = 1e400\n",
    "x = \"\\u00e9\\n\\t\"\n",
    "a = 1\na = 2\n",
    "o = { a = 1, a = 2 }\n",
    "x = \n",
    "b {\n",
    "a = { b = }\n",
    "a = 1\nb {\n  a = 1\n}\n",
];

/// An object attribute of `entries` entries, each written as `entry` writes
/// entry `index`.
fn big(entries: usize, entry: impl Fn(usize) -> String) -> String {
    let mut text = String::from("locals {\n  hosts = {\n");
    for index in 0..entries {
        text.push_str(&entry(index));
    }
    text.push_str("  }\n}\n");
    text
}

/// Objects of many entries written in every way entries are: one a line,
/// several a line, with comments, blank lines, values over lines, nested
/// objects of many entries, and keys written twice.
fn big_objects() -> Vec<String> {
    vec![
        big(1000, |index| format!("    web_{index} = \"10.0.{}.{}\"\n", index >> 8, index & 255)),
        big(500, |index| format!("    k{index} = {index}, \"q{index}\" = [{index}]\n")),
        big(400, |index| format!("    # c{index}\n\n    k{index} = {{ a = {index} }} // t\n")),
        big(300, |index| format!("    k{index} = [\n      {index},\n      \"x\",\n    ]\n    h{index} = <<EOT\n  {{ not = an entry }}\nEOT\n")),
        big(300, |index| format!("    k{index} = var.x[{index}]\n    t{index} = \"${{var.y}}-{index}\"\n    f{index} = {{ for k, v in var.m : k => v }}\n")),
        big(200, |index| format!("    n{index} = {{\n{}    }}\n", (0..200).map(|inner| format!("      i{inner} = {inner}\n")).collect::<String>())),
        big(300, |index| format!("    k{} = {index}\n", index % 250)),
        big(300, |index| if index == 250 { "    k3 = 1\n".to_string() } else { format!("    k{index} = {index}\n") }),
        big(300, |index| if index == 250 { "    \"k3\" = 1\n".to_string() } else { format!("    k{index} = {index}\n") }),
        big(300, |index| if index == 299 { "    broken = \n".to_string() } else { format!("    k{index} = {index}\n") }),
        big(300, |index| if index == 150 { "    k = 1 +\n      2\n".to_string() } else { format!("    k{index} = {index}\n") }),
        format!("v = [{}]\n", (0..300).map(|index| format!("{{ k{index} = {index} }}")).collect::<Vec<_>>().join(", ")),
        format!("v = f({{\n{}}})\n", entries(300, "=")),
        format!("v = var.c ? {{\n{}}} : {{}}\n", entries(300, "=")),
        format!("v = {{\n{}}}[\"k1\"]\n", entries(300, "=")),
        format!("v = {{ x = var.c ? 1 : {{\n{}}} }}\n", entries(300, "=")),
        format!("v = merge(local.a, {{\n{}}})\n", entries(300, ":")),
        format!("v = [\n  {{\n{}}},\n  {{\n{}}}\n]\n", entries(300, "="), entries(300, ":")),
        format!("b \"l\" {{\n  v = {{\n{}  }}\n}}\nb \"l\" {{\n  v = {{\n{}  }}\n}}\n", entries(300, "="), entries(200, "=")),
    ]
}

/// `count` entries `k0 = 0` (or `k0: 0`), one a line.
fn entries(count: usize, separator: &str) -> String {
    (0..count).map(|index| format!("  k{index} {separator} {index}\n")).collect()
}

#[test]
fn every_body_reads_as_hcl_rs_reads_it() {
    let mut failures = Vec::new();
    let sample = include_str!("../../../../tests/fixtures/samples/tunnel.large.hcl");
    let texts = BODIES.iter().map(|text| text.to_string()).chain(big_objects()).chain([sample.to_string()]);
    for text in texts {
        let ours = read(&text, false).map_err(|error| error.to_string());
        if ours != as_hcl_rs_reads(&text) {
            failures.push(text.chars().take(200).collect::<String>());
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The values written as expressions, as found by marking them in the
/// parsed body and reading it with `hcl-rs`.
fn expressions_as_hcl_rs_reads(text: &str) -> Vec<String> {
    use hcl_edit::expr::Expression;
    fn mark_body(body: &mut hcl_edit::structure::Body) {
        for mut structure in body.iter_mut() {
            if let Some(mut attribute) = structure.as_attribute_mut() {
                mark(attribute.value_mut());
            } else if let Some(block) = structure.as_block_mut() {
                mark_body(&mut block.body);
            }
        }
    }
    fn mark(expression: &mut Expression) {
        let literal = |template: &hcl_edit::template::Template| template.iter().all(|element| matches!(element, hcl_edit::template::Element::Literal(_)));
        let is_expression = match expression {
            Expression::Null(_) | Expression::Bool(_) | Expression::Number(_) | Expression::String(_) => false,
            Expression::StringTemplate(template) => !literal(template),
            Expression::HeredocTemplate(heredoc) => !literal(&heredoc.template),
            Expression::Array(items) => return items.iter_mut().for_each(mark),
            Expression::Object(object) => return object.iter_mut().for_each(|(_, value)| mark(value.expr_mut())),
            _ => true,
        };
        if is_expression {
            *expression = Expression::String(hcl_edit::Decorated::new(super::read::MARK.to_string()));
        }
    }
    let Ok(mut body) = text.parse::<hcl_edit::structure::Body>() else {
        return Vec::new();
    };
    mark_body(&mut body);
    let Ok(value) = hcl::from_body::<serde_json::Value>(body.into()) else {
        return Vec::new();
    };
    crate::editor::paths::fields(&value)
        .into_iter()
        .filter(|(pointer, _)| value.pointer(pointer).and_then(serde_json::Value::as_str) == Some(super::read::MARK))
        .map(|(pointer, _)| pointer)
        .collect()
}

#[test]
fn the_expressions_found_are_the_ones_hcl_rs_reads() {
    let mut failures = Vec::new();
    for text in BODIES.iter().map(|text| text.to_string()).chain(big_objects()) {
        let mut ours = HclBehavior.expressions(&text);
        let mut theirs = expressions_as_hcl_rs_reads(&text);
        ours.sort();
        theirs.sort();
        if ours != theirs {
            failures.push(format!("{}: {ours:?} / {theirs:?}", text.chars().take(120).collect::<String>()));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// What an object parsed in groups reads is what it reads parsed whole.
#[test]
fn an_object_parsed_in_groups_reads_as_one_parsed_whole() {
    for text in big_objects() {
        for marks in [false, true] {
            assert_eq!(
                super::read::read_with(&text, marks, true).map_err(|error| error.to_string()),
                super::read::read_with(&text, marks, false).map_err(|error| error.to_string()),
                "{}",
                &text[..200.min(text.len())]
            );
        }
    }
}

/// No object handed to the parser holds more than a few groups of entries.
#[test]
fn the_parser_never_gets_an_object_of_many_entries() {
    fn largest(body: &hcl_edit::structure::Body) -> usize {
        fn of(expression: &hcl_edit::expr::Expression) -> usize {
            match expression {
                hcl_edit::expr::Expression::Object(object) => object.iter().map(|(_, value)| of(value.expr())).max().unwrap_or(0).max(object.len()),
                hcl_edit::expr::Expression::Array(items) => items.iter().map(of).max().unwrap_or(0),
                _ => 0,
            }
        }
        body.iter()
            .map(|structure| match (structure.as_attribute(), structure.as_block()) {
                (Some(attribute), _) => of(&attribute.value),
                (_, Some(block)) => largest(&block.body),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }
    let text = big(5000, |index| format!("    k{index} = {{ a = {index} }}\n"));
    let parsed = super::big_objects::parse(&text, true).expect("it parses");
    let mut stack = vec![&parsed];
    let mut most = 0;
    while let Some(parsed) = stack.pop() {
        most = most.max(largest(&parsed.body));
        stack.extend(parsed.all_groups());
    }
    assert!(most <= 64, "an object of {most} entries parsed at once");
}

/// A document shape: its name, and its text of so many entries.
type Shape = (&'static str, fn(usize) -> String);

/// Reading a document of `size` entries of each shape, and its whole
/// pause's reading (the data and the expressions), takes time in step with
/// its length: eight times the entries cost about eight times as long,
/// not sixty-four.
#[test]
fn reading_grows_with_the_length_of_the_text() {
    let shapes: [Shape; 4] = [
        ("tfvars", |entries| (0..entries).map(|index| format!("instance_{index}_name = \"web-{index}.example.com\"\n")).collect()),
        ("locals", |entries| big(entries, |index| format!("    web_{index} = \"10.0.{}.{}\"\n", index >> 8, index & 255))),
        ("blocks", |entries| (0..entries / 3).map(|index| format!("svc_{index} {{\n  host = \"h{index}\"\n}}\n")).collect()),
        ("vars", |entries| format!("vars = {{\n{}}}\n", (0..entries).map(|index| format!("  k_{index} = \"v{index}\"\n")).collect::<String>())),
    ];
    let mut failures = Vec::new();
    for (name, shape) in shapes {
        let time = |entries: usize| {
            let text = shape(entries);
            best_of(3, || {
                let value = HclBehavior.parse(&text).expect("it parses");
                (value, HclBehavior.expressions(&text))
            })
            .0
        };
        let small = time(2_000);
        let large = time(16_000);
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
        if ratio > 20.0 {
            failures.push(format!("{name}: 2000 entries {small:?}, 16000 entries {large:?} ({ratio:.1}x)"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
