//! The options of an object's `oneOf`/`anyOf` (a DNS provider written as
//! `cloudflare: {…}` or `route53: {…}`) as hover and suggestions name
//! them, and which one the object is in (see `narrow`). Hover names the
//! matched option (or lists them all), and key suggestions carry their
//! option's title and list the matched option's keys first.

use super::node::{Node, State, Tree, Walked};
use super::query::leaves;
use super::type_includes;
use crate::editor::completion::ValueDoc;
use serde_json::Value;
use std::collections::HashSet;
use std::rc::Rc;

/// One object option of a `oneOf`/`anyOf`.
pub(crate) struct ObjectChoice<'a> {
    /// Every schema applying in it.
    pub(crate) branches: Vec<&'a Value>,
    /// Its `title` and `description`.
    pub(crate) doc: ValueDoc,
    matched: bool,
}

impl ObjectChoice<'_> {
    /// Whether `branch` (a schema applying to the object) is part of this
    /// option.
    pub(crate) fn holds(&self, branch: &Value) -> bool {
        self.branches.iter().any(|own| std::ptr::eq(*own, branch))
    }
}

/// The object options of an object: those of every `oneOf`/`anyOf`
/// applying to it with at least two object options (other options, a
/// `null` beside an object, are left out; the one object option beside
/// others is what an object is, so its own options are the object's).
pub(crate) struct Choices<'a> {
    pub(crate) choices: Vec<ObjectChoice<'a>>,
}

impl<'a> Choices<'a> {
    pub(super) fn of(tree: &Tree<'a>) -> Self {
        let mut found = Found {
            visited: HashSet::new(),
            choices: Vec::new(),
        };
        found.of(tree);
        Self {
            choices: found.choices,
        }
    }

    /// The choice `branch` belongs to, and whether it is a matched one.
    pub(crate) fn of_branch(&self, branch: &Value) -> Option<(&ObjectChoice<'a>, bool)> {
        self.choices
            .iter()
            .find(|choice| choice.holds(branch))
            .map(|choice| (choice, choice.matched))
    }

    /// The choices offering `key`, each with the key's schema there: the
    /// matched one alone when it offers it.
    pub(crate) fn holding(&self, key: &str) -> Vec<(&ObjectChoice<'a>, &'a Value)> {
        let property = |choice: &ObjectChoice<'a>| {
            choice
                .branches
                .iter()
                .find_map(|branch| branch.get("properties")?.get(key))
        };
        if let Some((choice, schema)) = self
            .choices
            .iter()
            .filter(|choice| choice.matched)
            .find_map(|choice| Some((choice, property(choice)?)))
        {
            return vec![(choice, schema)];
        }
        self.choices
            .iter()
            .filter_map(|choice| property(choice).map(|schema| (choice, schema)))
            .collect()
    }

    /// The titles of the choices offering `key` (see [`Self::holding`]),
    /// in one: `Manual, Static file`.
    pub(crate) fn titles_holding(&self, key: &str) -> Option<String> {
        let titles: Vec<&str> = self
            .holding(key)
            .into_iter()
            .filter_map(|(choice, _)| choice.doc.title.as_deref())
            .collect();
        (!titles.is_empty()).then(|| titles.join(", "))
    }

    /// What hover on `key`, a key of the object, says about the choices:
    /// the one offering it (the matched one when it does), else the titles
    /// of all that do.
    pub(crate) fn key_hover(&self, key: &str) -> Option<ChoiceHover> {
        match self.holding(key)[..] {
            [] => None,
            [(choice, _)] => {
                let doc = choice.doc.clone();
                (doc != ValueDoc::default()).then_some(ChoiceHover::Matched(doc))
            }
            ref several => {
                let titles: Vec<String> = several
                    .iter()
                    .filter_map(|(choice, _)| choice.doc.title.clone())
                    .collect();
                (!titles.is_empty()).then_some(ChoiceHover::OneOf(titles))
            }
        }
    }

    /// What hover says about them: the matched choice's title and
    /// description, else every titled choice.
    pub(crate) fn hover(&self) -> Option<ChoiceHover> {
        if let Some(choice) = self.choices.iter().find(|choice| choice.matched) {
            let doc = choice.doc.clone();
            return (doc != ValueDoc::default()).then_some(ChoiceHover::Matched(doc));
        }
        let titles: Vec<String> = self
            .choices
            .iter()
            .filter_map(|choice| choice.doc.title.clone())
            .collect();
        (!titles.is_empty()).then_some(ChoiceHover::OneOf(titles))
    }
}

/// What a field's hover says about the options its object may take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChoiceHover {
    /// The option the value is in.
    Matched(ValueDoc),
    /// No option matches yet: the titles of all of them.
    OneOf(Vec<String>),
}

