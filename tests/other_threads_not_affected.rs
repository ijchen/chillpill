#![allow(missing_docs, reason = "integration test")]

use std::{
    panic::AssertUnwindSafe,
    sync::atomic::{AtomicU8, Ordering},
};

static COUNTER: AtomicU8 = AtomicU8::new(0);

fn increment() {
    COUNTER.fetch_add(1, Ordering::SeqCst);
}

/// This test replaces the global panic hook with one that has a detectable side
/// effect, then ensures that a panic on a separate thread that happens to occur
/// while the main thread is within a [`chillpill::catch`] closure still uses
/// the old global panic hook.
#[test]
fn other_threads_not_affected() {
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

    // This panic should not increment the counter
    std::panic::catch_unwind(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

    // Replace the global panic hook with one that just increments our counter
    std::panic::set_hook(Box::new(|_| increment()));

    // Spawn a thread, ready to panic on our signal
    let (panic_tx, panic_rx) = std::sync::mpsc::channel::<()>();
    let handle = std::thread::spawn(move || {
        // Wait until the main thread is ready for us to panic
        panic_rx.recv().unwrap();

        // Panic
        panic!()
    });

    // This panic should increment the counter
    std::panic::catch_unwind(|| panic!()).unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

    chillpill::catch(AssertUnwindSafe(|| {
        // Now that we're within `catch`, trigger the other thread's panic
        // This panic should increment the counter
        panic_tx.send(()).unwrap();
        handle.join().unwrap_err();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);

        // This panic should *not* increment the counter
        panic!()
    }))
    .unwrap_err();
    assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
}
