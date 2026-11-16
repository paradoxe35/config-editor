//! A `ConfigDocuments` with `echoes: true` and its host, which lists every
//! document from its store after each change. The list (`documents_from`'s
//! `with_protection`, then `take_listed` and `show`), `replaceValue`
//! (`keep_edits`) and the settle call each document's own channel
//! (`HostChannel`, as `src/documents_view*.rs` and `src/documents_list.rs`
//! do), with `protect::ranges_in` written over positions; the view's
//! entries (`documents/entries.rs`: the ranges set before the value) are
//! modelled.

use std::collections::HashMap;

use config_editor_leptos::TextRange;

use crate::host::{HostDoc, ranges_in};
use crate::host_channel::{Given, HostChannel};
use crate::model::{Act, Class, Core, Failure, Form, Outcome, ResetKind, read_ranges, user_edit};
use config_editor_leptos::js_support::{Echo, echo_clock as clock};

/// One document: the `documents` signal's copy, its entry, its editor.
struct Doc {
    id: String,
    /// The `documents` signal's value and ranges.
    value: String,
    ranges: Vec<TextRange>,
    /// The entry's (its value follows the editor's edits).
    entry_value: String,
    entry_ranges: Vec<TextRange>,
    /// Its editor, mounted once shown.
    core: Option<Core>,
    host: HostDoc,
}

impl Doc {
    /// `entries.rs`'s update: the ranges first (placed once the task is
    /// over, or with a new text), then the value (its editor takes it at
    /// once).
    fn sync(&mut self) -> Outcome {
        if self.entry_ranges != self.ranges {
            self.entry_ranges.clone_from(&self.ranges);
            if let Some(core) = &mut self.core {
                core.set_given(self.ranges.clone());
            }
        }
        if self.entry_value != self.value {
            self.value.clone_into(&mut self.entry_value);
            if let Some(core) = &mut self.core {
                core.host_value(&self.value)?;
            }
        }
        Ok(())
    }

    fn shown(&self) -> &str {
        self.core
            .as_ref()
            .map_or(&self.entry_value, |core| &core.text)
    }

    fn check(&self, over: Class, what: &str) -> Outcome {
        let what = format!("{} {what}", self.id);
        match &self.core {
            Some(core) => {
                let mut core = core.clone();
                core.flush()?;
                core.check(&what, Some(over))
            }
            // What its editor mounts with.
            None => Core::new(&self.entry_value, self.entry_ranges.clone())
                .and_then(|core| core.check(&what, Some(over))),
        }
    }
}

pub(crate) struct DocumentsSim {
    docs: Vec<Doc>,
    /// `State::texts`: each document's text as the view knows it.
    texts: HashMap<String, String>,
    /// `Kept::channels`.
    channels: HashMap<String, HostChannel>,
    active: usize,
    settle_armed: bool,
    form: Form,
}

impl DocumentsSim {
    pub(crate) fn new(bases: [&str; 2], form: Form) -> Result<Self, Failure> {
        let mut sim = Self {
            docs: Vec::new(),
            texts: HashMap::new(),
            channels: HashMap::new(),
            active: 0,
            settle_armed: false,
            form,
        };
        for (index, base) in bases.into_iter().enumerate() {
            let id = ["a", "b"][index].to_string();
            let (given, print) = ranges_in(base, form);
            let ranges = read_ranges(&given, base, None)?;
            sim.texts.insert(id.clone(), base.to_string());
            sim.channels.insert(
                id.clone(),
                HostChannel::new(true, base, Some(print.clone()), base, ranges.clone()),
            );
            let mut host = HostDoc::new(base);
            host.last_print = print;
            sim.docs.push(Doc {
                id,
                value: base.to_string(),
                ranges: ranges.clone(),
                entry_value: base.to_string(),
                entry_ranges: ranges.clone(),
                core: (index == 0).then(|| Core::new(base, ranges)).transpose()?,
                host,
            });
        }
        Ok(sim)
    }

