//! Children added from another view under an XML start tag written over
//! several lines take the indent of the tag's first line and one unit
//! more, as do their own children and the end tag: the tag's other lines
//! (its attributes, aligned however the host likes) say nothing of the
//! element's depth (QA `E31-2`: under a tab-indented `<bean` over lines, a
//! child got the attribute line's indent and more, and `</bean>` two tabs).

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::json;

const VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// The host, and the saved text once `bean` holds `c: {k: v}` last.
fn case(unit: &str, over_lines: bool, self_closing: bool) -> (String, String) {
    let u = unit;
    // The attribute lines deeper than the children, as aligned tags often
    // are.
    let tag = if over_lines {
        format!("{u}<bean\n{u}{u}{u}id=\"a\"\n{u}{u}{u}class=\"b\"")
    } else {
        format!("{u}<bean id=\"a\" class=\"b\"")
    };
    let added = format!("{u}{u}<c>\n{u}{u}{u}<k>v</k>\n{u}{u}</c>\n");
    if self_closing {
        (
            format!("<root>\n{tag}/>\n</root>\n"),
            format!("<root>\n{tag}>\n{added}{u}</bean>\n</root>\n"),
        )
    } else {
        let body = format!("{u}{u}<x>1</x>\n");
        (
            format!("<root>\n{tag}>\n{body}{u}</bean>\n</root>\n"),
            format!("<root>\n{tag}>\n{body}{added}{u}</bean>\n</root>\n"),
        )
    }
}

#[test]
fn children_added_under_a_tag_over_lines_indent_from_its_first_line() {
    let mut failures = Vec::new();
    for unit in ["\t", "  ", "    "] {
        for over_lines in [false, true] {
            for self_closing in [false, true] {
                let (host, expected) = case(unit, over_lines, self_closing);
                for edit in VIEWS {
                    let saved = save_value(Language::XML, edit, &host, &|value| {
                        value["root"]["bean"]
                            .as_object_mut()
                            .expect("bean")
                            .insert("c".into(), json!({"k": "v"}));
                    });
                    if saved.as_deref() != Ok(expected.as_str()) {
                        failures.push(format!(
                            "unit {unit:?}, over lines {over_lines}, self-closing {self_closing}, via {edit}:\n{saved:?}\nwanted\n{expected:?}"
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}
