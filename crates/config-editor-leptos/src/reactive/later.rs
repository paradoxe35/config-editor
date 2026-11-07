//! The one door for deferred work: timers, animation frames and microtasks.
//!
//! Work deferred past the current task may run after the editor (or the
//! documents view) that scheduled it has unmounted and its signals are
//! disposed. So every such piece of work goes through [`after`],
//! [`next_frame`] or [`soon`], naming what it needs alive (an [`Alive`]
//! target), and runs only if that target still is. `clippy.toml` forbids
//! the browser's and Leptos's own schedulers everywhere else.
//!
//! No JavaScript closure is made per call: the work waits in a queue here and
//! one dispatcher per kind, made once, runs it. A cancelled timer drops its
//! work at once and work that ran is freed as it returns, so nothing waits
//! for the JavaScript garbage collector (Leptos's helpers leave a small stub
//! per call until it collects, and a cancelled `Closure::once_into_js` is
//! never freed).
//!
//! Native builds (the unit tests) have no clock: the work runs at once,
//! untracked as it would be from a timer, unless a test holds it (see
//! `testing::hold`).

use std::cell::RefCell;
use std::collections::BTreeMap;

use leptos::prelude::{IsDisposed, untrack};

/// What deferred work must find alive when it runs: a signal, a stored
/// value, an editor's state, a documents view.
pub(crate) trait Alive: Copy + 'static {
    /// Whether the reactive owner that made it still lives.
    fn is_alive(self) -> bool;
}

impl<T: IsDisposed + Copy + 'static> Alive for T {
    fn is_alive(self) -> bool {
        !self.is_disposed()
    }
}

type Work = Box<dyn FnOnce()>;

/// The work waiting for its time, per kind.
#[derive(Default)]
struct Queues {
    /// The last timer's id; 0 is no timer.
    last_id: u32,
    timers: BTreeMap<u32, Work>,
    frame: Vec<Work>,
    soon: Vec<Work>,
    /// A native test holds the work instead of running it at once.
    #[cfg(not(target_arch = "wasm32"))]
    held: bool,
}

thread_local! {
    static QUEUES: RefCell<Queues> = RefCell::default();
}

/// A timer [`after`] set, to [`cancel`](Self::cancel) it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Timeout {
    id: u32,
    handle: i32,
}

impl Timeout {
    /// Drops the timer's work if it has not run yet.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the protect tip cancels its timer")
    )]
    pub(crate) fn cancel(self) {
        if self.id == 0 {
            return;
        }
        let work = QUEUES.with(|queues| queues.borrow_mut().timers.remove(&self.id));
        if work.is_some() {
            platform::clear_timeout(self.handle);
        }
        // Dropped outside the queues: the work may own an owner whose
        // cleanup schedules more.
        drop(work);
    }
}

/// `run(target)`, if `target` is still alive.
pub(crate) fn guarded<T: Alive>(target: T, run: impl FnOnce(T) + 'static) -> Work {
    Box::new(move || {
        if target.is_alive() {
            run(target);
        }
    })
}

/// Runs `run(target)` after `ms` milliseconds, if `target` is still alive.
pub(crate) fn after<T: Alive>(target: T, ms: i32, run: impl FnOnce(T) + 'static) -> Timeout {
    let work = guarded(target, run);
    if !defers() {
        untrack(work);
        return Timeout::default();
    }
    let id = QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        queues.last_id = queues.last_id.wrapping_add(1).max(1);
        let id = queues.last_id;
        queues.timers.insert(id, work);
        id
    });
    match platform::set_timeout(id, ms) {
        Some(handle) => Timeout { id, handle },
        None => {
            drop(take_timer(id));
            Timeout::default()
        }
    }
}

/// Runs `run(target)` before the next frame is drawn, if `target` is still
/// alive.
pub(crate) fn next_frame<T: Alive>(target: T, run: impl FnOnce(T) + 'static) {
    let work = guarded(target, run);
    if !defers() {
        untrack(work);
        return;
    }
    let first = QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        queues.frame.push(work);
        queues.frame.len() == 1
    });
    if first {
        platform::request_frame();
    }
}

/// Runs `run(target)` at the end of the current task (a microtask), if
/// `target` is still alive.
pub(crate) fn soon<T: Alive>(target: T, run: impl FnOnce(T) + 'static) {
    let work = guarded(target, run);
    if !defers() {
        untrack(work);
        return;
    }
    let first = QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        queues.soon.push(work);
        queues.soon.len() == 1
    });
    if first {
        platform::queue_soon();
    }
}

