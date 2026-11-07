//! Heap statistics for hunting leaks (cargo feature `alloc-stats`, never in
//! the published package): a counting allocator, and functions the page can
//! call to read it.
//!
//! - `__allocStats()`: `[live bytes, live allocations]`.
//! - `__allocSizes()`: live allocations per size in bytes (index 4096 holds
//!   every larger one). Two snapshots around repeated work show which sizes
//!   it leaves behind.
//! - `__allocTrack([sizes])` (up to eight), then `__allocSites()`: the
//!   JavaScript stack (wasm function names included in a build without
//!   `strip`) of each live allocation of those sizes, grouped and counted.
//!
//! Build it: `cargo build --release --target wasm32-unknown-unknown -p
//! config-editor-js --features alloc-stats` (with
//! `CARGO_PROFILE_RELEASE_STRIP=false` for function names), then
//! `wasm-bindgen --keep-debug --target web` as `build.mjs` does.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use wasm_bindgen::prelude::*;

/// Sizes counted one by one; the last bucket holds every larger one.
const BUCKETS: usize = 4097;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static SIZES: [AtomicUsize; BUCKETS] = [const { AtomicUsize::new(0) }; BUCKETS];
/// The sizes whose allocation sites are recorded (0: none).
static TRACKED: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];

thread_local! {
    /// Set while recording a site: recording allocates too.
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    /// Live tracked allocations: address → (size, stack).
    static SITES: RefCell<Option<HashMap<usize, (usize, String)>>> = const { RefCell::new(None) };
}

fn bucket(size: usize) -> &'static AtomicUsize {
    &SIZES[size.min(BUCKETS - 1)]
}

fn tracked(size: usize) -> bool {
    size != 0 && TRACKED.iter().any(|tracked| tracked.load(Relaxed) == size)
}

/// Runs `record` unless a recording is already under way.
fn recording(record: impl FnOnce()) {
    if RECORDING.with(|busy| busy.replace(true)) {
        return;
    }
    record();
    RECORDING.with(|busy| busy.set(false));
}

fn allocated(pointer: *mut u8, size: usize) {
    LIVE_BYTES.fetch_add(size, Relaxed);
    LIVE_COUNT.fetch_add(1, Relaxed);
    bucket(size).fetch_add(1, Relaxed);
    if tracked(size) {
        recording(|| {
            let stack = js_sys::Reflect::get(&js_sys::Error::new(""), &"stack".into())
                .ok()
                .and_then(|stack| stack.as_string())
                .unwrap_or_default();
            SITES.with(|sites| {
                sites
                    .borrow_mut()
                    .get_or_insert_with(HashMap::new)
                    .insert(pointer as usize, (size, stack));
            });
        });
    }
}

fn freed(pointer: *mut u8, size: usize) {
    LIVE_BYTES.fetch_sub(size, Relaxed);
    LIVE_COUNT.fetch_sub(1, Relaxed);
    bucket(size).fetch_sub(1, Relaxed);
    if tracked(size) {
        recording(|| {
            SITES.with(|sites| {
                if let Some(sites) = sites.borrow_mut().as_mut() {
                    sites.remove(&(pointer as usize));
                }
            });
        });
    }
}

struct Counting;

// SAFETY: every call is passed on to `System` unchanged; the counting only
// reads the layout and the returned pointer.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        freed(pointer, layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        freed(pointer, layout.size());
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        allocated(moved, new_size);
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// `[live bytes, live allocations]`.
#[wasm_bindgen(js_name = __allocStats)]
pub fn alloc_stats() -> Vec<f64> {
    vec![
        LIVE_BYTES.load(Relaxed) as f64,
        LIVE_COUNT.load(Relaxed) as f64,
    ]
}

/// Live allocations per size.
#[wasm_bindgen(js_name = __allocSizes)]
pub fn alloc_sizes() -> Vec<f64> {
    SIZES
        .iter()
        .map(|count| count.load(Relaxed) as f64)
        .collect()
}

/// Records the sites of allocations of `sizes` from now on.
#[wasm_bindgen(js_name = __allocTrack)]
pub fn alloc_track(sizes: Vec<u32>) {
    let mut sizes = sizes.into_iter();
    for slot in &TRACKED {
        slot.store(sizes.next().unwrap_or(0) as usize, Relaxed);
    }
}

/// The live tracked allocations, grouped by size and stack, most first:
/// `x<count> <size> <stack>`, groups separated by `----`.
#[wasm_bindgen(js_name = __allocSites)]
pub fn alloc_sites() -> String {
    let mut report = String::new();
    recording(|| {
        let mut groups: HashMap<String, usize> = HashMap::new();
        SITES.with(|sites| {
            for (size, stack) in sites.borrow().iter().flat_map(HashMap::values) {
                *groups.entry(format!("{size} {stack}")).or_default() += 1;
            }
        });
        let mut groups: Vec<(String, usize)> = groups.into_iter().collect();
        groups.sort_by_key(|group| std::cmp::Reverse(group.1));
        report = groups
            .into_iter()
            .map(|(site, count)| format!("x{count} {site}\n----"))
            .collect::<Vec<_>>()
            .join("\n");
    });
    report
}
