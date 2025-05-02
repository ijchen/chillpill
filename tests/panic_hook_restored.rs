#![allow(missing_docs, reason = "integration test")]

use std::sync::atomic::{AtomicU8, Ordering};

static COUNTER: AtomicU8 = AtomicU8::new(0);

fn increment() {
    COUNTER.fetch_add(1, Ordering::SeqCst);
}

/// This test replaces the global panic hook with one that has a detectable side
/// effect, then ensures that that new hook is not triggered by a panic within a
/// [`chillpill::catch`] call, but *is* restored correctly and triggered in a
/// panic triggered after the `catch` ends.
#[test]
fn panic_hook_restored() {
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

    // This panic should not increment the counter
    std::panic::catch_unwind(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

    // Replace the global panic hook with one that just increments our counter
    std::panic::set_hook(Box::new(|_| increment()));

    // This panic should increment the counter
    std::panic::catch_unwind(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

    // This panic should *not* increment the counter
    chillpill::catch(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

    // This panic should increment the counter
    std::panic::catch_unwind(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
}
