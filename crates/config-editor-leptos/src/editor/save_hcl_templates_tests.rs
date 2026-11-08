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
                let saved = bridge.to_host(&edited, &host_text).unwrap().text;
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
                let saved = bridge.to_host(&edited, &host_text).unwrap().text;
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
        let saved = bridge.to_host(&edited, host_text).unwrap().text;
        assert_eq!(
            saved,
            "msg = <<EOT\nhello! ${var.x}\nEOT\nn = 1\n",
            "via {}",
            view.label()
        );
    }
}