struct Found<'a> {
    visited: HashSet<*const Node<'a>>,
    choices: Vec<ObjectChoice<'a>>,
}

impl<'a> Found<'a> {
    /// The choices of `tree`: through what applies (not an `if` left
    /// undecided), into the one object option of a `oneOf` beside others.
    fn of(&mut self, tree: &Tree<'a>) {
        if !self.visited.insert(Rc::as_ptr(tree)) {
            return;
        }
        match &**tree {
            Node::All(parts) | Node::Any(parts) => {
                parts.iter().for_each(|part| self.of(part));
            }
            Node::Options(options) => {
                let objects: Vec<(&Tree<'a>, Vec<&'a Value>, State)> = options
                    .iter()
                    .filter_map(|option| {
                        let branches = leaves(&option.tree, Walked::Any);
                        is_object(&branches).then_some((&option.tree, branches, option.state))
                    })
                    .collect();
                match &objects[..] {
                    [(only, _, _)] => self.of(only),
                    [] => {}
                    _ => {
                        for (_, branches, state) in objects {
                            let doc = doc_of(&branches);
                            self.choices.push(ObjectChoice {
                                branches,
                                doc,
                                matched: state == State::Matched,
                            });
                        }
                    }
                }
            }
            Node::Leaf(_) | Node::Either(_) | Node::Cut => {}
        }
    }
}

/// Whether an option's schemas describe an object.
fn is_object(branches: &[&Value]) -> bool {
    branches.iter().any(|branch| {
        branch.get("properties").is_some()
            || branch.get("required").is_some()
            || type_includes(branch, "object")
    })
}

fn doc_of(branches: &[&Value]) -> ValueDoc {
    let text = |keyword: &str| {
        branches
            .iter()
            .find_map(|branch| branch.get(keyword).and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    ValueDoc {
        title: text("title"),
        description: text("description"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Here, Seen};
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "$defs": {
                "cloudflare": {
                    "title": "Cloudflare",
                    "required": ["cloudflare"],
                    "properties": {"cloudflare": {"type": "object"}}
                }
            },
            "properties": {
                "dns": {
                    "oneOf": [
                        {"$ref": "#/$defs/cloudflare"},
                        {"title": "Route 53", "required": ["route53"], "properties": {"route53": {}}},
                        {"title": "Manual", "required": ["provider"], "properties": {"provider": {"const": "manual"}}},
                        {"title": "Static", "required": ["provider"], "properties": {"provider": {"const": "static"}}},
                        {"type": "null"}
                    ]
                }
            }
        })
    }

    fn matched(document: Value) -> Option<String> {
        let root = schema();
        let here = Here::walk(
            super::super::Entry::schema(&root["properties"]["dns"]),
            &root,
            Seen::value(Some(&document)),
        );
        let choices = here.choices();
        assert_eq!(choices.choices.len(), 4, "the null option is no object");
        choices
            .choices
            .iter()
            .find(|choice| choice.matched)
            .and_then(|choice| choice.doc.title.clone())
    }

    #[test]
    fn a_choice_is_matched_by_its_required_keys_then_its_consts() {
        assert_eq!(
            matched(json!({"cloudflare": {}})).as_deref(),
            Some("Cloudflare")
        );
        assert_eq!(matched(json!({"route53": {}})).as_deref(), Some("Route 53"));
        assert_eq!(
            matched(json!({"provider": "static"})).as_deref(),
            Some("Static")
        );
        assert_eq!(
            matched(json!({"provider": "manual"})).as_deref(),
            Some("Manual")
        );
        assert_eq!(matched(json!({"provider": "other"})), None);
        assert_eq!(matched(json!({"ttl": 3})), None);
        assert_eq!(matched(json!({"cloudflare": {}, "route53": {}})), None);
    }

    #[test]
    fn keys_alone_match_where_the_document_gives_no_value() {
        let root = schema();
        let dns = super::super::Entry::schema(&root["properties"]["dns"]);
        let keys: HashSet<String> = ["cloudflare".to_string()].into();
        let here = Here::walk(
            dns.clone(),
            &root,
            Seen {
                value: None,
                keys: Some(&keys),
            },
        );
        assert!(here.choices().choices[0].matched);
        let provider: HashSet<String> = ["provider".to_string()].into();
        let unknown = Here::walk(
            dns,
            &root,
            Seen {
                value: None,
                keys: Some(&provider),
            },
        );
        assert_eq!(
            unknown.choices().hover(),
            Some(ChoiceHover::OneOf(vec![
                "Cloudflare".into(),
                "Route 53".into(),
                "Manual".into(),
                "Static".into()
            ])),
            "two options require provider"
        );
    }
}
