//! A `ConfigEditor` with `echoes: true` and its host. `update` and
//! `replaceValue` are `src/editor.rs`'s, step for step, over the library's
//! own pieces (`Echoes`, `host_locks::{HostLocks, stay}`, `moved_between`,
//! `reads_as`), and `protect::ranges_in` written over positions; only the
//! JavaScript around them is left out (no edit is held back here, so
//! `told` and the flush change nothing). Keep the two in step when
//! `editor.rs` changes.

use config_editor_leptos::TextRange;

use crate::echoes::{Echo, Echoes, clock};
use crate::host::{HostDoc, ranges_in};
use crate::host_locks::{HostLocks, stay};
use crate::model::{Act, Class, Core, Failure, Form, Outcome, read_ranges, user_edit};
use crate::remap::{moved_between, reads_as};

pub(crate) struct EditorSim {
    core: Core,
    echoes: Echoes,
    /// The host's ranges as given (`ConfigEditor::locks`).
    locks: HostLocks,
    settle_armed: bool,
    host: HostDoc,
    form: Form,
    now: f64,
}

impl EditorSim {
    pub(crate) fn new(base: &str, form: Form) -> Result<Self, Failure> {
        let (ranges, print) = ranges_in(base, form);
        let mut echoes = Echoes::new(true);
        echoes.given(base);
        let mut host = HostDoc::new(base);
        host.last_print.clone_from(&print);
        Ok(Self {
            core: Core::new(base, ranges.clone())?,
            echoes,
            locks: HostLocks::given(Some(print), base, base, ranges),
            settle_armed: false,
            host,
            form,
            now: 0.0,
        })
    }

    /// `onChange` with the document: the host stores it later.
    fn tell_host(&mut self) {
        let text = self.core.text.clone();
        self.host.told(&text);
    }

    /// `update({ value?, readOnlyRanges?, rangesFor? })`; `own`: the host
    /// gives a text of its own (else it hands one back).
    fn update(
        &mut self,
        value: Option<String>,
        given: Option<(Vec<TextRange>, String)>,
        ranges_for: Option<String>,
        own: bool,
    ) -> Outcome {
        let flushed = clock();
        let latest = self.core.text.clone();
        let ranges_print = given.as_ref().map(|(_, print)| print.clone());
        let ranges_again = ranges_for.is_none()
            && given.is_some()
            && self.locks.again(ranges_print.as_deref()).is_some();
        let host_text = ranges_for
            .clone()
            .or_else(|| value.clone())
            .unwrap_or_else(|| latest.clone());
        let mut read_only_ranges = None;
        if let Some((ranges, _)) = &given {
            let mut replaces = || {
                value
                    .as_deref()
                    .is_some_and(|value| self.echoes.replaces(value, &latest))
            };
            if !ranges_again || !stay(true, replaces()) {
                read_only_ranges = Some(read_ranges(
                    ranges,
                    &host_text,
                    Some(self.locks.came_with()),
                )?);
            }
        }
        let echo = value.as_deref().map(|value| {
            self.echoes.at(self.now);
            self.echoes.received(value, &latest, flushed)
        });
        if std::env::var_os("HOST_SYNC_TRACE").is_some() {
            eprintln!("  update {value:?} over {latest:?}: {echo:?}");
        }
        let shown = value.clone().filter(|_| echo == Some(Echo::Apply));
        let before_reset = shown.is_none()
            && value
                .as_deref()
                .is_some_and(|value| self.echoes.before_reset(value));
        if before_reset {
            read_only_ranges = None;
        }
        if stay(ranges_again, shown.is_some()) {
            read_only_ranges = None;
        }
        {
            let document = shown.clone().unwrap_or_else(|| latest.clone());
            let host_text = ranges_for
                .clone()
                .or_else(|| value.clone())
                .unwrap_or_else(|| document.clone());
            if read_only_ranges.is_some() && host_text != document {
                read_only_ranges = moved_between(&host_text, &document, read_only_ranges, None).0;
            }
            if before_reset {
                self.locks.forget();
            } else if given.is_some() && read_only_ranges.is_none() {
                self.locks.came(&host_text);
            } else if let Some(ranges) = &read_only_ranges {
                self.locks = HostLocks::given(ranges_print, &host_text, &document, ranges.clone());
            } else if let Some(value) = shown.as_deref().filter(|value| !reads_as(value, &latest)) {
                read_only_ranges = Some(self.locks.replaced(value));
            }
        }
        // Its ranges before its text.
        if let Some(ranges) = read_only_ranges {
            self.core.set_given(ranges);
        }
        if let Some(value) = shown {
            if !own && value != latest {
                return Err(Failure::new(
                    self.host.lost_by(&value),
                    format!("update applied {value:?} over {latest:?}"),
                ));
            }
            self.core.host_value(&value)?;
        }
        if echo == Some(Echo::Resend) {
            self.tell_host();
        }
        if matches!(echo, Some(Echo::Resend | Echo::Settle)) {
            self.settle_armed = true;
        }
        Ok(())
    }

