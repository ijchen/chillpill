// These markdown links are used to override the docs.rs web links present at
// the bottom of README.md. These two lists must be kept in sync, or the links
// included here will be hard-coded links to docs.rs, not relative doc links.
// ON_RELEASE: the below link(s) should be verified to match the readme, and
// this "on release" comment removed (the above one should stay).
//! [`std::panic::catch_unwind`]: std::panic::catch_unwind
//! [`chillpill::catch`]: crate::catch
#![cfg_attr(any(doc, test), doc = include_str!("../README.md"))]

mod panic_data;
mod panic_hook;

use std::{cell::RefCell, panic::UnwindSafe};

pub use panic_data::{PanicData, PanicLocation};
use panic_hook::{add_chillpill_thread, remove_chillpill_thread};

/// A specialized [`Result`] type for chillpill.
pub type Result<T> = std::result::Result<T, PanicData>;

thread_local! {
    /// A thread-local stack of [`Option<PanicLocation>`]s, used to smuggle out
    /// the location of a panic from the panic hook. Each frame on this stack
    /// corresponds to one active call to [`chillpill::catch`] somewhere in the
    /// current thread's stack frame.
    ///
    /// On any given thread, this stack starts out initially empty. Every time
    /// `catch` is called, another frame is pushed to the top of the stack,
    /// where it will live until the end of that `catch` call. Nested calls to
    /// `catch` will grow the stack.
    ///
    /// Each frame starts as [`None`]. Anytime a panic occurs on a thread, the
    /// chillpill custom panic hook will replace the value at the top of this
    /// stack with the actual location of the panic. If some panics within a
    /// `catch` call are caught before unwinding all the way back to `catch`
    /// (ex., via [`std::panic::catch_unwind`]), the panic location that was
    /// recorded for them at the top of the stack is not relevant (and would be
    /// incorrect if `catch` were to use it). This is not a problem, because
    /// `catch` will only extract and use the panic location if it actually
    /// catches an unwinding panic - and if it does, it is guaranteed that that
    /// panic was the last one to record its location at the top of the panic
    /// location stack.
    ///
    /// Here is a basic summary of how this stack is used by both `catch` and
    /// our custom panic hook:
    ///
    /// Each call to `catch` does the following:
    /// 1. Push a `None` frame on entry
    /// 2. Run the user-provided closure
    /// 3. Pop that frame on exit (and, if the closure unwound, extract its
    ///    `Some(PanicLocation)`).
    ///
    /// Meanwhile, our custom global panic hook:
    /// - Delegates to the real hook if the thread local stack is empty
    /// - Otherwise, replaces the top `None` with `Some(PanicLocation)` when any
    ///   panic begins unwinding on this thread.
    ///
    /// This ensures all `catch` calls are able to reliably identify exactly
    /// which (if any) panic location corresponds to the unwinding panic it
    /// caught.
    ///
    /// [`chillpill::catch`]: crate::catch
    static PANIC_LOCATION_STACK: RefCell<Vec<Option<PanicLocation>>> = const { RefCell::new(Vec::new()) };
}

