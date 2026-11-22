//! A map a TOML view's dotted key made (`n.ode`), saved, then left alone
//! while an undo goes back past that save and a line elsewhere is
//! deleted: the save, made over the host text the view's links name,
//! keeps the form the later save wrote the map in. Over hosts that write
//! maps on lines and on one line (an HCL view has no dotted keys).

use super::ops::{END, Op};
use super::{ALL, replay};
use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;

/// The index of the first line of `text` holding `word`, and the column
/// just past that word's first character.
fn at(text: &str, word: &str) -> (usize, usize) {
    text.split('\n')
        .enumerate()
        .find_map(|(line, written)| {
            written
                .find(word)
                .map(|column| (line, written[..column].chars().count() + 1))
        })
        .unwrap_or_else(|| panic!("no `{word}` in:\n{text}"))
}

#[test]
fn a_dotted_key_map_left_alone_keeps_its_host_form_across_undos() {
    let hosts = [
        (
            Language::JSON,
            "{\n  \"name\": \"web-app\",\n  \"dependencies\": {\"react\": \"^18.3.1\", \"vite\": \"^5\"},\n  \"engines\": {\n    \"node\": \">=18\"\n  },\n  \"size\": 1e3\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"name\": \"web-app\",\n  \"dependencies\": {\"react\": \"^18.3.1\", \"vite\": \"^5\"},\n  \"engines\": {\"node\": \">=18\"},\n  \"size\": 1e3\n}\n",
        ),
        (
            Language::YAML,
            "name: web-app\ndependencies:\n  react: ^18.3.1\n  vite: ^5\nengines:\n  node: '>=18'\nsize: 1e3\n",
        ),
        (
            Language::YAML,
            "name: web-app\ndependencies:\n  react: ^18.3.1\n  vite: ^5\nengines: {node: '>=18'}\nsize: 1e3\n",
        ),
    ];
    let view = Language::TOML;
    for (host, text) in hosts {
        let shown = Bridge::new(host, view, None)
            .and_then(|bridge| bridge.to_edit(text).ok())
            .map(|conversion| conversion.text)
            .unwrap_or_else(|| panic!("{} shows no TOML view", host.label()));
        let (node, column) = at(&shown, "node");
        let (react, _) = at(&shown, "react");
        let to_host = ALL
            .iter()
            .position(|language| *language == host)
            .unwrap_or_default();
        replay(
            host,
            view,
            text,
            &[
                Op::Type {
                    line: node,
                    column,
                    text: ".",
                },
                Op::MoveLine {
                    line: 0,
                    down: true,
                },
                Op::Switch { to: to_host },
                Op::Enter {
                    line: 0,
                    column: END,
                },
                Op::Undo,
                Op::Undo,
                Op::Undo,
                Op::DeleteLine { line: react },
            ],
        );
    }
}
