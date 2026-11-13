//! The options of an object's `oneOf`/`anyOf` (a DNS provider written as
//! `cloudflare: {…}` or `route53: {…}`), and which one a value is in.
//!
//! The match is cheap and exact, never a validation: the one option whose
//! `required` keys are all present, else the one whose `const` properties
//! the value has, else none. Hover names the matched option (or lists
//! them all), and key suggestions carry their option's title and list the
//! matched option's keys first.

use super::*;

/// One object option of a `oneOf`/`anyOf`.
pub(crate) struct ObjectChoice<'a> {
    /// The option's resolved branches (through `$ref` and the combinators).
    pub(crate) branches: Vec<&'a Value>,
    /// Its `title` and `description`.
    pub(crate) doc: ValueDoc,
    required: Vec<&'a str>,
    /// Its properties that hold one `const` value.
    consts: Vec<(&'a str, &'a Value)>,
}

impl ObjectChoice<'_> {
    /// Whether `branch` (one of the resolved branches of the property) is
    /// part of this option.
    pub(crate) fn holds(&self, branch: &Value) -> bool {
        self.branches.iter().any(|own| std::ptr::eq(*own, branch))
    }

    /// Whether `values` has a const property with another value.
    fn contradicted(&self, values: &serde_json::Map<String, Value>) -> bool {
        self.consts
            .iter()
            .any(|(key, expected)| values.get(*key).is_some_and(|value| value != *expected))
    }

    fn required_present(&self, keys: &HashSet<String>) -> bool {
        !self.required.is_empty() && self.required.iter().all(|key| keys.contains(*key))
    }

    fn consts_match(&self, values: &serde_json::Map<String, Value>) -> bool {
        !self.consts.is_empty()
            && self
                .consts
                .iter()
                .all(|(key, expected)| values.get(*key) == Some(*expected))
    }
}