/// Whether work waits for its time (in a browser, or held by a test).
pub(crate) fn defers() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        QUEUES.with(|queues| queues.borrow().held)
    }
}

fn take_timer(id: u32) -> Option<Work> {
    QUEUES.with(|queues| queues.borrow_mut().timers.remove(&id))
}

/// Runs the timer `id`'s work (its dispatcher calls this).
#[cfg(any(target_arch = "wasm32", test))]
fn fire_timer(id: u32) {
    if let Some(work) = take_timer(id) {
        work();
    }
}

/// Runs the work waiting for this frame; work it schedules waits for the
/// next one.
#[cfg(any(target_arch = "wasm32", test))]
fn fire_frame() {
    let work = QUEUES.with(|queues| std::mem::take(&mut queues.borrow_mut().frame));
    for work in work {
        work();
    }
}

/// Runs the work waiting for the end of the task.
#[cfg(any(target_arch = "wasm32", test))]
fn fire_soon() {
    let work = QUEUES.with(|queues| std::mem::take(&mut queues.borrow_mut().soon));
    for work in work {
        work();
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = queueMicrotask)]
        fn queue_microtask(callback: &JsValue);
    }

    /// One dispatcher per kind of work, made once and kept for the page's
    /// life.
    struct Dispatchers {
        timer: Closure<dyn Fn(u32)>,
        frame: Closure<dyn Fn()>,
        soon: Closure<dyn Fn()>,
    }

    thread_local! {
        static DISPATCHERS: Dispatchers = Dispatchers {
            timer: Closure::new(super::fire_timer),
            frame: Closure::new(super::fire_frame),
            soon: Closure::new(super::fire_soon),
        };
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the door itself: the timer's work is queued in `later`"
    )]
    pub(super) fn set_timeout(id: u32, ms: i32) -> Option<i32> {
        let window = web_sys::window()?;
        DISPATCHERS.with(|dispatchers| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_1(
                    dispatchers.timer.as_ref().unchecked_ref(),
                    ms,
                    &JsValue::from(id),
                )
                .ok()
        })
    }

    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the protect tip cancels its timer")
    )]
    pub(super) fn clear_timeout(handle: i32) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(handle);
        }
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the door itself: the frame's work is queued in `later`"
    )]
    pub(super) fn request_frame() {
        let requested = web_sys::window().and_then(|window| {
            DISPATCHERS.with(|dispatchers| {
                window
                    .request_animation_frame(dispatchers.frame.as_ref().unchecked_ref())
                    .ok()
            })
        });
        // No frame to wait for: what waited for it goes.
        if requested.is_none() {
            drop(super::QUEUES.with(|queues| std::mem::take(&mut queues.borrow_mut().frame)));
        }
    }

    pub(super) fn queue_soon() {
        DISPATCHERS.with(|dispatchers| queue_microtask(dispatchers.soon.as_ref()));
    }
}

/// No clock natively: held work runs when a test says so.
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    pub(super) fn set_timeout(_id: u32, _ms: i32) -> Option<i32> {
        Some(0)
    }

    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the protect tip cancels its timer")
    )]
    pub(super) fn clear_timeout(_handle: i32) {}

    pub(super) fn request_frame() {}

    pub(super) fn queue_soon() {}
}

/// Holding deferred work in native tests, to see what waits and run it.
#[cfg(test)]
pub(crate) mod testing {
    use super::QUEUES;

    /// Runs `body` with deferred work held instead of run at once; what is
    /// still held at the end is dropped.
    pub(crate) fn hold<R>(body: impl FnOnce() -> R) -> R {
        QUEUES.with(|queues| queues.borrow_mut().held = true);
        let result = body();
        let left = QUEUES.with(|queues| std::mem::take(&mut *queues.borrow_mut()));
        drop(left);
        result
    }

    /// How many timers wait.
    pub(crate) fn waiting_timers() -> usize {
        QUEUES.with(|queues| queues.borrow().timers.len())
    }

    /// Runs every held piece of work: the timers (whatever their delay),
    /// the frame's and the microtasks.
    pub(crate) fn run_held() {
        let ids: Vec<u32> = QUEUES.with(|queues| queues.borrow().timers.keys().copied().collect());
        for id in ids {
            super::fire_timer(id);
        }
        super::fire_frame();
        super::fire_soon();
    }
}
