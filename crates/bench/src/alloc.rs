//! A global allocator that can count allocations, for measuring memory traffic.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};

/// Forwards to the system allocator, counting while [`measure`] runs.
pub(crate) struct CountingAllocator;

thread_local! {
    /// Only the thread inside [`measure`] counts.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
/// Bytes allocated and not yet freed since counting started. Can dip below zero when memory
/// from before is freed.
static LIVE: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);

fn counting() -> bool {
    // During thread teardown the flag may already be gone; don't count then.
    COUNTING.try_with(Cell::get).unwrap_or(false)
}

fn record_allocation(size: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    BYTES.fetch_add(size as u64, Relaxed);
    record_resize(signed(size));
}

fn signed(bytes: usize) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
}

fn record_resize(delta: i64) {
    let live = LIVE.fetch_add(delta, Relaxed) + delta;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting() {
            record_allocation(layout.size());
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if counting() {
            record_allocation(layout.size());
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if counting() {
            record_resize(-signed(layout.size()));
        }
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if counting() {
            ALLOCATIONS.fetch_add(1, Relaxed);
            BYTES.fetch_add(new_size as u64, Relaxed);
            record_resize(signed(new_size) - signed(layout.size()));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AllocStats {
    /// Calls to allocate or grow memory.
    pub allocations: u64,
    /// Total bytes requested by those calls.
    pub bytes: u64,
    /// Most heap memory held at once, above what was held when counting started.
    pub peak_bytes: u64,
}

/// Runs `f` and counts the allocations it makes on this thread. Not reentrant, and only one
/// thread may measure at a time.
pub(crate) fn measure<T>(f: impl FnOnce() -> T) -> (T, AllocStats) {
    ALLOCATIONS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    COUNTING.set(true);
    let result = f();
    COUNTING.set(false);
    let stats = AllocStats {
        allocations: ALLOCATIONS.load(Relaxed),
        bytes: BYTES.load(Relaxed),
        peak_bytes: u64::try_from(PEAK.load(Relaxed)).unwrap_or(0),
    };
    (result, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_allocations_inside_measure_only() {
        let ((), stats) = measure(|| {
            let a = vec![0u8; 1000];
            let b = vec![0u8; 500];
            drop(a);
            drop(b);
        });
        assert_eq!(stats.allocations, 2);
        assert_eq!(stats.bytes, 1500);
        assert_eq!(stats.peak_bytes, 1500);

        let (_, stats) = measure(|| 1 + 1);
        assert_eq!(stats.allocations, 0);
    }
}
