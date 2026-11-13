//! Reactive building blocks that keep short-lived views from leaking into
//! long-lived state.
//!
//! In Leptos 0.8 a disposed effect or memo stays in the subscriber list of
//! every signal it read (as a weak pointer, which keeps its allocation), and
//! a disposed owner stays in its parent's list of children. Nothing prunes
//! either list while the signal or the parent lives. A view rebuilt again
//! and again under long-lived state (a row scrolled in and out, the
//! underlines redrawn after each edit, a document's tab opened and closed)
//! therefore grows memory a little each time, without bound.
//!
//! The rule that follows: a view that comes and goes reads only state that
//! comes and goes with it.
//!
//! - [`Relay`]: a long-lived value handed to short-lived views as signals
//!   they own, so they never subscribe to the long-lived source.
//! - [`Fanout`]: the same for a value the owner writes itself (a row's
//!   state), without an effect.
//! - [`KeyedList`]: `For` whose rows own themselves instead of registering
//!   with the list's owner.
//! - [`detached_owner`]: an owner no parent keeps a record of.
//! - [`mount_detached`]: a view mounted under such an owner.
//! - [`Kept`]: a part a setting switches on and off, built once and then
//!   taken off the page and put back.
//! - [`later`]: the one door for deferred work (timers, frames,
//!   microtasks), which skips work whose editor has gone.
//! - [`window_listener`]: a window listener freed with its owner.

mod kept;
mod keyed;
pub(crate) mod later;
mod relay;
mod size_observer;
#[cfg(test)]
mod tests;
mod window_listener;

pub(crate) use kept::Kept;
pub(crate) use keyed::{KeyedList, MANY_ROWS, detached_owner, mount_detached};
pub(crate) use relay::{Fanout, Relay, notify};
pub(crate) use size_observer::size_observer;
pub(crate) use window_listener::window_listener;
