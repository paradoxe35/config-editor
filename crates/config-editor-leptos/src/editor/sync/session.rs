//! [`HostSession`]: the host's text and the text on screen that stands for
//! it, one record that each event sets whole.

use std::sync::Arc;

use leptos::prelude::*;

use super::SyncState;
use crate::editor::text::same_or_new;

/// A host text, shared by the records that name it.
pub(crate) type HostText = Arc<str>;

/// The host's text and the text on screen that stands for it.
#[derive(Clone, Debug)]
pub(crate) struct HostSession {
    /// The host's last text: the one it gave last, or was sent last.
    pub(crate) host: HostText,
    /// The text on screen, in the language on screen, that `host` stands
    /// for (`host` itself in the host's language).
    pub(crate) view: HostText,
    /// The host text the edit session began on: the last one the host gave,
    /// or one written in the host's language, not one an Edit-as save
    /// wrote. Its styles outlast the session's saves (see
    /// `Bridge::to_host_since`).
    pub(crate) origin: HostText,
    /// Another text on screen known to stand for a host text: the one a
    /// switch left (the switch's undo step records it), a step undone or
    /// redone back to a text saved before, a reloaded baseline.
    known: Option<(HostText, HostText)>,
}

impl HostSession {
    /// A session on the host's `host`, shown as `view`: one copy when they
    /// are the same text.
    pub(crate) fn shared(host: HostText, view: HostText) -> Self {
        Self {
            view: if *view == *host { host.clone() } else { view },
            origin: host.clone(),
            host,
            known: None,
        }
    }

    /// The host text `view`, a text on screen, stands for, and the
    /// session's copy of `view`, when known.
    pub(crate) fn saved_pair(&self, view: &str) -> Option<(HostText, HostText)> {
        if *self.view == *view {
            return Some((self.host.clone(), self.view.clone()));
        }
        self.known
            .as_ref()
            .filter(|(known, _)| **known == *view)
            .map(|(known, host)| (host.clone(), known.clone()))
    }

    /// The host text `view`, a text on screen, stands for, when known.
    pub(crate) fn saved_as(&self, view: &str) -> Option<HostText> {
        if *self.view == *view {
            return Some(self.host.clone());
        }
        self.known
            .as_ref()
            .filter(|(known, _)| **known == *view)
            .map(|(_, host)| host.clone())
    }
}

/// Read access to the host's last text (the session's `host`), for the
/// parts that read it: the session alone sets it.
#[derive(Clone, Copy)]
pub(crate) struct HostTextReader(pub(super) StoredValue<HostSession>);

impl HostTextReader {
    /// Runs `read` with the host's last text.
    pub(crate) fn with_value<T>(&self, read: impl FnOnce(&str) -> T) -> T {
        self.0.with_value(|session| read(&session.host))
    }

    /// The host's last text.
    pub(crate) fn get_value(&self) -> String {
        self.with_value(str::to_string)
    }
}

impl SyncState {
    /// Whether `text` is the text on screen that stands for the host's last
    /// text (nothing to save).
    pub(crate) fn is_synced(&self, text: &str) -> bool {
        self.session.with_value(|session| *session.view == *text)
    }

    /// The host text the session began on.
    pub(crate) fn origin(&self) -> String {
        self.session
            .with_value(|session| session.origin.to_string())
    }

    /// The host text the text on screen `view` stands for, when known.
    pub(crate) fn saved_as(&self, view: &str) -> Option<HostText> {
        self.session.with_value(|session| session.saved_as(view))
    }

    fn set(&self, change: impl FnOnce(&mut HostSession)) {
        self.session.update_value(change);
        self.session
            .with_value(|session| super::super::parse_cache::host_text(&session.host));
        self.synced.notify();
    }

    /// The host gave `host`, shown as `view`: a new session.
    pub(crate) fn received(&self, host: &str, view: &str) {
        let host = HostText::from(host);
        let view = same_or_new(&host, view);
        self.set(|session| *session = HostSession::shared(host, view));
    }

    /// The text on screen `view` was saved as `host`, sent to the host
    /// (each the editor's copy, see `EditorState::copy_of`).
    pub(crate) fn saved(&self, host: HostText, view: HostText) {
        self.set(|session| {
            session.view = if *view == *host { host.clone() } else { view };
            session.host = host;
        });
    }

    /// `text`, typed in the host's language, reached the host: it is the
    /// host's own text, and a view's next save patches it. `text` is the
    /// document's copy: the session holds that one.
    pub(crate) fn typed_in_host_language(&self, text: &HostText) {
        if self
            .session
            .with_value(|session| *session.host == **text && *session.view == **text)
        {
            // The same texts: shared, no news.
            self.session.update_value(|session| {
                if !Arc::ptr_eq(&session.host, text) {
                    let origin = Arc::ptr_eq(&session.origin, &session.host);
                    session.host = text.clone();
                    if origin {
                        session.origin = text.clone();
                    }
                }
                session.view = text.clone();
            });
            super::super::parse_cache::host_text(text);
            return;
        }
        self.set(|session| *session = HostSession::shared(text.clone(), text.clone()));
    }

    /// A switch with nothing unsaved shows the host's text as `view`; the
    /// text it left stands for the host's text still.
    pub(crate) fn switched(&self, view: &str) {
        self.set(|session| {
            let view = same_or_new(&session.host, view);
            let left = std::mem::replace(&mut session.view, view);
            session.known = Some((left, session.host.clone()));
        });
    }

    /// The text on screen `view` is known to stand for `host` (an undo step
    /// back to a text saved, a reloaded baseline): its save sends `host`.
    pub(crate) fn restored(&self, view: &str, host: HostText) {
        self.session
            .update_value(|session| session.known = Some((HostText::from(view), host)));
    }
}
