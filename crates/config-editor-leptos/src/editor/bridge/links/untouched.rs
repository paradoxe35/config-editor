//! The scalars of a view the user left as the editor wrote them, and the
//! host value each stands for: the lossless part of a save (toml_edit's
//! `Formatted<T>` model, with the host text as the scalar's repr). A view
//! writes a host value in its own spelling (an XML view reads every
//! scalar as text, a switch converting unsaved edits writes the next view
//! from that reading), so the view's value is not the host's; but a
//! scalar whose text is still the text the editor wrote for its source is
//! that source, and the save takes the host's value, which the host's
//! writers keep as written. A scalar typed otherwise is the user's value.

use super::super::super::codec;
use super::super::super::language::ConfigLanguage;
use super::super::super::parse_cache;
use super::super::super::paths;
use super::told::same_keys;
use super::{Links, Source, spelled_in};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A scalar of the view left as the editor wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Untouched {
    /// Its pointer in the view's value.
    pub(crate) pointer: String,
    /// The host text it stands for, and its pointer there.
    pub(crate) host: Arc<str>,
    pub(crate) from: String,
}

/// A scalar of the view and where to find what the editor wrote for it.
struct Candidate {
    pointer: String,
    host: Arc<str>,
    from: String,
    /// The view text the editor wrote, and the scalar's pointer there.
    shown: Arc<str>,
    at: String,
}

/// Every scalar's pointer in `value`, in document order of the value.
fn scalars(value: &Value, pointer: &mut String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&codec::escape(key));
                scalars(child, pointer, out);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&index.to_string());
                scalars(child, pointer, out);
                pointer.truncate(length);
            }
        }
        _ => out.push(pointer.clone()),
    }
}

/// The text `language` writes the value at each of `pointers` with in
/// `text` (`None` where it finds none).
fn written<'t>(
    language: &dyn ConfigLanguage,
    text: &'t str,
    pointers: &[String],
) -> Vec<Option<&'t str>> {
    // Every span from one read of the text.
    paths::with_spans(language, text, pointers, || {
        pointers
            .iter()
            .map(|pointer| {
                let span = paths::memo_value_span(language, text, pointer)
                    .or_else(|| codec::inline_location(language, text, pointer, false))?;
                text.get(codec::written_range(language, text, span)?)
            })
            .collect()
    })
}

impl Links {
    /// The scalars of `text` (in `language`) the user left as the editor
    /// wrote them, with their sources.
    pub(crate) fn untouched(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Vec<Untouched> {
        let parsed = parse_cache::parsed(language, text);
        let Ok(value) = parsed.as_ref() else {
            return Vec::new();
        };
        let mut pointers = Vec::new();
        scalars(value, &mut String::new(), &mut pointers);
        if pointers.is_empty() {
            return Vec::new();
        }
        let label = language.info().label;
        let candidates: Vec<Candidate> = match self {
            // The text since the last save or switch, its keys where they
            // were: each scalar is the host's at its own pointer.
            Links::Fresh {
                host,
                base,
                reading,
                ..
            } if same_keys(language, base, text) => {
                let host_value = reading.as_ref().and_then(|reading| reading.read(host));
                pointers
                    .into_iter()
                    .map(|pointer| Candidate {
                        host: host.clone(),
                        from: match &host_value {
                            Some(host_value) => spelled_in(host_value, &pointer),
                            None => pointer.clone(),
                        },
                        shown: base.clone(),
                        at: pointer.clone(),
                        pointer,
                    })
                    .collect()
            }
            _ => {
                let Some(paired) = self.paired(language, text) else {
                    return Vec::new();
                };
                let sources: HashMap<String, Arc<Source>> = paired.into_iter().collect();
                pointers
                    .into_iter()
                    .filter_map(|pointer| {
                        // The innermost item holding it, and the rest of
                        // its path.
                        let mut item = pointer.as_str();
                        let source = loop {
                            if let Some(source) = sources.get(item) {
                                break source;
                            }
                            item = &item[..item.rfind('/')?];
                        };
                        let rest = &pointer[item.len()..];
                        let shown = source
                            .shown
                            .as_ref()
                            .filter(|shown| shown.language == label)?;
                        let at = format!("{}{rest}", shown.pointer);
                        if shown.kept.as_ref().is_some_and(|kept| !kept.contains(&at)) {
                            return None;
                        }
                        Some(Candidate {
                            host: source.host.clone(),
                            from: format!("{}{rest}", source.pointer),
                            shown: shown.text.clone(),
                            at,
                            pointer,
                        })
                    })
                    .collect()
            }
        };
        // Each view text the editor wrote read once.
        let now: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.pointer.clone())
            .collect();
        let now = written(language, text, &now);
        let mut was = vec![None; candidates.len()];
        let mut texts: Vec<&Arc<str>> = Vec::new();
        for candidate in &candidates {
            if !texts.iter().any(|text| Arc::ptr_eq(text, &candidate.shown)) {
                texts.push(&candidate.shown);
            }
        }
        for shown in texts {
            let at: Vec<usize> = (0..candidates.len())
                .filter(|at| Arc::ptr_eq(&candidates[*at].shown, shown))
                .collect();
            let pointers: Vec<String> = at.iter().map(|at| candidates[*at].at.clone()).collect();
            // The text and what it reads as both: a scalar's span can
            // stay while text the user put below it joins it (a plain
            // YAML scalar running on).
            let read = parse_cache::parsed(language, shown);
            let held = read.as_ref().as_ref().ok();
            for (index, spelled) in at.into_iter().zip(written(language, shown, &pointers)) {
                let candidate = &candidates[index];
                let same = held.and_then(|held| held.pointer(&candidate.at))
                    == value.pointer(&candidate.pointer);
                was[index] = spelled.filter(|_| same).map(str::to_string);
            }
        }
        candidates
            .into_iter()
            .zip(now.into_iter().zip(was))
            .filter(|(_, (now, was))| now.is_some() && now.map(str::to_string) == *was)
            .map(|(candidate, _)| Untouched {
                pointer: candidate.pointer,
                host: candidate.host,
                from: candidate.from,
            })
            .collect()
    }
}
