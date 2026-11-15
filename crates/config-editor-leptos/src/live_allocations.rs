//! The test build's allocator: it counts, for each thread, the allocations
//! that thread made and that are not freed yet, so a test can tell whether
//! repeated work keeps memory (a leak) without a browser.
//!
//! Each allocation is counted against the thread that made it, wherever it
//! is freed: a small header in front of it names its owner. So tests running
//! at the same time on other threads cannot move a thread's count, whichever
//! thread frees what.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

/// Counter slots, one per thread (by the order threads first allocate); a
/// test run starts far fewer threads than this.
const SLOTS: usize = 1 << 16;

/// Live allocations and bytes made by one slot's thread, on a cache line of
/// its own so threads counting at once do not slow each other down.
#[repr(align(64))]
struct Slot {
    count: AtomicIsize,
    bytes: AtomicIsize,
}

static COUNTS: [Slot; SLOTS] = [const {
    Slot {
        count: AtomicIsize::new(0),
        bytes: AtomicIsize::new(0),
    }
}; SLOTS];

/// The next slot to give a thread.
static NEXT: AtomicUsize = AtomicUsize::new(1);

thread_local! {
    /// This thread's slot; 0 until its first allocation.
    static SLOT: Cell<usize> = const { Cell::new(0) };
}

/// This thread's slot, given on first use (slot 0 for a thread being torn
/// down, whose counter nothing reads).
fn slot() -> usize {
    SLOT.try_with(|slot| {
        if slot.get() == 0 {
            slot.set(NEXT.fetch_add(1, Ordering::Relaxed) % SLOTS);
        }
        slot.get()
    })
    .unwrap_or(0)
}

fn count(slot: usize, count: isize, bytes: isize) {
    COUNTS[slot].count.fetch_add(count, Ordering::Relaxed);
    COUNTS[slot].bytes.fetch_add(bytes, Ordering::Relaxed);
}

/// The header in front of each allocation: room for the owner's slot, kept
/// at the allocation's alignment so the data after it stays aligned.
fn header(layout: Layout) -> usize {
    layout.align().max(std::mem::size_of::<usize>())
}

/// The layout asked of `System`: the header, then the caller's data.
fn with_header(layout: Layout) -> Option<Layout> {
    Layout::from_size_align(layout.size().checked_add(header(layout))?, layout.align()).ok()
}

struct Counting;

// SAFETY: each allocation is `System`'s, grown by a header at the caller's
// alignment; the pointer handed out is past the header (still aligned), and
// dealloc steps back over it with the same layout arithmetic. The counters
// are atomics and a thread-local `Cell`, none of which allocates.
#[expect(unsafe_code, reason = "a global allocator is an unsafe trait")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(full) = with_header(layout) else {
            return std::ptr::null_mut();
        };
        let start = unsafe { System.alloc(full) };
        if start.is_null() {
            return start;
        }
        let owner = slot();
        unsafe { start.cast::<usize>().write(owner) };
        count(owner, 1, layout.size() as isize);
        unsafe { start.add(header(layout)) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let start = unsafe { pointer.sub(header(layout)) };
        let owner = unsafe { start.cast::<usize>().read() };
        count(owner, -1, -(layout.size() as isize));
        let full = with_header(layout).expect("the layout this allocation was made with");
        unsafe { System.dealloc(start, full) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The allocations this thread made and has not freed, wherever they are
/// freed: `(count, bytes)`.
#[allow(
    dead_code,
    reason = "the memory tests that use it need languages a build may leave out"
)]
pub(crate) fn live() -> (isize, isize) {
    let slot = slot();
    (
        COUNTS[slot].count.load(Ordering::Relaxed),
        COUNTS[slot].bytes.load(Ordering::Relaxed),
    )
}
