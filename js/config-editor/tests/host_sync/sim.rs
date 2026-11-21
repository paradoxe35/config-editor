//! A simulated view and its host: a `ConfigEditor` (one document) or a
//! `ConfigDocuments` (two, the user editing the one shown). Each document
//! goes through its record of its host as `src/editor.rs`,
//! `src/documents_list.rs` and `src/documents_view/echoing.rs` drive it:
//! the lists read in the text the record says they name (`anchors`), then
//! the whole update taken by the core's one entry. Only the JavaScript
//! around it is left out (no edit is held back here, so the flush changes
//! nothing).

use leptos_config_editor::RangesFor;
use leptos_config_editor::js_support::{
    Echo, HostChannel, HostUpdate, List, Listed, Reading, echo_clock as clock, statement_of,
};

use crate::host::{HostDoc, Queued};
use crate::model::{
    Act, Anchor, Class, Core, Failure, Form, Outcome, ResetKind, Via, host_lock, read_in, user_edit,
};

/// A list the host gives: the text it names (`rangesFor`) and the text it
/// worked it out in.
#[derive(Clone, Debug)]
struct Given {
    anchor: Option<RangesFor>,
    worked_on: String,
}

/// One document, its editor and its host.
pub(crate) struct DocSim {
    core: Core,
    host: HostDoc,
    form: Form,
    settle_armed: bool,
    /// The list the host gave last (a re-mount renders it again).
    last: Option<Given>,
    /// A text of the host's own came with no list since the last one: the
    /// locks were carried into it by the safe diff, which may lock whole
    /// changed lines (the growth the rules allow then).
    carried: bool,
    /// The user typed since the last hide and show (a late echo of the
    /// editor that is gone then replaces typing; else it brings the typing
    /// the gone editor had back).
    typed_since_fresh: bool,
    /// The editor shows the text a late echo of the editor a hide and show
    /// replaced brought back (no typing of this editor's): a newer text of
    /// the host's replacing it loses nothing typed here.
    brought_back: bool,
    /// A late text of the editor a hide and show replaced was shown as the
    /// host's own (the fresh editor cannot know it for an echo): until
    /// every text on its way came back, what replaces typing then is that
    /// same open finding (qa39 39b), whatever text does it.
    gone_text_shown: bool,
}

impl DocSim {
    fn new(base: &str, form: Form, revisions: bool) -> Result<Self, Failure> {
        let mut sim = Self {
            core: Core {
                text: base.to_string(),
                channel: HostChannel::new(true, base, None),
            },
            host: HostDoc::new(base, revisions),
            form,
            settle_armed: false,
            last: None,
            carried: false,
            typed_since_fresh: true,
            brought_back: false,
            gone_text_shown: false,
        };
        let given = Given {
            anchor: None,
            worked_on: base.to_string(),
        };
        sim.update(Some(base.to_string()), None, Some(given), true, None)?;
        Ok(sim)
    }

    /// Whether the host names its texts (revisions, anchors): no lock may
    /// grow beside its text then.
    fn exact(&self) -> bool {
        self.host.revisions
    }

    /// `onChange` with the document and its revision: the host stores it
    /// later.
    fn tell_host(&mut self) {
        let text = self.core.text.clone();
        let revision = self.core.channel.sent(&text);
        self.host.told(&text, Some(revision));
    }

    /// The editor sent the document again (recorded by the record).
    fn told_again(&mut self) {
        let text = self.core.text.clone();
        let revision = self.core.channel.last_sent();
        self.host.told(&text, revision);
    }