/// Invokes a closure, capturing the cause and location of an unwinding panic if
/// one occurs, and suppressing the default panic output on `stderr`.
///
/// This function is very similar to [`std::panic::catch_unwind`], with four
/// key differences:
///
/// 1. If the closure panics, this function reports the panic location (file,
///    line, and column) in addition to the payload
/// 2. This function suppresses the panic message that would normally be printed
///    to `stderr` for all panics originating in the provided closure (and
///    will prevent any other custom panic hook logic from running)
/// 3. If the current thread is already unwinding from a panic when this
///    function is called, a double-panic will occur immediately instead of only
///    if/when the provided closure itself panics.
/// 4. This function requires that no other code (on any thread) replaces the
///    global panic hook while this function is executing (ex. through
///    [`std::panic::set_hook`] or [`std::panic::take_hook`])
///
/// # Panic Hook Warning
///
/// That fourth point above is important - in order to access panic location
/// information and suppress the default error message, `chillpill` needs to
/// temporarily swap out the global panic hook with its own custom one. Since
/// the panic hook is a global resource, `chillpill` is not able to detect or
/// prevent a situation where other code replaces our panic hook during the
/// execution of this function.
///
/// **If any other code on any thread replaces the panic hook at any point
/// during the execution of this function, arbitrarily weird things may happen
/// with the panic hook.**
///
/// This caveat does not apply to multiple concurrent calls to this function -
/// [`chillpill::catch`] coordinates internally so that multiple calls will not
/// fight over the panic hook.
///
/// # Errors
///
/// Returns an error with panic data if the provided closure panics.
///
/// # Panics
///
/// If called from a panicking thread.
///
/// [`chillpill::catch`]: crate::catch
pub fn catch<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> Result<R> {
    assert!(
        !std::thread::panicking(),
        "cannot call `chillpill::catch` from a panicking thread"
    );

    // If this thread was previously not in a `catch` call, register another
    // thread as reliant on the chillpill panic hook.
    if PANIC_LOCATION_STACK.with_borrow(Vec::is_empty) {
        add_chillpill_thread();
    }

    // Push a new frame onto our panic location stack. See the documentation on
    // `PANIC_LOCATION_STACK` for details.
    PANIC_LOCATION_STACK.with_borrow_mut(|stack| stack.push(None));

    // Call the provided closure, using `std::panic::catch_unwind` to catch the
    // panic payload and prevent further unwinding
    let catch_unwind_result = std::panic::catch_unwind(f);

    // Pop the panic location from our panic location stack. See the
    // documentation on `PANIC_LOCATION_STACK` for details.
    let location = PANIC_LOCATION_STACK.with_borrow_mut(|stack| stack.pop().unwrap());

    // If this was the last `catch` call on this thread, unmark this thread as
    // relying on the chillpill panic hook.
    if PANIC_LOCATION_STACK.with_borrow(Vec::is_empty) {
        remove_chillpill_thread();
    }

    catch_unwind_result.map_err(|payload| PanicData { payload, location })
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use super::*;

    /// This test ensures basic closures that don't panic are run correctly by
    /// [`chillpill::catch`](crate::catch).
    #[test]
    fn basic_code_no_panic() {
        assert_eq!(catch(|| 2 + 2).unwrap(), 4);

        assert_eq!(catch(|| String::from("it works!")).unwrap(), "it works!");
    }

    /// This test ensures that [`chillpill::catch`] catches a panic (and its
    /// payload) that triggered within the closure it was given.
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn catches_basic_panic() {
        let result = catch(|| {
            panic!("uh oh spaghettio");
        })
        .unwrap_err();

        assert_eq!(result.payload_as_string().unwrap(), "uh oh spaghettio");
    }

    /// A helper macro to store the location of the invocation of this macro in
    /// some variable before panicking.
    ///
    /// Relies on the fact that the expansion of the `file!`, `line!`, and
    /// `column!` macros will occur after the expansion of this macro.
    macro_rules! panic_and_get_location {
        ($location:ident $(, $($arg:tt)*)*$(,)?) => {
            $location = ::core::option::Option::Some($crate::PanicLocation {
                file: String::from(file!()),
                line: line!(),
                col: column!(),
            });

            panic!($($($arg),*)*)
        };
    }

    /// This test ensures that [`chillpill::catch`] records the correct panic
    /// location for a panic that triggered within the closure it was given.
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn captures_location() {
        let mut location = None;
        let result = catch(AssertUnwindSafe(|| {
            panic_and_get_location!(location, "I'm freakin' out!!!");
        }))
        .unwrap_err();

        assert_eq!(result.payload_as_string().unwrap(), "I'm freakin' out!!!");
        assert_eq!(result.location, location);
    }

    /// This test ensures that [`chillpill::catch`] captures the panic payload
    /// correctly when it isn't a [`String`] or [`&str`](str).
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn non_string_payload() {
        let result = catch(|| {
            let payload: Vec<i32> = vec![1, 2, 3];
            std::panic::resume_unwind(Box::new(payload));
        })
        .unwrap_err();

        assert_eq!(*result.payload.downcast::<Vec<i32>>().unwrap(), &[1, 2, 3]);
    }

    /// This test ensures that multiple nested calls to [`chillpill::catch`]
    /// correctly catch the location and payloads of every panic.
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn nested_catches() {
        let mut location1 = None;
        let result1 = catch(AssertUnwindSafe(|| {
            let mut location2 = None;
            let result2 = catch(AssertUnwindSafe(|| {
                let mut location3 = None;
                let result3 = catch(AssertUnwindSafe(|| {
                    panic_and_get_location!(location3, "panic depth 3");
                }))
                .unwrap_err();

                assert_eq!(result3.payload_as_string().unwrap(), "panic depth 3");
                assert_eq!(result3.location, location3);

                panic_and_get_location!(location2, "panic depth 2");
            }))
            .unwrap_err();

            assert_eq!(result2.payload_as_string().unwrap(), "panic depth 2");
            assert_eq!(result2.location, location2);

            panic_and_get_location!(location1, "panic depth 1");
        }))
        .unwrap_err();

        assert_eq!(result1.payload_as_string().unwrap(), "panic depth 1");
        assert_eq!(result1.location, location1);
    }

    /// This test ensures that a [`std::panic::catch_unwind`] catching a panic
    /// within a [`chillpill::catch`] that does not panic behaves as expected.
    ///
    /// The expected behavior here is that the first `catch` call reports that
    /// the closure did not panic, and subsequent calls to `catch` are not
    /// affected.
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn catch_unwind_catches_only_panic() {
        catch(AssertUnwindSafe(|| {
            let _ = std::panic::catch_unwind(|| {
                panic!("this panic should not make it to the outer catch");
            });
        }))
        .unwrap();

        // Ensure the above won't cause future catches to do weird things

        let mut location = None;
        let result = catch(AssertUnwindSafe(|| {
            panic_and_get_location!(location, "unrelated later panic");
        }))
        .unwrap_err();

        assert_eq!(result.payload_as_string().unwrap(), "unrelated later panic");
        assert_eq!(result.location, location);
    }

    /// This test ensures that a [`std::panic::catch_unwind`] catching a panic
    /// within a [`chillpill::catch`] that does panic behaves as expected.
    ///
    /// The expected behavior here is that the first `catch` call reports that
    /// the closure panicked with the second panic's payload and location, and
    /// subsequent calls to `catch` are not affected.
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn catch_unwind_and_uncaught_panic() {
        let mut location = None;
        let result = catch(AssertUnwindSafe(|| {
            let _ = std::panic::catch_unwind(|| {
                panic!("this panic is irrelevant");
            });

            panic_and_get_location!(location, "actual panic");
        }))
        .unwrap_err();

        assert_eq!(result.payload_as_string().unwrap(), "actual panic");
        assert_eq!(result.location, location);

        // Ensure the above won't cause future catches to do weird things

        let mut location = None;
        let result = catch(AssertUnwindSafe(|| {
            panic_and_get_location!(location, "unrelated later panic");
        }))
        .unwrap_err();

        assert_eq!(result.payload_as_string().unwrap(), "unrelated later panic");
        assert_eq!(result.location, location);
    }
}
