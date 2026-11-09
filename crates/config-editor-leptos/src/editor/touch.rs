//! Whether the screen is a touch screen, whose texts name taps, not keys.

/// Whether the screen has no hover (a phone's, a tablet's).
pub(super) fn touch_screen() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(hover: none)").ok().flatten())
        .is_some_and(|query| query.matches())
}
