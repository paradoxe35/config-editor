//! The host shield (see the top of `src/css/base.css`): every element the
//! editor draws inside its roots starts from the browser's own styles, so
//! a page's rules for bare elements never reach it, and every rule of the
//! preset's for those elements carries `:not(#\#)` (an id's weight) to
//! outweigh the page's attribute and `:not()` rules. A rule written
//! without it would lose to the shield's reset, and the element would show
//! unstyled: this test names such rules.
//!
//! Left out on purpose: the color tokens (`:where(…)`, weightless so a
//! host can set them), the roots (`.config-editor`, `.config-editor-documents`,
//! `.config-editor-theme`) and the toolbar's and footer's own boxes, which
//! a host composes with its own elements (`ConfigEditorToolbar`,
//! `ConfigEditorFooter`).

use config_editor_leptos::PRESET_CSS;

const SHIELD: &str = r":not(#\#) ";

/// The classes of what a host composes: their rules keep a class's weight.
const HOST_CHROME: &[&str] = &[
    "config-editor",
    "config-editor-documents",
    "config-editor-theme",
    "config-editor__toolbar",
    "config-editor__file",
    "config-editor__file-name",
    "config-editor__toolbar-meta",
    "config-editor__schema-state",
    "config-editor__schema-check",
    "config-editor__language",
    "config-editor__status",
];

/// `text` without its comments.
fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("*/").expect("a closed comment") + start + 2;
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The selector lists of the style rules (at-rules' preludes left out).
fn selector_lists(css: &str) -> Vec<String> {
    let css = without_comments(css);
    let mut lists = Vec::new();
    let mut prelude = String::new();
    let mut in_rule = false;
    for character in css.chars() {
        match character {
            '{' if !in_rule => {
                let text = prelude.trim().to_string();
                if !text.starts_with('@') {
                    lists.push(text);
                    in_rule = true;
                }
                prelude.clear();
            }
            '}' if in_rule => in_rule = false,
            '}' => prelude.clear(),
            ';' if !in_rule => prelude.clear(),
            _ if !in_rule => prelude.push(character),
            _ => {}
        }
    }
    lists
}

/// `list` split on its commas outside brackets.
fn selectors(list: &str) -> Vec<String> {
    let (mut out, mut current, mut depth) = (Vec::new(), String::new(), 0);
    for character in list.chars() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    out.push(current.trim().to_string());
    out
}

/// The selector's last compound (what it styles).
fn subject(selector: &str) -> &str {
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in selector.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ' ' | '>' | '+' | '~' | '\n' if depth == 0 => start = index + 1,
            _ => {}
        }
    }
    &selector[start..]
}

fn exempt(selector: &str) -> bool {
    if selector.starts_with(":where(") {
        return true;
    }
    let subject = subject(selector);
    if subject.starts_with('*') {
        return true;
    }
    let class = subject.strip_prefix('.').map(|rest| {
        rest.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .next()
            .unwrap_or("")
    });
    class.is_some_and(|class| HOST_CHROME.contains(&class))
}

#[test]
fn every_rule_inside_the_editor_carries_the_shield() {
    let unshielded: Vec<String> = selector_lists(PRESET_CSS)
        .iter()
        .flat_map(|list| selectors(list))
        .filter(|selector| !selector.starts_with(SHIELD) && !exempt(selector))
        .collect();
    assert!(
        unshielded.is_empty(),
        "rules without `{}` lose to the host shield's reset: {unshielded:#?}",
        SHIELD.trim()
    );
}

#[test]
fn the_shield_resets_what_the_editor_draws() {
    assert!(PRESET_CSS.contains("all: revert;"));
    // The parser above finds rules, and the check above would see a
    // rule written without the shield.
    let lists = selector_lists(".config-editor__x { a: b }\n@media (x) { :not(#\\#) .y { c: d } }");
    assert_eq!(lists, [".config-editor__x", r":not(#\#) .y"]);
    assert!(
        !exempt(".config-editor__x")
            && exempt(".config-editor__toolbar")
            && exempt(":where(.a) { }")
    );
}
