//! A `ConfigEditor` with `echoes: true` and its host. `update` and
//! `replaceValue` call the editor's own channel (`HostChannel`, as
//! `src/editor.rs` does), with `protect::ranges_in` written over
//! positions; only the JavaScript around it is left out (no edit is held
//! back here, so `told` and the flush change nothing).

use leptos_config_editor::TextRange;

use crate::host::{HostDoc, ranges_in};
use crate::host_channel::{Given, HostChannel};
use crate::model::{Act, Class, Core, Failure, Form, Outcome, ResetKind, read_ranges, user_edit};
use leptos_config_editor::js_support::{Echo, echo_clock as clock};

pub(crate) struct EditorSim {
    core: Core,
    /// `ConfigEditor::channel`.
    channel: HostChannel,
    settle_armed: bool,
    host: HostDoc,
    form: Form,
}

impl EditorSim {
    pub(crate) fn new(base: &str, form: Form) -> Result<Self, Failure> {
        let (ranges, print) = ranges_in(base, form);
        let mut host = HostDoc::new(base);
        host.last_print.clone_from(&print);
        Ok(Self {
            core: Core::new(base, ranges.clone())?,
            channel: HostChannel::new(true, base, Some(print), base, ranges),
            settle_armed: false,
            host,
            form,
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
        if let Some(value) = &value {
            self.host.gave(value);
        }
        let latest = self.core.text.clone();
        let ranges = given
            .map(|(ranges, print)| {
                self.channel.ranges(
                    Some(print),
                    value.as_deref(),
                    ranges_for.as_deref(),
                    &latest,
                    false,
                    |text, came_with| read_ranges(&ranges, text, came_with),
                )
            })
            .transpose()?;
        let taken = self.channel.received(
            Given {
                value: value.as_deref(),
                revision: None,
                ranges_for: ranges_for.as_deref(),
                ranges,
                diagnostics: None,
                flushed,
                stale: false,
            },
            &latest,
        );
        if std::env::var_os("HOST_SYNC_TRACE").is_some() {
            eprintln!("  update {value:?} over {latest:?}: {taken:?}");
        }
        let echo = taken.echo;
        let shown = value.filter(|_| echo == Some(Echo::Apply));
        // Its ranges before its text.
        if let Some(ranges) = taken.ranges {
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
        if let Some(ranges) = self.channel.replaced(value, &self.core.text) {
            self.core.set_given(ranges);
        }
        self.core.host_value(value)?;
        Ok(true)
    }

    fn settle(&mut self) {
        if std::mem::take(&mut self.settle_armed) {
            let current = self.core.text.clone();
            if self.channel.settle(&current) {
                self.tell_host();
            }
        }
    }

    fn deliver(&mut self, pick: u16, dup: bool, ranges: bool) -> Outcome {
        let Some((text, _)) = self.host.take(pick) else {
            return Ok(());
        };
        self.host.hold(&text);
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
        match act {
            Act::Type { .. } | Act::Delete { .. } | Act::Enter { .. } | Act::MoveLine { .. } => {
                if let Some((start, end, insert)) = user_edit(act, &self.core.text)
                    && self.core.edit(start, end, &insert)
                {
                    let text = self.core.text.clone();
                    self.channel.sent(&text);
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
                        self.host.hold(&text);
                        text.clone_into(&mut self.host.last_listed);
                        self.host.lists(&text);
                    }
                }
            }
            Act::Own { edit, ranges, .. } => {
                let resets = self.host.resets(ResetKind::NewVersion(*edit), "");
                if let Some(text) = resets.into_iter().next() {
                    self.host.own(&text);
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
                Class::not_converged(self.host.holds_given_again),
                format!(
                    "host holds {:?}, the editor {:?}",
                    self.host.store, self.core.text
                ),
            ));
        }
        self.check(Class::OverLockAtRest, "at rest")
    }
}
