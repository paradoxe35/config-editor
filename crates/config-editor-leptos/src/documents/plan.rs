//! The decisions behind the documents view, apart from the DOM: what a new
//! list from the host does to the open editors, which document is active
//! after it, where the switch keys go, and which compiled schemas are
//! shared.

use std::collections::{HashMap, HashSet};

use crate::{Language, Schema, SchemaSource};

/// What an editor is built with once: changing either mounts a new editor
/// for the document (the rest of a [`Document`](crate::Document) updates
/// the mounted one live).
#[derive(Clone, Debug)]
pub(super) struct Setup {
    pub(super) language: Language,
    /// After [`SchemaCache::resolve_all`]: compiled and shared, or the JSON
    /// text of a schema that does not compile (the editor shows why).
    pub(super) schema: Option<SchemaSource>,
}

impl Setup {
    /// Whether an editor built with `self` serves `other` as it is.
    pub(super) fn same_editor(&self, other: &Setup) -> bool {
        let same_schema = match (&self.schema, &other.schema) {
            (None, None) => true,
            (Some(SchemaSource::Compiled(left)), Some(SchemaSource::Compiled(right))) => {
                left.shares(right)
            }
            (Some(SchemaSource::Json(left)), Some(SchemaSource::Json(right))) => left == right,
            _ => false,
        };
        self.language == other.language && same_schema
    }
}

/// What becomes of a document of the host's new list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fate {
    /// It was open with the same setup: its editor stays and takes the new
    /// text, baseline and read-only state.
    Keep(usize),
    /// It was open with another language or schema: a new editor replaces
    /// the old one.
    Remount(usize),
    /// New to the list.
    Add,
    /// An id already used earlier in the list: ignored.
    Duplicate,
}

/// The fate of each document of `incoming` (by position), and the
/// positions in `existing` of the documents no longer listed.
pub(super) fn reconcile(
    existing: &[(&str, &Setup)],
    incoming: &[(&str, &Setup)],
) -> (Vec<Fate>, Vec<usize>) {
    let positions: HashMap<&str, usize> = existing
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (*id, index))
        .collect();
    let mut seen = HashSet::new();
    let fates = incoming
        .iter()
        .map(|(id, setup)| {
            if !seen.insert(*id) {
                return Fate::Duplicate;
            }
            match positions.get(id) {
                Some(&index) if existing[index].1.same_editor(setup) => Fate::Keep(index),
                Some(&index) => Fate::Remount(index),
                None => Fate::Add,
            }
        })
        .collect();
    let removed = existing
        .iter()
        .enumerate()
        .filter(|(_, (id, _))| !seen.contains(id))
        .map(|(index, _)| index)
        .collect();
    (fates, removed)
}

/// The active document after the list went from `previous` to `current`:
/// the same one while it is listed; when it was closed, the one after it
/// (else before it) that is still listed, as an editor's tabs do; the first
/// one when none was active.
pub(super) fn active_after(
    previous: &[String],
    current: &[String],
    active: Option<&str>,
) -> Option<String> {
    let listed = |id: &str| current.iter().any(|candidate| candidate == id);
    if let Some(active) = active.filter(|active| listed(active)) {
        return Some(active.to_string());
    }
    if let Some(index) = active.and_then(|active| previous.iter().position(|id| id == active)) {
        let after = previous[index + 1..].iter();
        let before = previous[..index].iter().rev();
        if let Some(neighbor) = after.chain(before).find(|id| listed(id)) {
            return Some(neighbor.clone());
        }
    }
    current.first().cloned()
}

/// The document `offset` places from `active` in `ids`, wrapping around at
/// either end (the switch keys and the tab list's arrows); the first one
/// when `active` is not listed.
pub(super) fn step(ids: &[String], active: Option<&str>, offset: isize) -> Option<String> {
    let count = isize::try_from(ids.len()).ok().filter(|count| *count > 0)?;
    let Some(index) = active.and_then(|active| ids.iter().position(|id| id == active)) else {
        return ids.first().cloned();
    };
    let index = isize::try_from(index).ok()?;
    let target = usize::try_from((index + offset).rem_euclid(count)).ok()?;
    ids.get(target).cloned()
}

/// Compiled schemas by their JSON text, so documents given the same text
/// share one, compiled once.
#[derive(Default)]
pub(super) struct SchemaCache {
    compiled: HashMap<String, Schema>,
}

impl SchemaCache {
    /// Resolves each document's schema: JSON text to the compiled schema of
    /// that text (compiled now if no document had it before), a compiled
    /// schema as it is. Text that does not compile stays text, so the
    /// editor shows the error. Schemas no document uses any more are
    /// dropped.
    pub(super) fn resolve_all(
        &mut self,
        sources: Vec<Option<SchemaSource>>,
    ) -> Vec<Option<SchemaSource>> {
        let mut previous = std::mem::take(&mut self.compiled);
        sources
            .into_iter()
            .map(|source| match source? {
                SchemaSource::Json(json) => {
                    let schema = match self.compiled.get(&json) {
                        Some(schema) => Some(schema.clone()),
                        None => previous
                            .remove(&json)
                            .or_else(|| Schema::compile(&json).ok()),
                    };
                    Some(match schema {
                        Some(schema) => {
                            self.compiled.insert(json, schema.clone());
                            SchemaSource::Compiled(schema)
                        }
                        None => SchemaSource::Json(json),
                    })
                }
                compiled => Some(compiled),
            })
            .collect()
    }

    #[cfg(all(
        test,
        any(
            feature = "yaml",
            feature = "json",
            feature = "toml",
            feature = "hcl",
            feature = "xml"
        )
    ))]
    fn len(&self) -> usize {
        self.compiled.len()
    }
}

#[cfg(all(
    test,
    any(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )
))]
mod tests {
    use super::*;

