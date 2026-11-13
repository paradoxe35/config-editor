//! HCL template escapes (`$${`, `%%{`) in quoted strings and heredocs
//! (`<<EOT`, `<<-EOT`): read as the text they stand for, shown so in every
//! view and conversion, and written back escaped in the string's own form.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use serde_json::{Value, json};

/// The views an HCL host is edited in.
const VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::XML,
];

/// Texts holding `${` and `%{` at the start, in the middle, at the end, and
/// repeated.
const TEXTS: [&str; 5] = [
    "${a} x",
    "x ${a} y",
    "x %{b}",
    "${a}${a} %{b}%{b}",
    "x $${a}",
];

/// `text` escaped as HCL writes it in a template.
fn escaped(text: &str) -> String {
    text.replace("${", "$${").replace("%{", "%%{")
}

/// A string form: how it writes `raw` (template text, escapes and all) as
/// `msg`, and whether its text ends with a line break.
struct Form {
    name: &'static str,
    write: fn(&str) -> String,
    line_break: bool,
}

const FORMS: [Form; 3] = [
    Form {
        name: "quoted",
        write: |raw| format!("msg = \"{raw}\"\n"),
        line_break: false,
    },
    Form {
        name: "heredoc",
        write: |raw| format!("msg = <<EOT\n{raw}\nEOT\n"),
        line_break: true,
    },
    Form {
        name: "indented heredoc",
        write: |raw| format!("msg = <<-EOT\n  {raw}\n  EOT\n"),
        line_break: true,
    },
];

fn host(form: &Form, raw: &str, n: u32) -> String {
    format!("# c\n{}n = {n} # n\n", (form.write)(raw))
}

/// `value` with `msg` set, shown in `view` as the view writes it.
fn view_with(view: Language, shown: &str, change: impl Fn(&mut Value)) -> String {
    let mut value = codec::read(view.behavior(), shown, None).unwrap();
    change(&mut value);
    codec::serialize(view.behavior(), &value, Some(shown)).unwrap()
}