/// The object options of every `oneOf`/`anyOf` of `schema` (through `$ref`
/// and `allOf`) that offers at least two objects; other options (a `null`
/// beside an object) are left out.
pub(crate) fn object_choices<'a>(schema: &'a Value, root: &'a Value) -> Vec<ObjectChoice<'a>> {
    enum Step<'a> {
        Visit(&'a Value),
        Options(&'a Value),
    }
    let mut choices = Vec::new();
    let mut seen: HashSet<*const Value> = HashSet::new();
    // Each schema walked once, so a reference cycle ends and a chain of
    // any length is followed: the target's choices, then `allOf`'s, then
    // the schema's own.
    let mut steps = vec![Step::Visit(schema)];
    while let Some(step) = steps.pop() {
        let schema = match step {
            Step::Options(schema) => {
                for keyword in ["oneOf", "anyOf"] {
                    let Some(options) = schema.get(keyword).and_then(Value::as_array) else {
                        continue;
                    };
                    let objects: Vec<ObjectChoice<'a>> = options
                        .iter()
                        .filter_map(|option| choice(option, root))
                        .collect();
                    if objects.len() >= 2 {
                        choices.extend(objects);
                    }
                }
                continue;
            }
            Step::Visit(schema) => schema,
        };
        if !seen.insert(std::ptr::from_ref(schema)) {
            continue;
        }
        let mut next: Vec<Step<'a>> = Vec::new();
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| ref_target(reference, root))
        {
            next.push(Step::Visit(target));
        }
        next.extend(
            schema
                .get("allOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(Step::Visit),
        );
        next.push(Step::Options(schema));
        steps.extend(next.into_iter().rev());
    }
    choices
}

/// `option` as a choice, when it describes an object.
fn choice<'a>(option: &'a Value, root: &'a Value) -> Option<ObjectChoice<'a>> {
    let branches = resolve_branches(option, root);
    let is_object = branches.iter().any(|branch| {
        branch.get("properties").is_some()
            || branch.get("required").is_some()
            || type_includes(branch, "object")
    });
    if !is_object {
        return None;
    }
    let text = |keyword: &str| {
        branches
            .iter()
            .find_map(|branch| branch.get(keyword).and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let doc = ValueDoc {
        title: text("title"),
        description: text("description"),
    };
    let mut required = Vec::new();
    let mut consts = Vec::new();
    for branch in &branches {
        for name in branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !required.contains(&name) {
                required.push(name);
            }
        }
        for (key, property) in branch
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if let Some(value) = schema_const(property, root) {
                consts.push((key.as_str(), value));
            }
        }
    }
    Some(ObjectChoice {
        branches,
        doc,
        required,
        consts,
    })
}

/// The choice a value is in: the one whose `required` keys are all in
/// `keys`, else the one whose `const` properties `values` has, else none.
/// `values` (the value itself, which an unfinished document may not give)
/// is only read when the keys do not decide; a choice it contradicts (a
/// `const` with another value) never matches.
pub(crate) fn matched_choice(
    choices: &[ObjectChoice<'_>],
    keys: &HashSet<String>,
    values: impl FnOnce() -> Option<serde_json::Map<String, Value>>,
) -> Option<usize> {
    let by_required: Vec<usize> = (0..choices.len())
        .filter(|&index| choices[index].required_present(keys))
        .collect();
    if let [only] = by_required[..]
        && choices[only].consts.is_empty()
    {
        return Some(only);
    }
    let values = values();
    let possible: Vec<usize> = (0..choices.len())
        .filter(|&index| {
            values
                .as_ref()
                .is_none_or(|values| !choices[index].contradicted(values))
        })
        .collect();
    let by_required: Vec<usize> = by_required
        .into_iter()
        .filter(|index| possible.contains(index))
        .collect();
    if let [only] = by_required[..] {
        return Some(only);
    }
    let values = values?;
    let pool = if by_required.is_empty() {
        possible
    } else {
        by_required
    };
    let by_const: Vec<usize> = pool
        .into_iter()
        .filter(|&index| choices[index].consts_match(&values))
        .collect();
    match by_const[..] {
        [only] => Some(only),
        _ => None,
    }
}

/// The value at `path` in a parsed document: a step written as an index
/// names an item of an array and the key of its digits in an object. An
/// item of something the document has as one value (an XML element written
/// once) is that value.
pub(crate) fn value_at<'v>(document: &'v Value, path: &[SchemaPathSegment]) -> Option<&'v Value> {
    path.iter()
        .try_fold(document, |value, segment| match value {
            Value::Array(items) => items.get(segment.as_index()?),
            Value::Object(members) => members
                .get(&*segment.as_key())
                .or_else(|| (*segment == SchemaPathSegment::ArrayItem(0)).then_some(value)),
            _ => (*segment == SchemaPathSegment::ArrayItem(0)).then_some(value),
        })
}

/// The object choices of every schema in `schemas`.
fn choices_of<'a>(schemas: &[&'a Value], root: &'a Value) -> Vec<ObjectChoice<'a>> {
    schemas
        .iter()
        .flat_map(|schema| object_choices(schema, root))
        .collect()
}

/// The object at `path` in `source`, when it parses.
pub(crate) fn object_at(
    source: &str,
    path: &[SchemaPathSegment],
    behavior: &dyn ConfigLanguage,
) -> Option<serde_json::Map<String, Value>> {
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    value_at(document, path)?.as_object().cloned()
}

/// [`object_at`] with the line at `cursor_byte` left empty: the caret's
/// line holds a key being typed (`<` in XML), which would keep the rest
/// from parsing. Still `None` where the text around it does not parse
/// (a JSON entry ending in a comma): the keys alone then decide.
pub(crate) fn object_around(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
    behavior: &dyn ConfigLanguage,
) -> Option<serde_json::Map<String, Value>> {
    let document = document_around(source, cursor_byte, behavior)?;
    value_at(&document, path)?.as_object().cloned()
}

/// The document `source` holds with the line at `cursor_byte` left empty
/// (see [`object_around`]), when that parses.
pub(crate) fn document_around(
    source: &str,
    cursor_byte: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<Value> {
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |at| cursor_byte + at);
    let blanked = format!("{}{}", &source[..line_start], &source[line_end..]);
    // Read aside: the text with the line left out must not push the text
    // on screen out of the shared parse.
    behavior.parse(&blanked).ok()
}

