//! Allocation on the dispatch path, measured.
//!
//! This test binary installs a global allocator that counts the
//! allocations of each thread. Everything is built and warmed before the
//! measured call; the call itself is the whole dispatch of a public bundle
//! through two builders, from the pre-checks through both deadline checks
//! per adapter to the last write and the end-of-fan-out bookkeeping.
//!
//! It measures the runner and the core functions the dispatch calls, over
//! fake transports. It says nothing about allocations inside a real TLS
//! stream or the kernel.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::*;
use darkbot_bsc_execution::submission::send::{Armed, SendOutcome, dispatch_diagnostic};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    // A thread that is being torn down has no counter; nothing to count.
    let _ = ALLOCATIONS.try_with(|allocations| allocations.set(allocations.get() + 1));
}

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

// SAFETY: every call is forwarded unchanged to the system allocator; the
// counter is a const-initialised thread local without a destructor, so
// counting neither allocates nor re-enters the allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn the_dispatch_of_a_two_builder_bundle_allocates_nothing() {
    let mut world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let mut club48 = club48_frame(&world, &plan, &table);
    let mut blockrazor = blockrazor_frame(&world, &plan, &table);
    // The fakes record into storage reserved up front.
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    let clock = quick_clock();
    let deadline = deadline();
    let current = world.snapshot;
    let context = context(&table, &current);
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut club48,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: &mut b,
            frame: &mut blockrazor,
        },
    ];

    let before = allocations();
    let report = dispatch_diagnostic(
        &mut world.lane,
        &plan,
        &context,
        &clock,
        &deadline,
        &mut armed,
    );
    let during = allocations() - before;

    let report = report.unwrap();
    assert_eq!(
        report.adapter(adapter(CLUB48)).unwrap().outcome,
        SendOutcome::HandedOff
    );
    assert_eq!(
        report.adapter(adapter(BLOCKRAZOR)).unwrap().outcome,
        SendOutcome::HandedOff
    );
    assert_eq!(during, 0, "allocations during the dispatch");
}
