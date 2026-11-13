//! Keys pressed while an input method (IME) composes text belong to it:
//! `Enter` commits the candidate, arrows walk the candidates, `Escape`
//! cancels, `Tab` may too. No key handler acts on them.

use web_sys::KeyboardEvent;

/// Whether `event` is a key of an input method's composition: pressed
/// during one (`isComposing`), or the key that starts one (Chrome and
/// Safari report it as `Process`, key code 229).
pub(crate) fn composing(event: &KeyboardEvent) -> bool {
    event.is_composing() || event.key() == "Process" || event.key_code() == 229
}
