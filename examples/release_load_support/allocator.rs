//! Requested Rust allocation accounting for this fixture process, excluding C/OS allocations.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
pub struct Tracking;
fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
    PEAK.fetch_max(live, Ordering::Relaxed);
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}
// SAFETY: All storage operations delegate unchanged layouts/pointers to System;
// the added bookkeeping uses only static atomics and cannot allocate or recurse.
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc caller supplied a valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Same unchanged valid layout and System's zeroing guarantee.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: Caller transfers the exact live System allocation and layout.
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Caller provides a live allocation and valid replacement size.
        let result = unsafe { System.realloc(pointer, layout, new_size) };
        if !result.is_null() {
            LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
            added(new_size);
        }
        result
    }
}
pub fn snapshot() -> (u64, u64, u64) {
    (
        LIVE.load(Ordering::Relaxed),
        PEAK.load(Ordering::Relaxed),
        ALLOCATIONS.load(Ordering::Relaxed),
    )
}
pub fn reset_peak_quiescent() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
