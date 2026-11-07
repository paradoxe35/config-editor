//! Keyed lists whose rows own themselves (see the module docs of
//! `reactive`).

use std::hash::Hash;

use leptos::prelude::*;
use leptos::tachys::reactive_graph::OwnedView;
use leptos::tachys::view::keyed::{SerializableKey, keyed};

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