    /// `update({ value?, valueRevision?, readOnlyRanges?, rangesFor? })`;
    /// `own`: the host gives a text of its own (else it hands one back,
    /// `queued`).
    fn update(
        &mut self,
        value: Option<String>,
        revision: Option<u64>,
        given: Option<Given>,
        own: bool,
        queued: Option<&Queued>,
    ) -> Outcome {
        let latest = self.core.text.clone();
        if let Some(value) = &value {
            self.host.gave(value);
        }
        let mut update = HostUpdate {
            value: value.clone(),
            value_revision: revision.filter(|_| self.host.revisions),
            flushed: Some(clock()),
            ..HostUpdate::default()
        };
        if let Some(given) = &given {
            update.ranges_for.clone_from(&given.anchor);
            let lock = host_lock(&given.worked_on, self.form);
            let print = lock
                .as_ref()
                .map_or_else(|| "[]".to_string(), |lock| lock.print.clone());
            let stated = statement_of(&print);
            update.ranges = Some(Listed::stated(Vec::new(), stated));
            let items = match self.core.channel.reading(List::Ranges, &update, &latest) {
                Ok(Reading::Again) => Vec::new(),
                Ok(Reading::In(text)) => {
                    if *text != *given.worked_on {
                        return Err(Failure::new(
                            Class::LockOnOtherText,
                            format!(
                                "ranges worked out on {:?} read in {text:?} (named by {:?})",
                                given.worked_on, given.anchor
                            ),
                        ));
                    }
                    match &lock {
                        Some(lock) => match read_in(lock, &text, self.form) {
                            Some(range) => vec![range],
                            None => {
                                return Err(Failure::new(
                                    Class::PastEnd,
                                    format!("{lock:?} read in {text:?}"),
                                ));
                            }
                        },
                        None => Vec::new(),
                    }
                }
                Err(error) if error.is_warning() => Vec::new(),
                Err(error) => return Err(Failure::new(Class::AnchorRefused, error.to_string())),
            };
            update.ranges = Some(Listed::stated(items, stated));
            self.last = Some(given.clone());
        }
        let listed = given.is_some();
        let taken = self
            .core
            .channel
            .take_unmounted(update, &latest)
            .map_err(|error| Failure::new(Class::AnchorRefused, error.to_string()))?;
        if std::env::var_os("HOST_SYNC_TRACE").is_some() {
            eprintln!("  update {value:?} over {latest:?}: {taken:?}");
        }
        if !taken.warnings.is_empty() && taken.ranges {
            return Err(Failure::new(
                Class::AnchorOutOfDateTaken,
                format!("{:?} with {taken:?}", taken.warnings),
            ));
        }
        if taken.ranges && listed {
            self.carried = false;
        } else if taken.shown {
            self.carried = true;
        }
        if taken.shown
            && let Some(value) = value
        {
            let typing_back =
                queued.is_some_and(|queued| queued.before_fresh) && !self.typed_since_fresh;
            if !own && value != latest && !typing_back && !self.brought_back {
                return Err(Failure::new(
                    if self.gone_text_shown {
                        Class::TypingLostAfterHideShow
                    } else {
                        self.host.lost_by(queued, &value)
                    },
                    format!("update applied {value:?} over {latest:?}"),
                ));
            }
            self.core.text = value;
            self.brought_back = typing_back;
            if queued.is_some_and(|queued| queued.before_fresh) {
                self.gone_text_shown = true;
            }
        }
        if taken.echo == Some(Echo::Resend) {
            self.told_again();
        }
        if matches!(taken.echo, Some(Echo::Resend | Echo::Settle)) {
            self.settle_armed = true;
        }
        Ok(())
    }

    /// `replaceValue(value)`: whether it replaced the document.
    fn replace_value(&mut self, value: &str) -> Result<bool, Failure> {
        let taken = self
            .core
            .channel
            .take_unmounted(
                HostUpdate {
                    value: Some(value.to_string()),
                    reset: true,
                    ..HostUpdate::default()
                },
                &self.core.text,
            )
            .map_err(|error| Failure::new(Class::Panic, error.to_string()))?;
        if taken.shown {
            value.clone_into(&mut self.core.text);
            self.carried = true;
        }
        Ok(taken.shown)
    }

    fn settle(&mut self) {
        if std::mem::take(&mut self.settle_armed) {
            let current = self.core.text.clone();
            if self.core.channel.settle(&current) {
                self.told_again();
            }
        }
    }

    /// The anchor a host names `text` (sent at `revision`) by.
    fn anchor(&self, anchor: Anchor, text: &str, revision: Option<u64>) -> Option<RangesFor> {
        match (anchor, revision) {
            (Anchor::None, _) => None,
            (Anchor::Revision, Some(revision)) if self.host.revisions => {
                Some(RangesFor::Revision(revision))
            }
            _ => Some(RangesFor::Text(text.into())),
        }
    }

