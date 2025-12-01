// These markdown links are used to override the docs.rs web links present at
// the bottom of README.md. These two lists must be kept in sync, or the links
// included here will be hard-coded links to docs.rs, not relative doc links.
//! [`std::panic::catch_unwind`]: std::panic::catch_unwind
//! [`chillpill::catch`]: crate::catch
#![doc = include_str!("../README.md")]
#![expect(
    clippy::needless_doctest_main,
    reason = "README.md contains example usage with a `fn main()` that also runs as a doctest"
)]

mod panic_data;
mod panic_hook;
mod thread_local_catch_stack;

use std::panic::UnwindSafe;

pub use panic_data::{PanicData, PanicLocation};

use crate::thread_local_catch_stack::{
    CaptureBacktrace, CatchStackFrame, THREAD_LOCAL_CATCH_STACK,
};

/// A specialized [`Result`] type for chillpill.
pub type Result<T> = std::result::Result<T, PanicData>;

/// Invokes a closure, capturing the cause, location, and backtrace of an unwinding panic if one
/// occurs, and suppressing the default panic output on `stderr`.
///
/// This function is very similar to [`std::panic::catch_unwind`], with four key differences:
///
/// 1. If the closure panics, this function reports the panic location (file, line, and column) and
///    backtrace (see below) in addition to the payload
/// 2. This function suppresses the panic message that would normally be printed to `stderr` for all
///    panics originating in the provided closure (and will prevent any other custom panic hook
///    logic from running)
/// 3. The globally first call to this function must not be made from an unwinding thread. If you
///    might call this function while unwinding (in a [`Drop`] impl, for example), an easy way to
///    guarantee this condition is met is to call this function with an empty closure from the main
///    thread at the start of the program.
/// 4. The first time this function is called, it replaces the global panic hook with a chillpill
///    custom one. See below for more information on this.
///
/// # Backtrace Capture
///
/// This function determines whether or not to capture a backtrace based on environment variable
/// configuration, like [`std::backtrace::Backtrace::capture`]. To forcibly capture a backtrace
/// regardless of environment variables, use the [`catch_force_backtrace`] function. Similarly, to
/// unconditionally disable backtrace capture, use the [`catch_never_backtrace`] function.
///
/// # Panic Hook Replacement
///
/// In order to access additional panic information and suppress the default error message,
/// `chillpill` needs to replace the global panic hook with its own custom one. Since the panic hook
/// is a global resource, `chillpill` is not able to detect or prevent a situation where other code
/// replaces our panic hook with another one, which can cause unexpected behavior.
///
/// An easy way to prevent this is to ensure no other code replaces the panic hook at any point
/// after the first call to this function. Replacing the panic hook is uncommon, so usually this is
/// the easy (and correct) solution.
///
/// If other code does need to replace the panic hook, it must ensure that their panic hook invokes
/// ours at some point during its execution for unwinding panics. That is sufficient to ensure
/// chillpill can still capture panic information, although chillpill cannot prevent the new "outer"
/// panic hook from printing to stderr if it attempts to.
///
/// # No Hook Panics
///
/// It is uncommon but possible for code to panic without invoking the panic hook (e.g., via
/// [`std::panic::resume_unwind`]). These panics will still be captured by chillpill, but their
/// panic location and backtrace will be incorrect. Typically, "incorrect" means they will be
/// [`None`] and [`Backtrace::disabled()`] respectively.
///
/// The only exception is if:
/// 1. An unwinding, hook-invoking panic occurs within the closure;
/// 2. That panic is caught within the closure, *without* chillpill (e.g., via
///    `std::panic::catch_unwind`); and
/// 3. A second unwinding, *non-hook-invoking* panic occurs within the closure and escapes to be
///    caught by this catch call.
///
/// In this case, the reported panic location and backtrace will correspond to the first (caught,
/// hook-invoking) panic, but the payload will be from the second (uncaught, non-hook-invoking)
/// panic.
///
/// # Errors
///
/// Returns an error with panic data if the provided closure panics.
///
/// # Panics
///
/// If this is the globally first call to `chillpill::catch` (or the alternate backtrace variants),
/// ***and*** this thread is currently unwinding from a panic.
///
/// [`catch_force_backtrace`]: catch_force_backtrace
/// [`catch_never_backtrace`]: catch_never_backtrace
/// [`Backtrace::disabled()`]: std::backtrace::Backtrace::disabled
pub fn catch<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> Result<R> {
    catch_inner(f, CaptureBacktrace::Default)
}

