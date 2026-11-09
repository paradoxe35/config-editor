//! The check after an echo left the host on an older text (see `echoes`,
//! `Echo::Settle`): the echo of the document the host waits for may have
//! been lost in one of its renders that took a later text too, so once the
//! pause is over the host, still on an older text, hears the document again.

use config_editor_leptos::js_support::{Timer, after};
use leptos::prelude::{GetValue, LocalStorage, SetValue, StoredValue};

use crate::echoes::SETTLE_MS;

/// Where an editor or a documents view keeps its pending check.
pub(crate) type Slot = StoredValue<Timer, LocalStorage>;

/// Runs `check` once the pause is over; arming it again starts the pause
/// again. Nothing runs once `slot` is disposed (the editor unmounted).
pub(crate) fn arm(slot: Slot, check: impl FnOnce() + 'static) {
    cancel(slot);
    let timer = after(slot, SETTLE_MS, move |_| check());
    let _ = slot.try_set_value(timer);
}

/// Drops the pending check, if any.
pub(crate) fn cancel(slot: Slot) {
    if let Some(timer) = slot.try_get_value() {
        timer.cancel();
    }
}