    fn deliver(&mut self, pick: u16, dup: bool, ranges: bool, anchor: Anchor) -> Outcome {
        let Some(queued) = self.host.take(pick) else {
            return Ok(());
        };
        // A host keeping revisions never goes back to an older one.
        if !self.host.hold(&queued.text, queued.revision) {
            return Ok(());
        }
        queued.text.clone_into(&mut self.host.last_listed);
        let revision = queued.revision;
        for _ in 0..if dup { 2 } else { 1 } {
            let given = ranges.then(|| Given {
                anchor: self.anchor(anchor, &queued.text, revision),
                worked_on: queued.text.clone(),
            });
            self.update(
                Some(queued.text.clone()),
                revision,
                given,
                false,
                Some(&queued),
            )?;
        }
        if self.host.queue.is_empty() {
            self.gone_text_shown = false;
        }
        Ok(())
    }

    fn ranges(&mut self, anchor: Anchor, worked_on: u16) -> Outcome {
        let (text, revision) = match self.host.peek(worked_on).filter(|_| worked_on % 2 == 1) {
            Some(queued) if anchor != Anchor::None => (queued.text.clone(), queued.revision),
            _ => (self.host.store.clone(), self.host.store_revision),
        };
        if anchor == Anchor::None {
            // Named by the value it comes with: the store, rendered again
            // (the listings still queued are older).
            self.host.listed_store();
            let given = Given {
                anchor: None,
                worked_on: text.clone(),
            };
            return self.update(Some(text), revision, Some(given), false, None);
        }
        let given = Given {
            anchor: self.anchor(anchor, &text, revision),
            worked_on: text,
        };
        self.update(None, None, Some(given), false, None)
    }

    fn reset(&mut self, kind: ResetKind, via: Via) -> Outcome {
        let via = if via == Via::Both && !self.host.revisions {
            Via::SetValue
        } else {
            via
        };
        for text in self.host.resets(kind, &self.core.text) {
            let shown = self.core.text.clone();
            let replaced = self.replace_value(&text)?;
            self.host.reset(&text, &shown);
            // `false`: the document is that text already; the host holds it
            // once (its echo, if on its way), so it keeps its store and
            // lists nothing.
            if !replaced {
                continue;
            }
            match via {
                Via::ReplaceValue => {
                    // A host keeping revisions lists it with the revision the
                    // reset was recorded with (`sentRevision`): it never goes
                    // back to an older one.
                    let revision = self
                        .core
                        .channel
                        .latest_revision()
                        .filter(|_| self.host.revisions);
                    self.host.hold(&text, revision);
                    text.clone_into(&mut self.host.last_listed);
                    self.host.lists(&text, revision);
                }
                Via::SetValue => {
                    let revision = self.core.channel.latest_revision();
                    self.host.told(&text, revision);
                }
                Via::Both => {
                    let revision = self.core.channel.latest_revision();
                    self.host.told(&text, revision);
                    self.host.hold(&text, None);
                    text.clone_into(&mut self.host.last_listed);
                    // Its list rendered with it: worked out on that text, or
                    // naming the text it was worked out on.
                    let given = self.last.clone().map(|given| match given.anchor {
                        None => Given {
                            anchor: None,
                            worked_on: text.clone(),
                        },
                        Some(_) => given,
                    });
                    self.update(Some(text.clone()), None, given, true, None)?;
                }
            }
        }
        Ok(())
    }

    fn own(&mut self, edit: u16, ranges: bool) -> Outcome {
        let resets = self.host.resets(ResetKind::NewVersion(edit), "");
        if let Some(text) = resets.into_iter().next() {
            self.host.own(&text);
            text.clone_into(&mut self.host.last_listed);
            let given = ranges.then(|| Given {
                anchor: None,
                worked_on: text.clone(),
            });
            self.update(Some(text), None, given, true, None)?;
        }
        Ok(())
    }

    /// A re-mount: the channel goes on, the new editor takes the document
    /// and the list the host renders again.
    fn remount(&mut self) -> Outcome {
        self.core.channel.remount(&self.core.text);
        let Some(last) = self.last.clone() else {
            return Ok(());
        };
        // The binding renders the list it has again, worked out on the
        // parent's value, named by it when the list names no text.
        let given = match last.anchor {
            None => Given {
                anchor: Some(RangesFor::Text(self.host.store.as_str().into())),
                worked_on: self.host.store.clone(),
            },
            Some(_) => last,
        };
        self.update(None, None, Some(given), false, None)
    }

