#![allow(missing_docs, reason = "integration test")]

use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, mpsc::sync_channel},
};

/// A helper macro to store the location of the invocation of this macro in some
/// Arc<Mutex<_>> variable before panicking.
///
/// Relies on the fact that the expansion of the `file!`, `line!`, and `column!`
/// macros will occur after the expansion of this macro.
macro_rules! panic_and_get_location {
    ($location:ident $(, $($arg:tt)*)*$(,)?) => {
        *$location.lock().unwrap() = ::core::option::Option::Some(::chillpill::PanicLocation {
            file: String::from(file!()),
            line: line!(),
            col: column!(),
        });

        panic!($($($arg),*)*)
    };
}

/// This test spins up two new threads, and ensures the following sequence of
/// events occurs in order:
///
/// 1. Thread 1 enters [`chillpill::catch`]
/// 2. Thread 2 enters `chillpill::catch`
/// 3. Thread 1 panics and exits `chillpill::catch`
/// 4. Thread 2 panics and exits `chillpill::catch`
///
/// and that all panic locations are recorded correctly.
#[test]
fn overlapping_threads_handled_correctly() {
    let (tx1a, rx1a) = sync_channel::<()>(0);
    let (tx1b, rx1b) = sync_channel::<()>(0);
    let (tx2a, rx2a) = sync_channel::<()>(0);
    let (tx2b, rx2b) = sync_channel::<()>(0);
    let (tx3, rx3) = sync_channel::<()>(0);
    let (tx4, rx4) = sync_channel::<()>(0);

    let location1 = Arc::new(Mutex::new(None));
    let location1_copy = Arc::clone(&location1);
    let handle1 = std::thread::spawn(move || {
        // Wait for a signal from the main thread to enter `chillpill::catch`
        rx1a.recv().unwrap();

        chillpill::catch(AssertUnwindSafe(|| {
            // Inform the main thread that have entered `chillpill::catch`
            tx1b.send(()).unwrap();

            // Wait for a signal from the main thread to panic
            rx3.recv().unwrap();

            panic_and_get_location!(location1_copy, "Thread 1 panic");
        }))
        .unwrap_err()
    });

    let location2 = Arc::new(Mutex::new(None));
    let location2_copy = Arc::clone(&location2);
    let handle2 = std::thread::spawn(move || {
        // Wait for a signal from the main thread to enter `chillpill::catch`
        rx2a.recv().unwrap();

        chillpill::catch(AssertUnwindSafe(|| {
            // Inform the main thread that have entered `chillpill::catch`
            tx2b.send(()).unwrap();

            // Wait for a signal from the main thread to panic
            rx4.recv().unwrap();

            panic_and_get_location!(location2_copy, "Thread 2 panic");
        }))
        .unwrap_err()
    });

    // Trigger thread 1 to enter `chillpill::catch`, and wait for confirmation
    tx1a.send(()).unwrap();
    rx1b.recv().unwrap();

    // Trigger thread 2 to enter `chillpill::catch`, and wait for confirmation
    tx2a.send(()).unwrap();
    rx2b.recv().unwrap();

    // Trigger thread 1 to panic, and join to wait until it's done
    tx3.send(()).unwrap();
    let result1 = handle1.join().unwrap();

    // Trigger thread 2 to panic, and join to wait until it's done
    tx4.send(()).unwrap();
    let result2 = handle2.join().unwrap();

    // Ensure both threads reported correct panic data
    assert_eq!(result1.payload_as_string().unwrap(), "Thread 1 panic");
    assert_eq!(
        result1.location,
        Arc::into_inner(location1).unwrap().into_inner().unwrap()
    );
    assert_eq!(result2.payload_as_string().unwrap(), "Thread 2 panic");
    assert_eq!(
        result2.location,
        Arc::into_inner(location2).unwrap().into_inner().unwrap()
    );
}
