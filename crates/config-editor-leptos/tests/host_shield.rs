//! The host shield (see the top of `src/css/base.css`): every element the
//! editor draws inside its roots starts from the browser's own styles, so
//! a page's rules for bare elements never reach it, and every rule of the
//! preset's for those elements carries `:not(#\#):not(#\#)` (two ids' weight) to
//! outweigh the page's attribute, `:not()` and id-scoped (`#app div`) rules. A rule written
//! without it would lose to the shield's reset, and the element would show
//! unstyled: this test names such rules.
//!
//! Left out on purpose: the color tokens (`:where(…)`, weightless so a
//! host can set them) and the roots (`.config-editor`,
//! `.config-editor-documents`, `.config-editor-theme`). The toolbar's and
//! footer's classes keep a class's weight for a host's own toolbar or
//! footer (`ConfigEditorToolbar`, `ConfigEditorFooter`), whose contents are
//! the page's; each such rule also names the built-in toolbar or footer
//! inside the frame with the shield, which is shielded (QA 32, UX32-2).

use config_editor_leptos::PRESET_CSS;

const SHIELD: &str = r":not(#\#):not(#\#) ";

/// The built-in toolbar and footer: inside the frame.
const BUILT_IN: &str = r":not(#\#):not(#\#) .config-editor__frame ";

/// The roots: their rules keep a class's weight.
const ROOTS: &[&str] = &[
    "config-editor",
    "config-editor-documents",
    "config-editor-theme",
];

/// The classes of what a host composes: their rules keep a class's
/// weight, beside a shielded twin for the built-in toolbar and footer.
const HOST_CHROME: &[&str] = &[
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

/// The class a selector styles (its last compound's first class).
fn subject_class(selector: &str) -> Option<&str> {
    subject(selector).strip_prefix('.').map(|rest| {
        rest.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .next()
            .unwrap_or("")
    })
}

fn exempt(selector: &str) -> bool {
    selector.starts_with(":where(")
        || subject(selector).starts_with('*')
        || subject_class(selector).is_some_and(|class| ROOTS.contains(&class))
}

fn host_chrome(selector: &str) -> bool {
    subject_class(selector).is_some_and(|class| HOST_CHROME.contains(&class))
}

#[test]
fn every_rule_inside_the_editor_carries_the_shield() {
    let mut unshielded = Vec::new();
    for list in selector_lists(PRESET_CSS) {
        let selectors = selectors(&list);
        for selector in &selectors {
            // The id weight on an ancestor (`:not(#\#):not(#\#) .x`) or joined to
            // the first compound (`:not(#\#)[dir="rtl"] .x`, which also
            // matches `<html dir="rtl">`, an element with no ancestor).
            if selector.starts_with(SHIELD.trim_end()) || exempt(selector) {
                continue;
            }
            // A host's toolbar or footer: fine with its shielded twin for
            // the built-in one.
            let twin = format!("{BUILT_IN}{selector}");
            if host_chrome(selector) && selectors.contains(&twin) {
                continue;
            }
            unshielded.push(selector.clone());
        }
    }
    assert!(
        unshielded.is_empty(),
        "rules without `{}` lose to the host shield's reset: {unshielded:#?}",
        SHIELD.trim()
    );
}

/// An editor in a documents view is inside a root, so shielded: each rule
/// of the editor root's names it there with the shield too.
#[test]
fn the_editor_root_rules_reach_an_editor_in_a_documents_view() {
    let twin = r":not(#\#):not(#\#) .config-editor-documents .config-editor";
    let missing: Vec<String> = selector_lists(PRESET_CSS)
        .into_iter()
        .filter(|list| {
            let selectors = selectors(list);
            selectors
                .iter()
                .any(|selector| selector == ".config-editor")
                && !selectors.iter().any(|selector| selector == twin)
        })
        .collect();
    assert!(missing.is_empty(), "without `{twin}`: {missing:#?}");
}

/// QA 32, UX32-1: the page's inherited text properties stop at the roots:
/// each is set on them, and the frame (and the documents view's column)
/// is inline-level, which keeps the page's text decoration off the text.
#[test]
fn the_roots_set_every_inherited_text_property() {
    let css = without_comments(PRESET_CSS);
    let start = css
        .find(".config-editor,\n.config-editor-documents,\n")
        .expect("the roots' rule");
    let rule = &css[start..start + css[start..].find('}').unwrap()];
    for property in [
        "text-align:",
        "text-align-last:",
        "text-justify:",
        "text-indent:",
        "text-transform:",
        "text-shadow:",
        "text-emphasis:",
        "text-underline-offset:",
        "text-underline-position:",
        "text-rendering:",
        "letter-spacing:",
        "word-spacing:",
        "white-space:",
        "word-break:",
        "overflow-wrap:",
        "line-break:",
        "hyphens:",
        "tab-size:",
        "writing-mode:",
        "font-feature-settings:",
        "font-variation-settings:",
        "font-kerning:",
        "font-variant:",
        "-webkit-text-stroke:",
        "-webkit-text-fill-color:",
        "-webkit-text-security:",
    ] {
        assert!(
            rule.contains(property),
            "{property} is not set on the roots"
        );
    }
    assert!(css.contains("display: inline-flex;"));
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
            && !exempt(".config-editor__toolbar")
            && host_chrome(".config-editor__toolbar")
            && exempt(".config-editor")
            && exempt(":where(.a) { }")
    );
}