/// Like [`chillpill::catch`], but always captures a backtrace. See its documentation for details.
///
/// # Errors
///
/// See [`chillpill::catch`].
///
/// # Panics
///
/// See [`chillpill::catch`].
///
/// [`chillpill::catch`]: crate::catch
pub fn catch_force_backtrace<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> Result<R> {
    catch_inner(f, CaptureBacktrace::Always)
}

/// Like [`chillpill::catch`], but never captures a backtrace. See its documentation for details.
///
/// # Errors
///
/// See [`chillpill::catch`].
///
/// # Panics
///
/// See [`chillpill::catch`].
///
/// [`chillpill::catch`]: crate::catch
pub fn catch_never_backtrace<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> Result<R> {
    catch_inner(f, CaptureBacktrace::Never)
}

fn catch_inner<F: FnOnce() -> R + UnwindSafe, R>(
    f: F,
    capture_backtrace: CaptureBacktrace,
) -> Result<R> {
    // Ensure the chillpill panic hook is installed
    if let Err(()) = panic_hook::install_if_not_installed() {
        panic!("the first call to `chillpill::catch` must not be made from a panicking thread");
    }

    // Push a new frame corresponding to this call to `catch_inner`. See the documentation on
    // `THREAD_LOCAL_CATCH_STACK` for details.
    THREAD_LOCAL_CATCH_STACK.with_borrow_mut(|stack| {
        stack.push(CatchStackFrame::new(capture_backtrace));
    });

    // Call the provided closure, using `std::panic::catch_unwind` to catch the panic payload and
    // prevent further unwinding
    let catch_unwind_result = std::panic::catch_unwind(f);

    // Pop this `catch_inner` call's frame. See the documentation on `THREAD_LOCAL_CATCH_STACK` for
    // details.
    let frame = THREAD_LOCAL_CATCH_STACK
        .with_borrow_mut(Vec::pop)
        .expect("catch stack should not be empty, since we just pushed a frame - this is a bug in chillpill");

    // If the closure panicked, combine the payload, location, and backtrace into a `PanicData`
    catch_unwind_result.map_err(|payload| {
        let location = frame.location;
        let backtrace = frame.backtrace;

        PanicData {
            payload,
            location,
            backtrace,
        }
    })
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

    /// This test ensures that [`chillpill::catch`] catches a panic (and its payload) that triggered
    /// within the closure it was given.
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

    /// A helper macro to store the location of the invocation of this macro in some variable before
    /// panicking.
    ///
    /// Relies on the fact that the expansion of the `file!`, `line!`, and `column!` macros will
    /// occur after the expansion of this macro.
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

    /// This test ensures that [`chillpill::catch`] records the correct panic location for a panic
    /// that triggered within the closure it was given.
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

    /// This test ensures that [`chillpill::catch`] captures the panic payload correctly when it
    /// isn't a [`String`] or [`&str`](str).
    ///
    /// [`chillpill::catch`]: crate::catch
    #[test]
    fn non_string_payload() {
        let result = catch(|| {
            let payload: Vec<i32> = vec![1, 2, 3];
            std::panic::panic_any(payload);
        })
        .unwrap_err();

        assert_eq!(*result.payload.downcast::<Vec<i32>>().unwrap(), &[1, 2, 3]);
    }

    /// This test ensures that multiple nested calls to [`chillpill::catch`] correctly catch the
    /// location and payloads of every panic.
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

    /// This test ensures that a [`std::panic::catch_unwind`] catching a panic within a
    /// [`chillpill::catch`] that does not panic behaves as expected.
    ///
    /// The expected behavior here is that the first `catch` call reports that the closure did not
    /// panic, and subsequent calls to `catch` are not affected.
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

    /// This test ensures that a [`std::panic::catch_unwind`] catching a panic within a
    /// [`chillpill::catch`] that does panic behaves as expected.
    ///
    /// The expected behavior here is that the first `catch` call reports that the closure panicked
    /// with the second panic's payload and location, and subsequent calls to `catch` are not
    /// affected.
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
