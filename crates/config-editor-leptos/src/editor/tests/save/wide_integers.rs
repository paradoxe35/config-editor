//! An integer past 64 bits typed in an Edit-as view: its own language
//! saves it as written, the problem still shown (QA `F30-3`), and so does
//! a view of a host that holds it as written (XML, whose numbers are
//! text): the save writes the digits (QA `E32-7`: such a number typed in
//! a YAML view of an XML host waited, and could only be saved quoted). A
//! host that refuses it (TOML) still waits.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::report;

const WIDE: &str = "123456789012345678901234";

/// The host saved after the view's `from` is typed over with `to`.
fn typed(
    host: Language,
    edit: Language,
    host_text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    let bridge = Bridge::new(host, edit, None).ok_or("same language")?;
    let shown = bridge
        .to_edit(host_text)
        .map_err(|error| error.to_string())?
        .text;
    assert!(shown.contains(from), "{edit} view of {host}: {shown}");
    let edited = shown.replacen(from, to, 1);
    bridge
        .to_host(&edited, host_text)
        .map_err(|error| format!("{error:?}"))
}

#[test]
fn an_integer_past_64_bits_saves_as_written_where_the_host_holds_it() {
    let mut failures = Vec::new();
    let host = "<r>\n  <a>1</a>\n  <l>1</l>\n  <l>2</l>\n</r>\n";
    let cases = [
        (Language::YAML, "a: 1", format!("a: {WIDE}")),
        (Language::YAML, "- 2", format!("- -{WIDE}")),
        (Language::JSON, "\"a\": 1", format!("\"a\": {WIDE}")),
        (Language::JSON, "    2\n", format!("    -{WIDE}\n")),
        (Language::HCL, "a = 1", format!("a = {WIDE}")),
    ];
    for (edit, from, to) in cases {
        let wanted = if to.contains('-') {
            host.replace("<l>2</l>", &format!("<l>-{WIDE}</l>"))
        } else {
            host.replace("<a>1</a>", &format!("<a>{WIDE}</a>"))
        };
        match typed(Language::XML, edit, host, from, &to) {
            Ok(saved) if saved == wanted => {}
            other => failures.push(format!("XML host via {edit}, {to:?}: {other:?}")),
        }
    }
    // A host that refuses it waits.
    match typed(
        Language::TOML,
        Language::YAML,
        "a = 1\n",
        "a: 1",
        &format!("a: {WIDE}"),
    ) {
        Err(error) if error.contains("Parse") => {}
        other => failures.push(format!("TOML host via YAML: {other:?}")),
    }
    report(failures);
}