    /// `replaceValue(value)`: whether it replaced the document.
    fn replace_value(&mut self, value: &str) -> Result<bool, Failure> {
        if value == self.core.text {
            return Ok(false);
        }
        self.echoes.replaced(value);
        if !reads_as(value, &self.core.text) {
            let ranges = self.locks.replaced(value);
            self.core.set_given(ranges);
        }
        self.core.host_value(value)?;
        Ok(true)
    }

    fn settle(&mut self) {
        if std::mem::take(&mut self.settle_armed) {
            let current = self.core.text.clone();
            if self.echoes.settle(&current) {
                self.tell_host();
            }
        }
    }

    fn deliver(&mut self, pick: u16, dup: bool, ranges: bool) -> Outcome {
        let Some((text, _)) = self.host.take(pick) else {
            return Ok(());
        };
        text.clone_into(&mut self.host.store);
        text.clone_into(&mut self.host.last_listed);
        for _ in 0..if dup { 2 } else { 1 } {
            let given = ranges.then(|| {
                self.host.gives_ranges(&text, self.form, true);
                ranges_in(&text, self.form)
            });
            self.update(Some(text.clone()), given, None, false)?;
        }
        Ok(())
    }

    pub(crate) fn step(&mut self, act: &Act) -> Outcome {
        if !act.same_task() {
            self.core.flush()?;
        }
        self.now += 10.0;
        match act {
            Act::Type { .. } | Act::Delete { .. } | Act::Enter { .. } | Act::MoveLine { .. } => {
                if let Some((start, end, insert)) = user_edit(act, &self.core.text)
                    && self.core.edit(start, end, &insert)
                {
                    let text = self.core.text.clone();
                    self.echoes.sent(&text);
                    self.tell_host();
                }
            }
            Act::Deliver {
                pick, dup, ranges, ..
            } => self.deliver(*pick, *dup, *ranges)?,
            Act::Drop { pick, .. } => self.host.retry(*pick),
            Act::Reset { kind, .. } => {
                for text in self.host.resets(*kind, &self.core.text) {
                    let shown = self.core.text.clone();
                    let replaced = self.replace_value(&text)?;
                    self.host.reset(&text, &shown);
                    // `false`: the document is that text already; the host
                    // holds it once (its echo, if on its way), so it keeps
                    // its store and lists nothing.
                    if replaced {
                        text.clone_into(&mut self.host.store);
                        text.clone_into(&mut self.host.last_listed);
                        self.host.lists(&text);
                    }
                }
            }
            Act::Own { edit, ranges, .. } => {
                let resets = self
                    .host
                    .resets(crate::model::ResetKind::NewVersion(*edit), "");
                if let Some(text) = resets.into_iter().next() {
                    self.host.queue.clear();
                    text.clone_into(&mut self.host.store);
                    text.clone_into(&mut self.host.last_listed);
                    let given = self
                        .host
                        .gives_ranges(&text, self.form, *ranges)
                        .then(|| ranges_in(&text, self.form));
                    self.update(Some(text), given, None, true)?;
                }
            }
            Act::RangesFor { .. } => {
                let store = self.host.store.clone();
                self.host.gives_ranges(&store, self.form, true);
                let given = ranges_in(&store, self.form);
                self.update(None, Some(given), Some(store), false)?;
            }
            Act::Settle => self.settle(),
            Act::Select { .. } => {}
        }
        self.check(Class::OverLockInFlight, "after the step")
    }

    /// The state, for a trace (`HOST_SYNC_TRACE=1`).
    pub(crate) fn describe(&self) -> String {
        format!(
            "  editor {:?} locks {:?} given {:?}\n  host {:?} queue {:?}",
            self.core.text, self.core.locks, self.core.given, self.host.store, self.host.queue
        )
    }

    /// The locks as the next key finds them (the task over).
    fn check(&self, over: Class, what: &str) -> Outcome {
        let mut core = self.core.clone();
        core.flush()?;
        core.check(what, Some(over))
    }

    /// Every text comes back, oldest first, and every pause runs out: the
    /// host and the editor agree, and the lock is the host's exactly.
    pub(crate) fn drain(&mut self) -> Outcome {
        for _ in 0..500 {
            self.core.flush()?;
            self.now += 10.0;
            if self.settle_armed {
                self.settle();
            } else if !self.host.queue.is_empty() {
                self.deliver(0, false, true)?;
            } else {
                break;
            }
        }
        self.core.flush()?;
        if self.host.store != self.core.text {
            return Err(Failure::new(
                Class::NotConverged,
                format!(
                    "host holds {:?}, the editor {:?}",
                    self.host.store, self.core.text
                ),
            ));
        }
        self.check(Class::OverLockAtRest, "at rest")
    }
}
