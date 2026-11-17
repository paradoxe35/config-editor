//! The test build's allocator: it counts the allocations each thread makes
//! and has not freed, so a test can tell whether repeated work keeps
//! memory (a leak) without a browser.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Live allocations and bytes this thread made (freed ones taken off).
    static LIVE: Cell<(isize, isize)> = const { Cell::new((0, 0)) };
}

fn counted(count: isize, bytes: isize) {
    // A thread being torn down has no counter left; nothing reads it then.
    let _ = LIVE.try_with(|live| {
        let (was_count, was_bytes) = live.get();
        live.set((was_count + count, was_bytes + bytes));
    });
}

struct Counting;

// SAFETY: every call is passed on to `System` unchanged; the counting only
// reads the layout and touches a thread-local `Cell` that never allocates.
#[expect(unsafe_code, reason = "a global allocator is an unsafe trait")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        counted(1, layout.size() as isize);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        counted(1, layout.size() as isize);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        counted(-1, -(layout.size() as isize));
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        counted(0, new_size as isize - layout.size() as isize);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The allocations this thread made and has not freed: `(count, bytes)`.
#[allow(
    dead_code,
    reason = "the memory tests that use it need languages a build may leave out"
)]
pub(crate) fn live() -> (isize, isize) {
    LIVE.with(Cell::get)
}
