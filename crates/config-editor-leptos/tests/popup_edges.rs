//! Every popup gets the strong edge a user asking for more contrast needs
//! (`prefers-contrast: more`), from one rule: the preset sets
//! `--config-editor-popup-edge` only there, and every popup draws its edge
//! with it. A popup is a raised box over the text: a rule drawing a border
//! and a shadow from one of the `--config-editor-*-shadow` colors. A new
//! popup drawing its edge otherwise would keep a faint edge under more
//! contrast: this test names it.

use leptos_config_editor::PRESET_CSS;

const EDGE: &str = "--config-editor-popup-edge";

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

/// A style rule: its selector list, its declarations, and the at-rules
/// around it.
struct Rule {
    selectors: String,
    body: String,
    within: Vec<String>,
}

fn rules(css: &str) -> Vec<Rule> {
    let css = without_comments(css);
    let (mut rules, mut within, mut prelude) = (Vec::new(), Vec::<String>::new(), String::new());
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

/// The declarations of `rule` for `property` (exactly that name).
fn declared<'r>(rule: &'r Rule, property: &str) -> Vec<&'r str> {
    rule.body
        .split(';')
        .filter_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim() == property).then_some(value.trim())
        })
        .collect()
}

/// The class each selector of `rule` styles (its last compound's first
/// class, pseudo-classes and modifiers left on).
fn subjects(rule: &Rule) -> Vec<String> {
    rule.selectors
        .split(',')
        .filter_map(|selector| {
            let last = selector.split_whitespace().next_back()?;
            // A pseudo-element (a scroll bar's thumb) is not the box.
            if last.contains("::") {
                return None;
            }
            let class = last.strip_prefix('.')?;
            Some(
                class
                    .split([':', '[', '.'])
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

/// The classes of the preset's popups (see the module).
fn popups(css: &str) -> Vec<String> {
    let mut found: Vec<String> = rules(css)
        .iter()
        .filter(|rule| {
            !declared(rule, "border").is_empty()
                && declared(rule, "box-shadow").iter().any(|shadow| {
                    shadow.contains("var(--config-editor-") && shadow.contains("shadow)")
                })
        })
        .flat_map(subjects)
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Each popup rule (a popup's class, or one of its modifiers) drawing an
/// edge without the shared variable: `selector: declaration`.
fn faint_edges(css: &str) -> Vec<String> {
    let popups = popups(css);
    let is_popup = |class: &str| {
        popups.iter().any(|popup| {
            class == popup
                || class
                    .strip_prefix(popup.as_str())
                    .is_some_and(|rest| rest.starts_with("--"))
        })
    };
    let mut faint = Vec::new();
    for rule in rules(css) {
        if !subjects(&rule).iter().any(|class| is_popup(class)) {
            continue;
        }
        for property in ["border", "border-color"] {
            for value in declared(&rule, property) {
                if !value.contains(EDGE) && value != "0" && value != "none" {
                    faint.push(format!("{}: {property}: {value}", rule.selectors.trim()));
                }
            }
        }
    }
    faint
}

#[test]
fn every_popup_draws_its_edge_with_the_shared_variable() {
    let popups = popups(PRESET_CSS);
    for expected in [
        "config-editor__hover",
        "config-editor__problem-tip",
        "config-editor__completions",
        "config-editor__uses",
        "config-editor__protect-tip",
        "config-editor__diff-peek",
    ] {
        assert!(
            popups.iter().any(|popup| popup == expected),
            "{expected} is found as a popup: {popups:?}"
        );
    }
    assert_eq!(faint_edges(PRESET_CSS), Vec::<String>::new());
}

#[test]
fn more_contrast_sets_the_edge_for_every_root() {
    let set: Vec<Rule> = rules(PRESET_CSS)
        .into_iter()
        .filter(|rule| !declared(rule, EDGE).is_empty())
        .collect();
    assert_eq!(set.len(), 1, "set in one rule only");
    assert_eq!(set[0].within, ["@media (prefers-contrast: more)"]);
    assert!(
        set[0]
            .selectors
            .starts_with(":where(.config-editor, .config-editor-documents")
    );
    assert_eq!(
        declared(&set[0], EDGE),
        ["var(--config-editor-raised-text)"]
    );
}

#[test]
fn a_popup_drawing_its_own_edge_is_named() {
    let css = ".x__new { border: 1px solid red; box-shadow: 0 1px var(--config-editor-popup-shadow); }\n\
               .x__new--warn { border-color: orange; }\n\
               .x__ok { border: 1px solid var(--config-editor-popup-edge, red); box-shadow: 0 1px var(--config-editor-hover-shadow); }";
    assert_eq!(
        faint_edges(css),
        [
            ".x__new: border: 1px solid red",
            ".x__new--warn: border-color: orange"
        ]
    );
}
