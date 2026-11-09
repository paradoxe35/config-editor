//! Keyed lists whose rows own themselves (see the module docs of
//! `reactive`).

use std::hash::Hash;

use leptos::prelude::*;
use leptos::tachys::reactive_graph::OwnedView;
use leptos::tachys::view::keyed::{SerializableKey, keyed};
use leptos::tachys::view::{Mountable, Render};

/// A new owner that no other owner records as its child: it lives for as
/// long as its holder keeps it, and is cleaned up when dropped.
///
/// `Owner::new` registers the owner with the current one, which keeps a
/// record of it (and its allocation) until it is cleaned up itself; under a
/// long-lived owner, one record per row ever shown adds up.
pub(crate) fn detached_owner() -> Owner {
    let Some(current) = Owner::current() else {
        return Owner::new();
    };
    current.clone().unset();
    let owner = Owner::new();
    current.set();
    owner
}

/// Mounts the view `f` builds into `parent`, as `leptos::mount::mount_to`
/// does, under an owner of its own that no other records
/// ([`detached_owner`]). Dropping the returned handle unmounts the view and
/// then cleans its owner up, at once: an owner left to go with its last
/// holder would live on in its effects' first runs, which would read what
/// the view was given after it is gone.
pub(crate) fn mount_detached<F, N>(parent: web_sys::HtmlElement, f: F) -> Box<dyn std::any::Any>
where
    F: FnOnce() -> N + 'static,
    N: IntoView + 'static,
{
    let owner = detached_owner();
    let mountable = owner.with(move || {
        let mut mountable = f().into_view().build();
        mountable.mount(&parent, None);
        mountable
    });
    Box::new(Detached { owner, mountable })
}

/// A view [`mount_detached`] mounted, with its owner.
struct Detached<M: Mountable> {
    owner: Owner,
    mountable: M,
}

impl<M: Mountable> Drop for Detached<M> {
    fn drop(&mut self) {
        self.mountable.unmount();
        self.owner.cleanup();
    }
}

/// `For`, with each row's owner held by the row alone: rows that come and
/// go (scrolled in and out, opened and closed) leave nothing behind in the
/// list's owner. Rows read no context.
#[component]
pub(crate) fn KeyedList<IF, I, T, EF, N, KF, K>(each: IF, key: KF, children: EF) -> impl IntoView
where
    IF: Fn() -> I + Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
    EF: Fn(T) -> N + Send + Clone + 'static,
    N: IntoView + 'static,
    KF: Fn(&T) -> K + Send + Clone + 'static,
    K: Eq + Hash + SerializableKey + 'static,
    T: Send + 'static,
{
    let row = move |_, item| {
        let owner = detached_owner();
        let view = owner.with(|| children(item));
        (drop, OwnedView::new_with_owner(view, owner))
    };
    move || keyed(each(), key.clone(), row.clone())
}
