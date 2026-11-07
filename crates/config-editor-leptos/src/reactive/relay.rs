//! Long-lived values read by short-lived views through signals the views
//! own (see the module docs of `reactive`).

use leptos::prelude::*;
use leptos::reactive::effect::ImmediateEffect;

/// A value and the live signals that copy it, one per short-lived reader.
/// Each reader's signal belongs to the owner that asked for it, so it goes
/// (with every subscription to it) when that owner does.
pub(crate) struct Fanout<T: Send + Sync + 'static> {
    value: T,
    readers: Vec<RwSignal<T>>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Fanout<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value,
            readers: Vec::new(),
        }
    }

    /// The value.
    pub(crate) fn value(&self) -> &T {
        &self.value
    }

    /// A signal holding the value, owned by the current owner and updated
    /// through [`Fanout::set`] until that owner is disposed.
    pub(crate) fn reader(&mut self) -> RwSignal<T> {
        self.readers.retain(|reader| !reader.is_disposed());
        let reader = RwSignal::new(self.value.clone());
        self.readers.push(reader);
        reader
    }

    /// Takes `value`; returns the live readers to write it to (none when it
    /// did not change). The caller writes them with [`notify`], outside any
    /// borrow of the fanout: a reader's subscribers may run at once.
    #[must_use]
    pub(crate) fn set(&mut self, value: T) -> Vec<RwSignal<T>> {
        if self.value == value {
            return Vec::new();
        }
        self.value = value;
        self.readers.retain(|reader| !reader.is_disposed());
        self.readers.clone()
    }

    /// The live readers.
    #[cfg(test)]
    pub(crate) fn reader_count(&self) -> usize {
        self.readers
            .iter()
            .filter(|reader| !reader.is_disposed())
            .count()
    }
}

/// Writes `value` to `readers` (from [`Fanout::set`]).
pub(crate) fn notify<T: Clone + Send + Sync + 'static>(readers: Vec<RwSignal<T>>, value: &T) {
    for reader in readers {
        let _ = reader.try_set(value.clone());
    }
}

/// A long-lived reactive value for short-lived views: `source` is tracked
/// by one effect of the relay's owner, and each view reads its own copy
/// ([`Relay::read`]).
///
/// The effect runs as soon as the source changes, so readers change in the
/// same flush as anything reading the source directly; keep `source` cheap.
pub(crate) struct Relay<T: Send + Sync + 'static> {
    fanout: StoredValue<Fanout<T>>,
}

impl<T: Send + Sync + 'static> Clone for Relay<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Relay<T> {}

impl<T: Clone + Default + PartialEq + Send + Sync + 'static> Relay<T> {
    /// Follows `source` for as long as the current owner lives.
    pub(crate) fn new(source: impl Fn() -> T + Send + Sync + 'static) -> Self {
        let fanout = StoredValue::new(Fanout::new(untrack(&source)));
        ImmediateEffect::new_scoped(move || {
            let value = source();
            if let Some(readers) = fanout.try_update_value(|fanout| fanout.set(value.clone())) {
                notify(readers, &value);
            }
        });
        Self { fanout }
    }

    /// The value as a signal owned by the current owner: a view built under
    /// a short-lived owner reads this instead of the source.
    pub(crate) fn read(&self) -> Signal<T> {
        match self.fanout.try_update_value(Fanout::reader) {
            Some(reader) => reader.read_only().into(),
            // The relay's owner is gone: the view asking is being torn down.
            None => Signal::stored(T::default()),
        }
    }

    /// The live readers.
    #[cfg(test)]
    pub(crate) fn reader_count(&self) -> usize {
        self.fanout.with_value(Fanout::reader_count)
    }
}