    /// The ids before, the ids after, the active one before, the one expected.
    type ActiveCase = (
        &'static [&'static str],
        &'static [&'static str],
        Option<&'static str>,
        Option<&'static str>,
    );

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    fn setup(language: Language) -> Setup {
        Setup {
            language,
            schema: None,
        }
    }

    #[test]
    fn the_active_document_stays_while_listed_and_moves_to_a_neighbor_when_closed() {
        // (previous, current, active, expected): every case reported at once.
        let cases: &[ActiveCase] = &[
            (&["a", "b", "c"], &["a", "b", "c"], Some("b"), Some("b")),
            (&["a", "b", "c"], &["c", "b", "a"], Some("b"), Some("b")),
            (&["a", "b", "c"], &["a", "c"], Some("b"), Some("c")),
            (&["a", "b", "c"], &["a", "b"], Some("c"), Some("b")),
            (&["a", "b", "c"], &["b", "c"], Some("a"), Some("b")),
            (&["a", "b", "c"], &["a"], Some("c"), Some("a")),
            (&["a", "b", "c"], &[], Some("b"), None),
            (&["a", "b", "c"], &["x", "y"], Some("b"), Some("x")),
            (&[], &["x", "y"], None, Some("x")),
            (&["a"], &["a", "b"], None, Some("a")),
            (&["a", "b"], &["a", "b"], Some("gone"), Some("a")),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|(previous, current, active, expected)| {
                let got = active_after(&ids(previous), &ids(current), *active);
                (got.as_deref() != *expected).then(|| {
                    format!("{previous:?} -> {current:?} with {active:?}: got {got:?}, want {expected:?}")
                })
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn switching_steps_through_the_documents_and_wraps_around() {
        let list = ids(&["a", "b", "c"]);
        let cases: &[(Option<&str>, isize, Option<&str>)] = &[
            (Some("a"), 1, Some("b")),
            (Some("c"), 1, Some("a")),
            (Some("a"), -1, Some("c")),
            (Some("b"), -1, Some("a")),
            (Some("b"), 4, Some("c")),
            (None, 1, Some("a")),
            (Some("gone"), -1, Some("a")),
        ];
        for (active, offset, expected) in cases {
            assert_eq!(
                step(&list, *active, *offset).as_deref(),
                *expected,
                "{active:?} {offset}"
            );
        }
        assert_eq!(step(&[], Some("a"), 1), None);
        assert_eq!(
            step(&ids(&["only"]), Some("only"), 1).as_deref(),
            Some("only")
        );
    }

    #[test]
    fn a_new_list_keeps_remounts_adds_and_removes_by_id() {
        let yaml = setup(Language::default());
        let other = Language::ALL
            .iter()
            .copied()
            .find(|language| *language != Language::default());
        let changed = other.map(setup);
        let existing = [("a", &yaml), ("b", &yaml), ("c", &yaml)];
        let mut incoming = vec![("c", &yaml), ("a", &yaml), ("d", &yaml), ("a", &yaml)];
        if let Some(changed) = &changed {
            incoming.push(("b", changed));
        }
        let (fates, removed) = reconcile(&existing, &incoming);
        let mut expected = vec![Fate::Keep(2), Fate::Keep(0), Fate::Add, Fate::Duplicate];
        if changed.is_some() {
            expected.push(Fate::Remount(1));
            assert!(removed.is_empty());
        } else {
            assert_eq!(removed, vec![1]);
        }
        assert_eq!(fates, expected);
    }

    #[test]
    fn documents_with_the_same_schema_text_share_one_compiled_schema() {
        let json = r#"{"properties":{"listen":{"type":"integer"}}}"#;
        let other = r#"{"properties":{"name":{"type":"string"}}}"#;
        let compiled = Schema::compile(other).unwrap();
        let mut cache = SchemaCache::default();
        let resolved = cache.resolve_all(vec![
            Some(json.into()),
            None,
            Some(json.to_string().into()),
            Some(compiled.clone().into()),
            Some("{ not json".into()),
        ]);
        let schema = |index: usize| match &resolved[index] {
            Some(SchemaSource::Compiled(schema)) => schema.clone(),
            other => panic!("document {index}: {other:?}"),
        };
        assert!(schema(0).shares(&schema(2)), "one compile for one text");
        assert!(
            schema(3).shares(&compiled),
            "a compiled schema is passed as it is"
        );
        assert!(resolved[1].is_none());
        assert!(
            matches!(&resolved[4], Some(SchemaSource::Json(text)) if text == "{ not json"),
            "an invalid schema stays text, for the editor to report"
        );
        assert_eq!(cache.len(), 1);

        // A later list reuses the compiled schema, and drops it once unused.
        let again = cache.resolve_all(vec![Some(json.into())]);
        assert!(matches!(&again[0], Some(SchemaSource::Compiled(s)) if s.shares(&schema(0))));
        cache.resolve_all(vec![None]);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn an_editor_is_kept_for_the_same_language_and_shared_schema_only() {
        let schema = Schema::compile("{}").unwrap();
        let recompiled = Schema::compile("{}").unwrap();
        let language = Language::default();
        let with = |schema: Option<SchemaSource>| Setup { language, schema };
        assert!(with(None).same_editor(&with(None)));
        assert!(with(Some(schema.clone().into())).same_editor(&with(Some(schema.clone().into()))));
        assert!(!with(Some(schema.clone().into())).same_editor(&with(Some(recompiled.into()))));
        assert!(!with(Some(schema.into())).same_editor(&with(None)));
        assert!(with(Some("{".into())).same_editor(&with(Some("{".into()))));
        for other in Language::ALL.iter().filter(|other| **other != language) {
            assert!(!with(None).same_editor(&setup(*other)));
        }
    }
}
