use std::{backtrace::Backtrace, cell::RefCell};

use crate::PanicLocation;

thread_local! {
    /// A thread-local stack of [`CatchStackFrame`]s, used to enable communication between
    /// [`chillpill::catch`] calls and the custom panic hook. Specifically, calls to `catch` need
    /// to tell the panic hook whether or not to capture a backtrace, and the panic hook needs a way
    /// to smuggle the panic locations and backtraces back to `catch` calls. This thread-local stack
    /// enables both.
    ///
    /// Each frame on this stack corresponds to one active call to [`chillpill::catch`] (or more
    /// precisely, `catch_inner`) somewhere in the current thread's call stack.
    ///
    /// On any given thread, this stack starts out initially empty. Every time `catch` is called,
    /// another frame is pushed to the top of the stack, where it will live until the end of that
    /// `catch` call. Nested calls to `catch` will grow the stack.
    ///
    /// Each frame starts with [`None`] for its panic location, [`Backtrace::disabled()`] for its
    /// backtrace, and a [`CaptureBacktrace`] indicating whether or not to capture a backtrace.
    /// Anytime a panic occurs on a thread, the chillpill custom panic hook will inspect the top
    /// frame of the stack to determine if it should capture a backtrace, and then stashes away the
    /// panic location and backtrace in that frame. If some panics within a `catch` call are caught
    /// before unwinding all the way back to `catch` (e.g., via [`std::panic::catch_unwind`]), the
    /// panic location that was recorded for them at the top of the stack is not relevant (and would
    /// be incorrect for `catch` to use). This is usually not a problem, because `catch` will only
    /// extract and use the panic location if it actually catches an unwinding panic - and if it
    /// does, it is likely that that panic recorded its location at the top of the panic location
    /// stack, overwriting any previous incorrect location. The only exception to this is if the
    /// caught panic did *not* invoke the panic hook. This is rare, but can happen (e.g., via
    /// [`std::panic::resume_unwind`]). This situation is further documented in the documentation
    /// for [`catch`].
    ///
    /// Here is a basic summary of how this stack is used by both `catch` and our custom panic hook:
    ///
    /// Each call to `catch` does the following:
    /// 1. Push a new frame containing whether to capture a backtrace, and room to later store the
    ///    panic location and captured backtrace.
    /// 2. Run the user-provided closure, catching any unwinding panics that occur.
    /// 3. Pop the frame and, if the closure unwound, extract the panic location and backtrace.
    ///
    /// Meanwhile, our custom global panic hook:
    /// - Delegates to the real hook if the thread local stack is empty
    /// - Otherwise, uses the top stack frame to determine whether to capture a backtrace, and as a
    ///   storage location to smuggle out the panic location and captured backtrace.
    ///
    /// [`chillpill::catch`]: crate::catch
    pub static THREAD_LOCAL_CATCH_STACK: RefCell<Vec<CatchStackFrame>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub struct CatchStackFrame {
    /// When to capture backtrace - provided by the call to `catch`.
    pub capture_backtrace: CaptureBacktrace,

    /// The captured panic location - set in our custom panic hook on panics.
    ///
    /// This is set to `None` initially (before any panics), and the result of
    /// `PanicHookInfo::location` for the most recent hook-invoking panic afterwards. Note that this
    /// may still be `None` even after a panic in two situations:
    /// 1. If `PanicHookInfo::location` returns `None` in the panic hook.
    /// 2. If the panic hook is not invoked for the panic (e.g., via `std::panic::resume_unwind`).
    pub location: Option<PanicLocation>,

    /// The captured backtrace - set in our custom panic hook on panics.
    ///
    /// This is set to a disabled backtrace initially (before any panics), and a [`Backtrace`] for
    /// the most recent hook-invoking panic afterwards. Whether or not a backtrace is actually
    /// captured depends on the value of `capture_backtrace` (see [`CaptureBacktrace`]). Note that
    /// this may still be a disabled backtrace even after a panic if the panic hook is not invoked
    /// for the panic (e.g., via `std::panic::resume_unwind`).
    pub backtrace: Backtrace,
}

impl CatchStackFrame {
    pub fn new(capture_backtrace: CaptureBacktrace) -> Self {
        Self {
            capture_backtrace,
            location: None,
            backtrace: Backtrace::disabled(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CaptureBacktrace {
    /// Always capture a backtrace.
    Always,

    /// Capture a backtrace only if enabled via the `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE`
    /// environment variables.
    ///
    /// See: [`std::backtrace::Backtrace::capture`]
    Default,

    /// Never capture a backtrace.
    Never,
}
