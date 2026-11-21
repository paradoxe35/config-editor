//! The scalars of a view the user left as the editor wrote them, and the
//! host value each stands for: the lossless part of a save (toml_edit's
//! `Formatted<T>` model, with the host text as the scalar's repr). A view
//! writes a host value in its own spelling (an XML view reads every
//! scalar as text, a switch converting unsaved edits writes the next view
//! from that reading), so the view's value is not the host's; but a
//! scalar whose text is still the text the editor wrote for its source is
//! that source, and the save takes the host's value, which the host's
//! writers keep as written. A scalar typed otherwise is the user's value.

use super::super::super::codec::{self, written_text_range};
use super::super::super::language::{ConfigLanguage, pointer_key};
use super::super::super::parse_cache;
use super::super::super::paths;
use super::super::super::text::{TextRange, slice, text_size};
use super::super::super::text_change::changed_bytes;
use super::super::super::work_count::{Work, note_many};
use super::told::same_keys;
use super::{Links, Source};
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
    /// Of a copy of its item (see `Source::copied`): new where it is,
    /// though written as its source.
    pub(crate) copied: bool,
}

/// A scalar of the view and where to find what the editor wrote for it.
struct Candidate {
    pointer: String,
    host: Arc<str>,
    from: String,
    copied: bool,
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
                pointer.push_str(&pointer_key(key));
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

/// Where in `text` `language` writes the value at each of `pointers`
/// (`None` where it finds none).
fn written(
    language: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
) -> Vec<Option<TextRange>> {
    // Every span from one read of the text.
    paths::with_spans(language, text, pointers, || {
        pointers
            .iter()
            .map(|pointer| {
                let span = paths::memo_value_span(language, text, pointer)
                    .or_else(|| codec::inline_location(language, text, pointer, false))?;
                written_text_range(language, text, span)
                    .filter(|range| slice::slice(text, *range).is_some())
            })
            .collect()
    })
}

impl Links {
    /// The scalars of `text` (in `language`) the user left as the editor
    /// wrote them, with their sources; `None` for a text with no record
    /// ([`Links::Unrecorded`]).
    pub(crate) fn untouched(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Option<Vec<Untouched>> {
        if *self == Links::Unrecorded {
            return None;
        }
        let parsed = parse_cache::parsed(language, text);
        let Ok(value) = parsed.as_ref() else {
            return Some(Vec::new());
        };
        let mut pointers = Vec::new();
        scalars(value, &mut String::new(), &mut pointers);
        if pointers.is_empty() {
            return Some(Vec::new());
        }
        let label = language.info().label;
        // The bytes typing changed since the text the editor wrote, when
        // every scalar stands where it did there (see below).
        let mut typed = None;
        let candidates: Vec<Candidate> = match self {
            // The text since the last save or switch, its keys where they
            // were: each scalar is the host's at its own pointer.
            Links::Fresh {
                host,
                base,
                reading,
                ..
            } if same_keys(language, base, text) => {
                // Paired with the host as `base` is: this text holds its
                // keys and lists alike.
                let paths = reading
                    .as_ref()
                    .and_then(|reading| reading.paths(host, value))
                    .unwrap_or_default();
                let (start, _, end) = changed_bytes(base, text);
                typed = Some((base.clone(), start, end));
                pointers
                    .into_iter()
                    .map(|pointer| Candidate {
                        copied: false,
                        host: host.clone(),
                        from: paths.value(&pointer),
                        shown: base.clone(),
                        at: pointer.clone(),
                        pointer,
                    })
                    .collect()
            }
            _ => {
                let Some(paired) = self.paired(language, text) else {
                    return Some(Vec::new());
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
                            item = item.rsplit_once('/')?.0;
                        };
                        let rest = pointer.strip_prefix(item).unwrap_or_default();
                        let shown = source
                            .shown
                            .as_ref()
                            .filter(|shown| shown.language == label)?;
                        let at = format!("{}{rest}", shown.pointer);
                        // A text a switch wrote names each scalar's own
                        // source: the item holding it may hold items moved
                        // or copied within it.
                        // A copy keeps its mark across the switch, its item
                        // there linked or not.
                        let (host, from, copied) = match &shown.kept {
                            Some(kept) => {
                                let kept = kept.get(&at)?;
                                (
                                    kept.host.clone(),
                                    kept.from.clone(),
                                    kept.copied || source.copied,
                                )
                            }
                            None => (
                                source.host.clone(),
                                format!(
                                    "{}{rest}",
                                    source.holding.value_of(source.pointer.clone())
                                ),
                                source.copied,
                            ),
                        };
                        Some(Candidate {
                            copied,
                            host,
                            from,
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
        let places = written(language, text, &now);
        let now: Vec<Option<&str>> = places
            .iter()
            .map(|place| place.and_then(|place| slice::slice(text, place)))
            .collect();
        let mut was = vec![None; candidates.len()];
        // Typing since the text the editor wrote, which kept every key and
        // list: a scalar written wholly before or after the bytes it
        // changed is written as the editor wrote it, where it reads the
        // same (text joining it changes what it reads); one inside them
        // that reads otherwise was typed. Only the rest is read in the
        // text the editor wrote.
        let mut asked = vec![true; candidates.len()];
        let base_read = typed
            .as_ref()
            .map(|(base, _, _)| parse_cache::parsed(language, base));
        if let (Some((_, start, end)), Some(Ok(held))) =
            (&typed, base_read.as_ref().map(|read| read.as_ref()))
        {
            let changed_end = text_size(*end);
            for (index, (candidate, place)) in candidates.iter().zip(&places).enumerate() {
                let same = held.pointer(&candidate.at) == value.pointer(&candidate.pointer);
                let Some(place) = place else {
                    asked[index] = false;
                    continue;
                };
                let apart = place.end() <= text_size(*start) || place.start() >= changed_end;
                if !same || apart {
                    asked[index] = false;
                    if same {
                        was[index] = now[index].map(str::to_string);
                    }
                }
            }
        }
        note_many(
            Work::SaveItemJudged,
            asked.iter().filter(|asked| **asked).count(),
        );
        let mut texts: Vec<&Arc<str>> = Vec::new();
        for (candidate, _) in candidates.iter().zip(&asked).filter(|(_, asked)| **asked) {
            if !texts.iter().any(|text| Arc::ptr_eq(text, &candidate.shown)) {
                texts.push(&candidate.shown);
            }
        }
        for shown in texts {
            let at: Vec<usize> = (0..candidates.len())
                .filter(|at| asked[*at] && Arc::ptr_eq(&candidates[*at].shown, shown))
                .collect();
            let pointers: Vec<String> = at.iter().map(|at| candidates[*at].at.clone()).collect();
            // The text and what it reads as both: a scalar's span can
            // stay while text the user put below it joins it (a plain
            // YAML scalar running on).
            let read = parse_cache::parsed(language, shown);
            let held = read.as_ref().as_ref().ok();
            for (index, place) in at.into_iter().zip(written(language, shown, &pointers)) {
                let spelled = place.and_then(|place| slice::slice(shown, place));
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
                copied: candidate.copied,
            })
            .collect::<Vec<_>>()
            .into()
    }
}
