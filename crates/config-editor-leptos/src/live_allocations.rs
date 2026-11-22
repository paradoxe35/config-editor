//! The test build's allocator: it counts the allocations each thread makes
//! and has not freed, so a test can tell whether repeated work keeps
//! memory (a leak) without a browser. It also finds the copies of a long
//! text a thread holds (see [`copies_of`]).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// How many live allocations of at least the watched size are recorded.
const WATCHED: usize = 256;

thread_local! {
    /// Live allocations and bytes this thread made (freed ones taken off).
    static LIVE: Cell<(isize, isize)> = const { Cell::new((0, 0)) };
    /// The size from which allocations are recorded (0: none), see
    /// [`copies_of`].
    static WATCH_FROM: Cell<usize> = const { Cell::new(0) };
    /// The live recorded allocations: address and size (0: a free slot).
    static BIG: [Cell<(usize, usize)>; WATCHED] = const { [const { Cell::new((0, 0)) }; WATCHED] };
}

fn counted(count: isize, bytes: isize) {
    // A thread being torn down has no counter left; nothing reads it then.
    let _ = LIVE.try_with(|live| {
        let (was_count, was_bytes) = live.get();
        live.set((was_count + count, was_bytes + bytes));
    });
}

/// Records the live allocation at `pointer` of `size` bytes, when watched.
fn watched(pointer: *mut u8, size: usize) {
    let from = WATCH_FROM.try_with(Cell::get).unwrap_or(0);
    if from == 0 || size < from || pointer.is_null() {
        return;
    }
    let _ = BIG.try_with(|big| {
        if let Some(slot) = big.iter().find(|slot| slot.get().0 == 0) {
            slot.set((pointer as usize, size));
        }
    });
}

/// Forgets the allocation at `pointer`, freed.
fn unwatched(pointer: *mut u8) {
    let _ = BIG.try_with(|big| {
        if let Some(slot) = big.iter().find(|slot| slot.get().0 == pointer as usize) {
            slot.set((0, 0));
        }
    });
}

struct Counting;

// SAFETY: every call is passed on to `System` unchanged; the counting only
// reads the layout and touches a thread-local `Cell` that never allocates.
#[expect(unsafe_code, reason = "a global allocator is an unsafe trait")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        counted(1, layout.size() as isize);
        let pointer = unsafe { System.alloc(layout) };
        watched(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        counted(1, layout.size() as isize);
        let pointer = unsafe { System.alloc_zeroed(layout) };
        watched(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        counted(-1, -(layout.size() as isize));
        unwatched(pointer);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        counted(0, new_size as isize - layout.size() as isize);
        unwatched(pointer);
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        watched(moved, new_size);
        moved
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

/// From now on, records this thread's allocations of at least `size`
/// bytes (0: none), for [`copies_of`].
#[allow(
    dead_code,
    reason = "the copy counts that use it need languages a build may leave out"
)]
pub(crate) fn watch_from(size: usize) {
    WATCH_FROM.with(|from| from.set(size));
}

/// How many live allocations recorded since [`watch_from`] hold a text
/// beginning with `head`: at their start (a `String`) or after an `Arc`'s
/// two counts (an `Arc<str>`). The copies of a long text the thread keeps.
#[allow(
    dead_code,
    reason = "the copy counts that use it need languages a build may leave out"
)]
#[expect(unsafe_code, reason = "reads the bytes of the recorded allocations")]
pub(crate) fn copies_of(head: &str) -> usize {
    let head = head.as_bytes();
    let counts = std::mem::size_of::<[usize; 2]>();
    BIG.with(|big| {
        big.iter()
            .map(Cell::get)
            .filter(|(address, size)| {
                if *address == 0 {
                    return false;
                }
                // SAFETY: a live allocation of `size` bytes at `address`
                // (recorded when made, forgotten when freed).
                // Only the first bytes are read: those a text holds from
                // its start (a buffer's spare capacity comes after).
                let read = (*size).min(counts + head.len());
                let bytes = unsafe { std::slice::from_raw_parts(*address as *const u8, read) };
                bytes.starts_with(head)
                    || bytes
                        .get(counts..)
                        .is_some_and(|rest| rest.starts_with(head))
            })
            .count()
    })
}