/// The choices of the value at `path`, and the one it is in.
pub(crate) struct ChoicesAt<'a> {
    pub(crate) choices: Vec<ObjectChoice<'a>>,
    pub(crate) matched: Option<usize>,
}

impl<'a> ChoicesAt<'a> {
    /// The choices of the object described by `schemas`, matched against
    /// its `keys` and, when they do not decide, its parsed `values`.
    pub(crate) fn new(
        schemas: &[&'a Value],
        root: &'a Value,
        keys: &HashSet<String>,
        values: impl FnOnce() -> Option<serde_json::Map<String, Value>>,
    ) -> Self {
        let choices = choices_of(schemas, root);
        let matched = if choices.is_empty() {
            None
        } else {
            matched_choice(&choices, keys, values)
        };
        Self { choices, matched }
    }

    /// [`Self::new`] for an object known only by its parsed value, read
    /// only when `schemas` offer choices (hover reads no document for a
    /// field without them).
    pub(crate) fn in_object(
        schemas: &[&'a Value],
        root: &'a Value,
        object: impl FnOnce() -> Option<serde_json::Map<String, Value>>,
    ) -> Self {
        let choices = choices_of(schemas, root);
        let matched = if choices.is_empty() {
            None
        } else {
            let values = object();
            let keys = values
                .as_ref()
                .map(|values| values.keys().cloned().collect())
                .unwrap_or_default();
            matched_choice(&choices, &keys, || values)
        };
        Self { choices, matched }
    }

    /// The choice `branch` belongs to, and whether it is the matched one.
    pub(crate) fn of(&self, branch: &Value) -> Option<(&ObjectChoice<'a>, bool)> {
        self.choices
            .iter()
            .enumerate()
            .find(|(_, choice)| choice.holds(branch))
            .map(|(index, choice)| (choice, self.matched == Some(index)))
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
        if let Some(choice) = self.matched.map(|index| &self.choices[index])
            && let Some(schema) = property(choice)
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
        if let Some(index) = self.matched {
            let doc = self.choices[index].doc.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn keys(names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

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
        let at = ChoicesAt::new(
            &[&root["properties"]["dns"]],
            &root,
            &document
                .as_object()
                .map(|map| map.keys().cloned().collect())
                .unwrap_or_default(),
            || document.as_object().cloned(),
        );
        assert_eq!(at.choices.len(), 4, "the null option is no object");
        at.matched
            .and_then(|index| at.choices[index].doc.title.clone())
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
    fn values_are_only_read_when_the_keys_do_not_decide() {
        let root = schema();
        let dns = [&root["properties"]["dns"]];
        let at = ChoicesAt::new(&dns, &root, &keys(&["cloudflare"]), || {
            panic!("the required key decides")
        });
        assert_eq!(at.matched, Some(0));
        let unknown = ChoicesAt::new(&dns, &root, &keys(&["provider"]), || None);
        assert_eq!(unknown.matched, None, "two options require provider");
        assert_eq!(
            unknown.hover(),
            Some(ChoiceHover::OneOf(vec![
                "Cloudflare".into(),
                "Route 53".into(),
                "Manual".into(),
                "Static".into()
            ]))
        );
    }

    #[test]
    fn values_are_found_at_their_path() {
        let document = json!({"a": {"b": [{"c": 1}]}, "x": {"y": 2}});
        let key = |name: &str| SchemaPathSegment::Key(name.into());
        assert_eq!(
            value_at(
                &document,
                &[
                    key("a"),
                    key("b"),
                    SchemaPathSegment::ArrayItem(0),
                    key("c")
                ]
            ),
            Some(&json!(1))
        );
        assert_eq!(
            value_at(
                &document,
                &[key("x"), SchemaPathSegment::ArrayItem(0), key("y")]
            ),
            Some(&json!(2)),
            "a lone value is its own first item"
        );
        assert_eq!(value_at(&document, &[key("z")]), None);
    }
}