    /// The host lists every document from its store; `own`: the document
    /// it gave a text of its own.
    fn list(&mut self, own: Option<usize>) -> Outcome {
        let flushed = clock();
        let form = self.form;
        // `with_protection`: each document's channel reads its ranges.
        let mut listed = Vec::new();
        for doc in &mut self.docs {
            doc.host.listed_store();
            doc.host.last_listed.clone_from(&doc.host.store);
            let value = doc.host.store.clone();
            let (given, print) = ranges_in(&value, form);
            let current = &self.texts[&doc.id];
            let ranges = self
                .channels
                .get_mut(&doc.id)
                .expect("listed before")
                .ranges(
                    Some(print),
                    Some(&value),
                    None,
                    current,
                    false,
                    |text, came_with| read_ranges(&given, text, Some(came_with)),
                )?;
            listed.push((value, ranges));
        }
        // `take_listed`.
        let mut resend = Vec::new();
        let mut settles = false;
        for (index, (value, ranges)) in listed.into_iter().enumerate() {
            let doc = &mut self.docs[index];
            let current = self.texts[&doc.id].clone();
            let channel = self.channels.get_mut(&doc.id).expect("listed before");
            let taken = channel.received(
                Given {
                    value: Some(&value),
                    revision: None,
                    ranges_for: None,
                    ranges: Some(ranges),
                    diagnostics: None,
                    flushed,
                    stale: false,
                },
                &current,
            );
            let applies = taken.shows();
            if applies && own != Some(index) && value != current {
                return Err(Failure::new(
                    doc.host.lost_by(&value),
                    format!("{}: list applied {value:?} over {current:?}", doc.id),
                ));
            }
            // `show`: `remember_texts`, then the signal, taken now; ranges
            // that stay are the ones its editor has.
            if let Some(ranges) = taken.ranges {
                doc.ranges = ranges;
            }
            doc.value = if applies { value } else { current.clone() };
            self.texts.insert(doc.id.clone(), doc.value.clone());
            match taken.echo {
                Some(Echo::Settle) => settles = true,
                Some(Echo::Resend) => {
                    settles = true;
                    resend.push((index, current));
                }
                _ => {}
            }
        }
        for doc in &mut self.docs {
            doc.sync()?;
        }
        // `tell_again`.
        for (index, text) in resend {
            self.docs[index].host.told(&text);
        }
        self.settle_armed |= settles;
        Ok(())
    }

    /// `replaceValue(id, value)`.
    fn replace_value(&mut self, index: usize, value: &str) -> Result<bool, Failure> {
        let id = self.docs[index].id.clone();
        if self.texts.get(&id).is_some_and(|known| known == value) {
            return Ok(false);
        }
        // `State::replaced`, from the text the view knows.
        let ranges = self.channels.get_mut(&id).and_then(|channel| {
            let known = self.texts.get(&id)?;
            channel.replaced(value, known)
        });
        self.texts.insert(id.clone(), value.to_string());
        // `keep_edits`, the new value and ranges, then the list taken now.
        for doc in &mut self.docs {
            if let Some(core) = &doc.core {
                core.text.clone_into(&mut doc.value);
            }
        }
        let doc = &mut self.docs[index];
        value.clone_into(&mut doc.value);
        if let Some(ranges) = ranges {
            doc.ranges = ranges;
        }
        for doc in &mut self.docs {
            doc.sync()?;
        }
        Ok(true)
    }

    /// `settle_all`.
    fn settle(&mut self) {
        if !std::mem::take(&mut self.settle_armed) {
            return;
        }
        for doc in &mut self.docs {
            let Some(text) = self.texts.get(&doc.id).cloned() else {
                continue;
            };
            if self
                .channels
                .get_mut(&doc.id)
                .is_some_and(|channel| channel.settle(&text))
            {
                doc.host.told(&text);
            }
        }
    }

    fn flush(&mut self) -> Outcome {
        for doc in &mut self.docs {
            if let Some(core) = &mut doc.core {
                core.flush()?;
            }
        }
        Ok(())
    }

    fn doc(&self, doc: u8) -> usize {
        doc as usize % self.docs.len()
    }