    /// A hide and show: a fresh editor on the host's store (with its
    /// revision), its list worked out on that text.
    fn fresh_mount(&mut self) -> Outcome {
        self.host.fresh_mount();
        let store = self.host.store.clone();
        let revision = self.host.store_revision.filter(|_| self.host.revisions);
        self.core = Core {
            text: store.clone(),
            channel: HostChannel::new(true, &store, revision),
        };
        self.settle_armed = false;
        self.typed_since_fresh = false;
        let given = Given {
            anchor: revision.map(RangesFor::Revision),
            worked_on: store.clone(),
        };
        self.update(Some(store), revision, Some(given), true, None)
    }

    /// The locks as the next key finds them.
    fn check(&self, what: &str, over: Class) -> Outcome {
        if self.carried {
            return self.core.check(what, None);
        }
        let class = if self.exact() {
            Class::LockOnOtherText
        } else {
            over
        };
        self.core.check(what, Some(class))
    }

    fn drain(&mut self) -> Outcome {
        for _ in 0..500 {
            if self.settle_armed {
                self.settle();
            } else if !self.host.queue.is_empty() {
                let anchor = if self.host.revisions {
                    Anchor::Revision
                } else {
                    Anchor::None
                };
                self.deliver(0, false, true, anchor)?;
            } else {
                break;
            }
        }
        if self.host.store != self.core.text {
            return Err(Failure::new(
                Class::not_converged(self.host.holds_given_again),
                format!(
                    "host holds {:?}, the editor {:?}",
                    self.host.store, self.core.text
                ),
            ));
        }
        self.check("at rest", Class::OverLockAtRest)
    }

    fn describe(&self) -> String {
        format!(
            "  editor {:?} locks {:?}\n  host {:?} at {:?} queue {:?}",
            self.core.text,
            self.core.locks(),
            self.host.store,
            self.host.store_revision,
            self.host.queue
        )
    }
}

/// A view: one document (a `ConfigEditor`) or two (a `ConfigDocuments`).
pub(crate) struct Sim {
    docs: Vec<DocSim>,
    active: usize,
}

impl Sim {
    pub(crate) fn new(bases: &[&str], form: Form, revisions: bool) -> Result<Self, Failure> {
        let docs = bases
            .iter()
            .map(|base| DocSim::new(base, form, revisions))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { docs, active: 0 })
    }

    fn doc(&mut self, doc: u8) -> &mut DocSim {
        let at = doc as usize % self.docs.len();
        &mut self.docs[at]
    }

    pub(crate) fn step(&mut self, act: &Act) -> Outcome {
        match act {
            Act::Type { .. } | Act::Delete { .. } | Act::Enter { .. } | Act::MoveLine { .. } => {
                let doc = &mut self.docs[self.active];
                if let Some((start, end, insert)) = user_edit(act, &doc.core.text)
                    && doc.core.edit(start, end, &insert)
                {
                    doc.typed_since_fresh = true;
                    doc.brought_back = false;
                    doc.tell_host();
                }
            }
            Act::Deliver {
                doc,
                pick,
                dup,
                ranges,
                anchor,
                ..
            } => self.doc(*doc).deliver(*pick, *dup, *ranges, *anchor)?,
            Act::Drop { doc, pick } => self.doc(*doc).host.retry(*pick),
            Act::Reset { doc, kind, via, .. } => self.doc(*doc).reset(*kind, *via)?,
            Act::Own {
                doc, edit, ranges, ..
            } => self.doc(*doc).own(*edit, *ranges)?,
            Act::Ranges {
                doc,
                anchor,
                worked_on,
                ..
            } => self.doc(*doc).ranges(*anchor, *worked_on)?,
            Act::Settle => {
                for doc in &mut self.docs {
                    doc.settle();
                }
            }
            Act::Select { doc } => self.active = *doc as usize % self.docs.len(),
            Act::Remount { doc } => self.doc(*doc).remount()?,
            Act::FreshMount { doc } => self.doc(*doc).fresh_mount()?,
        }
        for (at, doc) in self.docs.iter().enumerate() {
            doc.check(
                &format!("document {at} after the step"),
                Class::OverLockInFlight,
            )?;
        }
        Ok(())
    }

    /// Every text comes back, oldest first, and every pause runs out: the
    /// host and the editor agree, and the lock is the host's exactly.
    pub(crate) fn drain(&mut self) -> Outcome {
        for doc in &mut self.docs {
            doc.drain()?;
        }
        Ok(())
    }

    /// The state, for a trace (`HOST_SYNC_TRACE=1`).
    pub(crate) fn describe(&self) -> String {
        self.docs
            .iter()
            .map(DocSim::describe)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