#[test]
fn template_escapes_read_as_text_and_write_back_in_the_strings_form() {
    let mut failures = Vec::new();
    for form in &FORMS {
        for text in TEXTS {
            let read = if form.line_break {
                format!("{text}\n")
            } else {
                text.to_string()
            };
            let host_text = host(form, &escaped(text), 1);
            let case = format!("{} {text:?}", form.name);
            match Language::HCL.parse(&host_text) {
                Ok(value) if value["msg"] == json!(read) => {}
                other => failures.push(format!("{case}: parses as {other:?}")),
            }
            for view in VIEWS {
                let case = format!("{case} via {}", view.label());
                let bridge = Bridge::new(Language::HCL, view, None).unwrap();
                let shown = bridge.to_edit(&host_text).unwrap();
                let seen = codec::read(view.behavior(), &shown.text, None).unwrap();
                if seen["msg"] != json!(read) {
                    failures.push(format!("{case}: shows {}", seen["msg"]));
                }
                // Another value edited: the string is written as it was.
                let edited = view_with(view, &shown.text, |value| value["n"] = json!(2));
                let saved = bridge.to_host(&edited, &host_text).unwrap();
                if saved != host(form, &escaped(text), 2) {
                    failures.push(format!("{case}, n edited: saved\n{saved}"));
                }
                // The string edited to text holding more sequences: escaped,
                // in its own form.
                let new_text = format!("${{z}} {text} %{{w}}");
                let new_read = if form.line_break {
                    format!("{new_text}\n")
                } else {
                    new_text.clone()
                };
                let edited = view_with(view, &shown.text, |value| value["msg"] = json!(new_read));
                let saved = bridge.to_host(&edited, &host_text).unwrap();
                if saved != host(form, &escaped(&new_text), 1) {
                    failures.push(format!("{case}, msg edited: saved\n{saved}"));
                }
                // Converting gives the text too.
                let converted = codec::convert(
                    &host_text,
                    Language::HCL.behavior(),
                    view.behavior(),
                    None,
                    None,
                )
                .unwrap();
                let back = codec::read(view.behavior(), &converted.text, None).unwrap();
                if back["msg"] != json!(read) || !converted.losses.is_empty() {
                    failures.push(format!("{case}: converts to\n{}", converted.text));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A heredoc holding a live interpolation is a template: its text stays its
/// own when edited (its `${` is not escaped), as a quoted template's does.
#[test]
fn a_heredoc_template_keeps_its_interpolation_when_edited() {
    let host_text = "msg = <<EOT\nhello ${var.x}\nEOT\nn = 1\n";
    for view in VIEWS {
        let bridge = Bridge::new(Language::HCL, view, None).unwrap();
        let shown = bridge.to_edit(host_text).unwrap();
        let edited = view_with(view, &shown.text, |value| {
            value["msg"] = json!("hello! ${var.x}\n")
        });
        let saved = bridge.to_host(&edited, host_text).unwrap();
        assert_eq!(
            saved,
            "msg = <<EOT\nhello! ${var.x}\nEOT\nn = 1\n",
            "via {}",
            view.label()
        );
    }
}

/// M25-E2: a document's expressions (templates `"${var.x}-y"`, heredocs
/// holding `${…}`, references `var.z`, in lists and blocks too) are read
/// as their text, as an escaped literal `"$${only}"` is: converting lists
/// each one as a loss, so a round trip that writes it back as text does not
/// change its meaning silently. Literals are no loss.
#[test]
fn a_conversion_lists_the_documents_expressions_as_losses() {
    let source = "a = \"${var.x}-y\"\nb = <<EOT\nhi ${var.n}\n$${lit}\nEOT\nc = var.z\nd = \"$${only}\"\ne = \"plain\"\nl = [var.p, \"q\", \"${var.r}\"]\nf = upper(\"x\")\nsvc \"web\" {\n  port = var.port\n  name = \"$${n}\"\n}\n";
    let expected = ["/a", "/b", "/c", "/l/0", "/l/2", "/f", "/svc/web/port"];
    let mut failures = Vec::new();
    for target in super::save_matrix::ALL {
        match Language::HCL.convert(source, target) {
            Ok(conversion) => {
                let mut losses = conversion.losses.clone();
                losses.sort();
                let mut wanted = expected.map(str::to_string).to_vec();
                wanted.sort();
                if losses != wanted {
                    failures.push(format!("to {target}: losses {losses:?}"));
                }
            }
            Err(error) => failures.push(format!("to {target}: {error}")),
        }
    }
    // A document with no expressions converts with none.
    let literal = "d = \"$${only}\"\nh = <<EOT\n$${lit}\nEOT\n";
    for target in super::save_matrix::ALL {
        match Language::HCL.convert(literal, target) {
            Ok(conversion) if conversion.losses.is_empty() => {}
            other => failures.push(format!("literals to {target}: {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Through Edit as, each string keeps its meaning however the view shows
/// it: a template stays one, a reference stays one, an escaped literal
/// stays escaped, and `${` typed into a literal is text.
#[test]
fn edits_keep_templates_references_and_literals_apart_from_every_view() {
    let host_text = "a = \"${var.x}-y\"\nc = var.z\nd = \"$${only}\"\ne = \"plain\"\n";
    let cases: [(&str, &str, &str, &str); 4] = [
        (
            "/a",
            "${var.x}-z",
            "a = \"${var.x}-y\"",
            "a = \"${var.x}-z\"",
        ),
        ("/c", "${var.w}", "c = var.z", "c = var.w"),
        ("/d", "${only2}", "d = \"$${only}\"", "d = \"$${only2}\""),
        ("/e", "plain${q}", "e = \"plain\"", "e = \"plain$${q}\""),
    ];
    let mut failures = Vec::new();
    for (pointer, text, before, after) in cases {
        for view in VIEWS {
            let bridge = Bridge::new(Language::HCL, view, None).unwrap();
            let shown = bridge.to_edit(host_text).unwrap();
            let edited = view_with(view, &shown.text, |value| {
                *value.pointer_mut(pointer).unwrap() = json!(text)
            });
            match bridge.to_host(&edited, host_text) {
                Ok(saved) if saved == host_text.replace(before, after) => {}
                other => failures.push(format!("{pointer} via {}: {other:?}", view.label())),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