    pub(crate) fn step(&mut self, act: &Act) -> Outcome {
        if !act.same_task() {
            self.flush()?;
        }
        match act {
            Act::Type { .. } | Act::Delete { .. } | Act::Enter { .. } | Act::MoveLine { .. } => {
                let doc = &mut self.docs[self.active];
                if let Some(core) = &mut doc.core
                    && let Some((start, end, insert)) = user_edit(act, &core.text)
                    && core.edit(start, end, &insert)
                {
                    // The panel's `changed`, then the view's.
                    let text = core.text.clone();
                    text.clone_into(&mut doc.entry_value);
                    self.texts.insert(doc.id.clone(), text.clone());
                    if let Some(channel) = self.channels.get_mut(&doc.id) {
                        channel.sent(&text);
                    }
                    doc.host.told(&text);
                }
            }
            Act::Deliver { doc, pick, dup, .. } => {
                let index = self.doc(*doc);
                if let Some((text, listing)) = self.docs[index].host.take(*pick) {
                    // A listing renders the store; an echo is stored.
                    if !listing {
                        self.docs[index].host.store = text;
                    }
                    self.list(None)?;
                    if *dup {
                        self.list(None)?;
                    }
                }
            }
            Act::Drop { doc, pick } => {
                let index = self.doc(*doc);
                self.docs[index].host.retry(*pick);
            }
            Act::Reset { doc, kind, .. } => {
                let index = self.doc(*doc);
                let shown = self.docs[index].shown().to_string();
                for text in self.docs[index].host.resets(*kind, &shown) {
                    let shown = self.docs[index].shown().to_string();
                    let replaced = self.replace_value(index, &text)?;
                    let host = &mut self.docs[index].host;
                    host.reset(&text, &shown);
                    // `false`: the document is that text already; the host
                    // holds it once (its echo, if on its way), so it keeps
                    // its store and lists nothing.
                    if replaced {
                        text.clone_into(&mut host.store);
                        text.clone_into(&mut host.last_listed);
                        host.lists(&text);
                    }
                }
            }
            Act::Own { doc, edit, .. } => {
                let index = self.doc(*doc);
                let host = &mut self.docs[index].host;
                if let Some(text) = host.resets(ResetKind::NewVersion(*edit), "").pop() {
                    host.queue.clear();
                    host.store = text;
                    self.list(Some(index))?;
                }
            }
            Act::RangesFor { .. } => {}
            Act::Settle => self.settle(),
            Act::Select { doc } => {
                let index = self.doc(*doc);
                self.active = index;
                let doc = &mut self.docs[index];
                if doc.core.is_none() {
                    doc.core = Some(Core::new(&doc.entry_value, doc.entry_ranges.clone())?);
                }
            }
        }
        self.check(Class::OverLockInFlight, "after the step")
    }

    /// The state, for a trace (`HOST_SYNC_TRACE=1`).
    pub(crate) fn describe(&self) -> String {
        self.docs
            .iter()
            .map(|doc| {
                format!(
                    "  {} shown {:?} locks {:?} entry {:?} {:?}\n    host {:?} queue {:?}",
                    doc.id,
                    doc.shown(),
                    doc.core.as_ref().map(|core| core.locks.clone()),
                    doc.entry_value,
                    doc.entry_ranges,
                    doc.host.store,
                    doc.host.queue
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn check(&self, over: Class, what: &str) -> Outcome {
        self.docs.iter().try_for_each(|doc| doc.check(over, what))
    }

    /// Every text comes back, oldest first, and every pause runs out.
    pub(crate) fn drain(&mut self) -> Outcome {
        for _ in 0..500 {
            self.flush()?;
            if self.settle_armed {
                self.settle();
            } else if let Some(index) = self.docs.iter().position(|doc| !doc.host.queue.is_empty())
            {
                let Some((text, listing)) = self.docs[index].host.take(0) else {
                    break;
                };
                if !listing {
                    self.docs[index].host.store = text;
                }
                self.list(None)?;
            } else {
                break;
            }
        }
        self.flush()?;
        for doc in &self.docs {
            if doc.host.store != doc.shown() {
                return Err(Failure::new(
                    Class::NotConverged,
                    format!(
                        "{}: host holds {:?}, the editor {:?}",
                        doc.id,
                        doc.host.store,
                        doc.shown()
                    ),
                ));
            }
        }
        self.check(Class::OverLockAtRest, "at rest")
    }
}
