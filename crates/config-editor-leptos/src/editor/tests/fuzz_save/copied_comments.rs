//! A list item copied in an Edit-as view and pasted before the next item
//! takes its own comment along, and leaves the next item's comment above
//! the next item only: no comment is written twice, whichever host writes
//! comments, from every view a copy of an entry's lines makes, the copy
//! left as it is or one of its values typed over.

use super::ops::{END, Op, Put, Take};
use super::saved_after;
use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;

/// Each host's list of three items, each with a comment of its own above
/// it (`web`, `logging`, `tests`).
const HOSTS: &[(Language, &str)] = &[
    (
        Language::YAML,
        "deps:\n  # web\n  - name: spring-web\n    version: 6.1.2\n  # logging\n  - name: slf4j-api\n    version: 2.0.9\n  # tests\n  - name: junit\n    version: 5.10.1\n",
    ),
    (
        Language::TOML,
        "# web\n[[deps]]\nname = \"spring-web\"\nversion = \"6.1.2\"\n\n# logging\n[[deps]]\nname = \"slf4j-api\"\nversion = \"2.0.9\"\n\n# tests\n[[deps]]\nname = \"junit\"\nversion = \"5.10.1\"\n",
    ),
    (
        Language::XML,
        "<deps>\n  <!-- web -->\n  <dep>\n    <name>spring-web</name>\n    <version>6.1.2</version>\n  </dep>\n  <!-- logging -->\n  <dep>\n    <name>slf4j-api</name>\n    <version>2.0.9</version>\n  </dep>\n  <!-- tests -->\n  <dep>\n    <name>junit</name>\n    <version>5.10.1</version>\n  </dep>\n</deps>\n",
    ),
    (
        Language::HCL,
        "deps = [\n  # web\n  { name = \"spring-web\", version = \"6.1.2\" },\n  # logging\n  { name = \"slf4j-api\", version = \"2.0.9\" },\n  # tests\n  { name = \"junit\", version = \"5.10.1\" },\n]\n",
    ),
];

#[test]
fn a_copied_item_pasted_before_the_next_writes_no_comment_twice() {
    let mut failures = Vec::new();
    for (host, text) in HOSTS {
        for view in [Language::YAML, Language::JSON, Language::TOML] {
            if view == *host {
                continue;
            }
            let shown = Bridge::new(*host, view, None)
                .and_then(|bridge| bridge.to_edit(text).ok())
                .map(|shown| shown.text)
                .expect("shown");
            let lines: Vec<&str> = shown.lines().collect();
            let at = |piece: &str| lines.iter().position(|line| line.contains(piece));
            let (Some(copied), Some(next)) = (at("slf4j-api"), at("junit")) else {
                panic!("{host} as {view}:\n{shown}");
            };
            // An item's first line: a YAML item's own, else the line
            // opening it above its name.
            let back = usize::from(view != Language::YAML);
            let (copied, next) = (copied - back, next - back);
            for typed in [false, true] {
                let mut ops = vec![
                    Op::Copy {
                        line: copied,
                        how: Take::Entry,
                    },
                    Op::Paste {
                        line: next,
                        how: Put::Before,
                    },
                ];
                if typed {
                    ops.push(Op::Type {
                        line: next + back,
                        column: END + 1,
                        text: "x",
                    });
                }
                let saved = saved_after(*host, view, text, &ops);
                let count = |word: &str| saved.matches(word).count();
                if count("tests") != 1 || count("web") != 2 || count("logging") > 2 {
                    failures.push(format!(
                        "{host} host via {view}, typed over {typed}:\n{saved}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
