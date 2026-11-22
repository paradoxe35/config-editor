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
//! inside the frame with the shield, which is shielded. The rules in
//! `@layer config-editor.chrome` that set only `--config-editor-*`
//! variables: the reset leaves variables alone, and the layer exists so a
//! host's unlayered rule wins over them.

use leptos_config_editor::PRESET_CSS;

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

/// A style rule: its selector list, its declarations, and the preludes of
/// the at-rules around it, outermost first.
struct Rule {
    selectors: String,
    body: String,
    within: Vec<String>,
}

/// The style rules of `css`.
fn rules(css: &str) -> Vec<Rule> {
    let css = without_comments(css);
    let mut rules = Vec::new();
    let mut within: Vec<String> = Vec::new();
    let mut prelude = String::new();
    let mut rule: Option<Rule> = None;
    for character in css.chars() {
        match (&mut rule, character) {
            (None, '{') => {
                let text = prelude.trim().to_string();
                if text.starts_with('@') {
                    within.push(text);
                } else {
                    rule = Some(Rule {
                        selectors: text,
                        body: String::new(),
                        within: within.clone(),
                    });
                }
                prelude.clear();
            }
            (Some(_), '}') => rules.extend(rule.take()),
            (None, '}') => {
                within.pop();
                prelude.clear();
            }
            (None, ';') => prelude.clear(),
            (None, _) => prelude.push(character),
            (Some(rule), _) => rule.body.push(character),
        }
    }
    rules
}

/// The selector lists of the style rules (at-rules' preludes left out).
fn selector_lists(css: &str) -> Vec<String> {
    rules(css).into_iter().map(|rule| rule.selectors).collect()
}

/// A rule in the cosmetic layer that sets only the editor's custom
/// properties. The shield's `all: revert` leaves custom properties as they
/// are, so it cannot undo such a rule; and the layer is there so that a
/// host's unlayered rule wins over it, which the shield's weight would not
/// change. A layer rule that sets a real property still needs the shield.
fn sets_only_variables_in_the_chrome_layer(rule: &Rule) -> bool {
    let declarations: Vec<&str> = rule
        .body
        .split(';')
        .map(str::trim)
        .filter(|declaration| !declaration.is_empty())
        .collect();
    rule.within
        .iter()
        .any(|prelude| prelude == "@layer config-editor.chrome")
        && !declarations.is_empty()
        && declarations
            .iter()
            .all(|declaration| declaration.starts_with("--config-editor-"))
}

/// The selectors of `css`'s rules that would lose to the shield's reset.
fn unshielded(css: &str) -> Vec<String> {
    let mut unshielded = Vec::new();
    for rule in rules(css) {
        if sets_only_variables_in_the_chrome_layer(&rule) {
            continue;
        }
        let selectors = selectors(&rule.selectors);
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
    unshielded
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
    let unshielded = unshielded(PRESET_CSS);
    assert!(
        unshielded.is_empty(),
        "rules without `{}` lose to the host shield's reset: {unshielded:#?}",
        SHIELD.trim()
    );
}

/// Only the cosmetic layer's rules that set nothing but the editor's
/// variables go without the shield: a layer rule that sets a real
/// property, a variable rule outside the layer, and a rule in another
/// at-rule are still named.
#[test]
fn the_chrome_layer_is_exempt_only_for_its_variables() {
    let rule = ".config-editor-documents__add:hover";
    let layer = |body: &str| format!("@layer config-editor.chrome {{ {rule} {{ {body} }} }}");
    assert!(unshielded(&layer("--config-editor-tab-add-bg: red;")).is_empty());
    assert!(unshielded(&layer("--config-editor-a: red; --config-editor-b: blue")).is_empty());
    for css in [
        layer("color: red;"),
        layer("--config-editor-tab-add-bg: red; color: red;"),
        layer("--host-own: red;"),
        layer(""),
        format!("{rule} {{ --config-editor-a: red; }}"),
        format!("@layer other {{ {rule} {{ --config-editor-a: red; }} }}"),
        format!("@media (x) {{ {rule} {{ --config-editor-a: red; }} }}"),
    ] {
        assert_eq!(unshielded(&css), [rule], "{css}");
    }
    // A rule after the layer's block is outside it again.
    let after = format!(
        "{} .config-editor__x {{ --config-editor-a: red; }}",
        layer("--config-editor-a: red;")
    );
    assert_eq!(unshielded(&after), [".config-editor__x"]);
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

/// The page's inherited text properties stop at the roots:
/// each is set on them, and the frame (and the documents view's column)
/// is inline-level, which keeps the page's text decoration off the text.
#[test]
fn the_roots_set_every_inherited_text_property() {
    let css = without_comments(PRESET_CSS);
    // The roots' text properties come at the shield's weight, so a page
    // rule naming an id cannot reach them.
    let start = css
        .find(":not(#\\#):not(#\\#) .config-editor,\n:not(#\\#):not(#\\#) .config-editor-documents")
        .expect("the roots' shielded text rule");
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
