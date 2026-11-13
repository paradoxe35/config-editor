//! Keyed lists whose rows own themselves (see the module docs of
//! `reactive`).

use std::hash::Hash;

use leptos::either::Either;
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

/// The rows a list that may show many at once makes room for (see
/// [`KeyedList`]'s `reserve`): the rows in view with overscan.
pub(crate) const MANY_ROWS: usize = 128;

/// Tasks the executor's queue is prepared for, the first time a list with
/// many rows is built. Each effect a new row makes queues its first run;
/// a whole screenful replaced at once queues over a thousand together.
#[cfg(target_arch = "wasm32")]
const QUEUED_TASKS: usize = 4096;

/// Makes the executor's queue big enough for [`QUEUED_TASKS`] tasks, once.
/// Its capacity grows by doubling and is never given back, so it would grow
/// in whichever cycle first queued more than it held, a place that moves
/// with the speed of the machine: memory that grows once, but at a time
/// nobody can name.
#[cfg(target_arch = "wasm32")]
fn prepare_queue() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|done| done.replace(true)) {
        return;
    }
    for _ in 0..QUEUED_TASKS {
        // Tasks that do nothing, run before anything else can: no owner to
        // wait for.
        #[allow(clippy::disallowed_methods)]
        leptos::task::spawn_local(async {});
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prepare_queue() {}

/// What a list shows when it is first built with room for more rows than
/// it has: its rows, then placeholders (see [`KeyedList`]).
enum Row<T> {
    Item(T),
    Pad(usize),
}

#[derive(PartialEq, Eq, Hash)]
enum Slot<K> {
    Item(K),
    Pad(usize),
}

/// `For`, with each row's owner held by the row alone: rows that come and
/// go (scrolled in and out, opened and closed) leave nothing behind in the
/// list's owner. Rows read no context.
///
/// A list that may show many rows at once passes `reserve` (see
/// [`MANY_ROWS`]). The record a keyed view keeps of its rows is a `Vec`
/// that a change which adds rows grows to the old rows and the new ones
/// together, by doubling, and never shrinks; a screenful replaced at once
/// (a jump, a slow machine's wheel steps coalesced) sets its size, a new
/// high every time one is bigger than any before. So the list is first
/// built with `reserve` rows, the real ones and then empty placeholders
/// that the first change removes: the record's size is then reached at the
/// start and not when someone first scrolls a long way.
#[component]
pub(crate) fn KeyedList<IF, I, T, EF, N, KF, K>(
    each: IF,
    key: KF,
    children: EF,
    #[prop(optional)] reserve: usize,
) -> impl IntoView
where
    IF: Fn() -> I + Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
    I::IntoIter: Send,
    EF: Fn(T) -> N + Send + Clone + 'static,
    N: IntoView + 'static,
    KF: Fn(&T) -> K + Send + Clone + 'static,
    K: Eq + Hash + SerializableKey + 'static,
    T: Send + 'static,
{
    let row = move |_, row| match row {
        Row::Item(item) => {
            let owner = detached_owner();
            let view = owner.with(|| children(item));
            (
                drop as fn(usize),
                Either::Left(OwnedView::new_with_owner(view, owner)),
            )
        }
        Row::Pad(_) => (drop as fn(usize), Either::Right(())),
    };
    let key = move |row: &Row<T>| match row {
        Row::Item(item) => Slot::Item(key(item)),
        Row::Pad(index) => Slot::Pad(*index),
    };
    let built = std::sync::atomic::AtomicBool::new(false);
    move || {
        let first = reserve > 0 && !built.swap(true, std::sync::atomic::Ordering::Relaxed);
        let items: Vec<Row<T>> = each().into_iter().map(Row::Item).collect();
        let pads = if first {
            reserve.saturating_sub(items.len())
        } else {
            0
        };
        if first {
            prepare_queue();
        }
        keyed(
            items.into_iter().chain((0..pads).map(Row::Pad)),
            key.clone(),
            row.clone(),
        )
    }
}
