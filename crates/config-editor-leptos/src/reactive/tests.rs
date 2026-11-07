//! What a short-lived reader leaves behind in long-lived state: a probe
//! subscriber counts the reactive nodes still holding on to it once its
//! owner is gone.

use std::sync::{Arc, Weak};

use leptos::prelude::*;
use leptos::reactive::graph::{AnySource, AnySubscriber, ReactiveNode, Subscriber, WithObserver};

use super::later::testing::{hold, run_held, waiting_timers};
use super::later::{self, Alive};
use super::{Fanout, Relay, detached_owner};

/// Stands in for a view's effect: it subscribes to whatever it reads.
struct Probe;

impl ReactiveNode for Probe {
    fn mark_dirty(&self) {}
    fn mark_check(&self) {}
    fn mark_subscribers_check(&self) {}
    fn update_if_necessary(&self) -> bool {
        false
    }
}

impl Subscriber for Probe {
    fn add_source(&self, _source: AnySource) {}
    fn clear_sources(&self, _subscriber: &AnySubscriber) {}
}

/// Runs `read` as a new probe; the probe's weak count is the number of
/// subscriber lists it is in.
fn probe(read: impl FnOnce()) -> Arc<Probe> {
    let probe = Arc::new(Probe);
    let weak: Weak<dyn Subscriber + Send + Sync> = Arc::downgrade(&probe) as _;
    AnySubscriber(Arc::as_ptr(&probe) as usize, weak).with_observer(read);
    probe
}

/// Reads `read` from a short-lived owner, as a row or popup would, and
/// returns how many subscriber lists still hold the reader once that owner
/// is gone.
fn left_behind(read: impl FnOnce()) -> usize {
    let owner = Owner::new();
    let reader = owner.with(|| probe(read));
    owner.cleanup();
    drop(owner);
    Arc::weak_count(&reader)
}

#[test]
fn a_view_reading_a_long_lived_signal_stays_in_its_subscribers() {
    // The leak this module exists for (Leptos 0.8): if it ever fails,
    // Leptos prunes disposed subscribers and the relays can go.
    let root = Owner::new();
    root.set();
    let long_lived = RwSignal::new(1);
    assert_eq!(left_behind(|| _ = long_lived.get()), 1);
}

#[test]
fn a_view_reading_a_relay_leaves_nothing_behind() {
    let root = Owner::new();
    root.set();
    let long_lived = RwSignal::new(1);
    let relay = Relay::new(move || long_lived.get());
    for _ in 0..50 {
        assert_eq!(left_behind(|| _ = relay.read().get()), 0);
    }
    // Disposed readers are dropped from the relay too.
    let reader = relay.read();
    assert_eq!(relay.reader_count(), 1);
    long_lived.set(2);
    assert_eq!(reader.get_untracked(), 2);
}

#[test]
fn relay_readers_follow_the_source() {
    let root = Owner::new();
    root.set();
    let source = RwSignal::new("a".to_string());
    let relay = Relay::new(move || source.get());
    let first = relay.read();
    source.set("b".to_string());
    let second = relay.read();
    assert_eq!(first.get_untracked(), "b");
    assert_eq!(second.get_untracked(), "b");
    source.set("c".to_string());
    assert_eq!(
        (first.get_untracked(), second.get_untracked()),
        ("c".into(), "c".into())
    );
}

#[test]
fn a_view_reading_a_fanout_leaves_nothing_behind() {
    let root = Owner::new();
    root.set();
    let fanout = StoredValue::new(Fanout::new(0));
    for round in 0..50 {
        assert_eq!(
            left_behind(|| _ = fanout.try_update_value(Fanout::reader).unwrap().get()),
            0
        );
        let readers = fanout
            .try_update_value(|fanout| fanout.set(round + 1))
            .unwrap();
        assert!(readers.is_empty(), "disposed readers are not written");
    }
    let reader = fanout.try_update_value(Fanout::reader).unwrap();
    let readers = fanout.try_update_value(|fanout| fanout.set(100)).unwrap();
    super::notify(readers, &100);
    assert_eq!(reader.get_untracked(), 100);
    assert_eq!(fanout.with_value(Fanout::reader_count), 1);
}

#[test]
fn a_detached_owner_is_cleaned_up_when_dropped_not_with_its_creator() {
    let parent = Owner::new();
    let cleaned = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let owner = parent.with(detached_owner);
    assert!(owner.parent().is_none());
    // The creator is still the current owner afterwards.
    parent.with(|| {
        let _ = detached_owner();
        assert_eq!(Owner::current(), Some(parent.clone()));
    });
    let count = cleaned.clone();
    owner.with(|| on_cleanup(move || _ = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    parent.cleanup();
    assert_eq!(cleaned.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(owner);
    assert_eq!(cleaned.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Deferred work runs only while its target lives: a timer, a frame and a
/// microtask scheduled for a signal whose owner is gone are skipped.
#[test]
fn deferred_work_skips_a_disposed_target_and_runs_a_live_one() {
    hold(|| {
        let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let live_owner = Owner::new();
        let live = live_owner.with(|| RwSignal::new(0));
        let gone_owner = Owner::new();
        let gone = gone_owner.with(|| RwSignal::new(0));
        for (name, target) in [("live", live), ("gone", gone)] {
            let log = ran.clone();
            later::after(target, 10, move |_| {
                log.borrow_mut().push(format!("{name} timer"))
            });
            let log = ran.clone();
            later::next_frame(target, move |_| {
                log.borrow_mut().push(format!("{name} frame"))
            });
            let log = ran.clone();
            later::soon(target, move |_| {
                log.borrow_mut().push(format!("{name} soon"))
            });
        }
        assert!(ran.borrow().is_empty(), "held until run");
        gone_owner.cleanup();
        assert!(live.is_alive());
        assert!(!gone.is_alive());
        run_held();
        assert_eq!(*ran.borrow(), ["live timer", "live frame", "live soon"]);
        drop(live_owner);
    });
}

/// A cancelled timer's work is dropped then, not when it would have fired.
#[test]
fn a_cancelled_timer_drops_its_work() {
    hold(|| {
        let owner = Owner::new();
        let target = owner.with(|| RwSignal::new(0));
        let timer = later::after(target, 10, move |target| target.set(1));
        assert_eq!(waiting_timers(), 1);
        timer.cancel();
        assert_eq!(waiting_timers(), 0);
        run_held();
        assert_eq!(target.get_untracked(), 0);
    });
}

/// Without a clock (native builds), deferred work runs at once.
#[test]
fn natively_deferred_work_runs_at_once() {
    let owner = Owner::new();
    let target = owner.with(|| RwSignal::new(0));
    later::after(target, 10, |target| target.update(|value| *value += 1));
    later::next_frame(target, |target| target.update(|value| *value += 1));
    later::soon(target, |target| target.update(|value| *value += 1));
    assert_eq!(target.get_untracked(), 3);
}
