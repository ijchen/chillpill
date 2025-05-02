use std::{
    num::NonZeroUsize,
    panic::PanicHookInfo,
    sync::{LazyLock, Mutex},
};

use crate::{PANIC_LOCATION_STACK, panic_data::PanicLocation};

/// Whether or not the panic hook is currently replaced by chillpill, and if so:
/// 1. how many threads are currently in a [`chillpill::catch`](crate::catch)
///    call (and are relying on the chillpill panic hook being set)
/// 2. the old panic hook to delegate to in case any unrelated threads panic
///    while we've hijacked the panic hook.
///
/// This is necessary since the panic hook is a shared resource across all
/// threads, and we want to handle potentially multiple concurrent
/// [`chillpill::catch`](crate::catch) calls gracefully.
static PANIC_HOOK_STATUS: LazyLock<Mutex<PanicHookStatus>> =
    LazyLock::new(|| Mutex::new(PanicHookStatus::NotReplaced));

/// Increments the count of threads relying on the chillpill panic hook,
/// replacing the global panic hook with the chillpill custom hook if it isn't
/// already replaced.
pub fn add_chillpill_thread() {
    let mut panic_hook_status = PANIC_HOOK_STATUS.lock().unwrap();

    match *panic_hook_status {
        // If the panic hook isn't already replaced, we have to replace it
        PanicHookStatus::NotReplaced => {
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(chillpill_panic_hook));

            *panic_hook_status = PanicHookStatus::Replaced {
                previous_hook,
                threads_in_catch: NonZeroUsize::new(1).unwrap(),
            };
        }

        // If the panic hook is already replaced, all we have to do is increment
        // the counter of threads relying on it.
        PanicHookStatus::Replaced {
            ref mut threads_in_catch,
            ..
        } => {
            let Some(new_threads_in_catch) = threads_in_catch.checked_add(1) else {
                // This really should never happen - it would mean we have more
                // threads than usize::MAX, which is pretty ridiculous
                eprintln!(
                    "overflow incrementing the number of threads concurrently calling `chillpill::catch`"
                );
                std::process::abort();
            };

            *threads_in_catch = new_threads_in_catch;
        }
    }

    // Critical section ends here - importantly, no other concurrent call to
    // this function (or remove_chillpill_thread) could race with each other and
    // mix up the global panic hook, since both hold on to the PANIC_HOOK_STATUS
    // lock the whole time they're messing with the global panic hook. We could
    // still race with other, unrelated code - this is one of the reasons it is
    // a requirement that no other code on any thread touches the global panic
    // hook during any call to `chillpill::catch`.
    drop(panic_hook_status);
}

/// Decrements the count of threads relying on the chillpill panic hook,
/// restoring the previous global panic hook if no more threads are relying on
/// the chillpill hook.
///
/// # Panics
///
/// If there are zero threads currently relying on the chillpill panic hook.
pub fn remove_chillpill_thread() {
    let mut panic_hook_status = PANIC_HOOK_STATUS.lock().unwrap();

    let PanicHookStatus::Replaced {
        threads_in_catch,
        previous_hook,
    } = std::mem::replace(&mut *panic_hook_status, PanicHookStatus::NotReplaced)
    else {
        // This should never happen, and would indicate a chillpill logic error
        panic!("`chillpill::remove_chillpill_thread` called while panic hook was not replaced")
    };

    match threads_in_catch.get() {
        // NonZeroUsize cannot be 0
        0 => unreachable!(),

        // We were the last thread relying on the chillpill panic hook
        1 => std::panic::set_hook(previous_hook),

        // There are still other threads relying on the chillpill panic hook
        n @ 2.. => {
            *panic_hook_status = PanicHookStatus::Replaced {
                previous_hook,
                threads_in_catch: NonZeroUsize::new(n - 1).unwrap(),
            }
        }
    }

    // Critical section ends here - importantly, no other concurrent call to
    // this function (or add_chillpill_thread) could race with each other and
    // mix up the global panic hook, since both hold on to the PANIC_HOOK_STATUS
    // lock the whole time they're messing with the global panic hook. We could
    // still race with other, unrelated code - this is one of the reasons it is
    // a requirement that no other code on any thread touches the global panic
    // hook during any call to `chillpill::catch`.
    drop(panic_hook_status);
}

enum PanicHookStatus {
    NotReplaced,
    Replaced {
        previous_hook: Box<dyn Fn(&PanicHookInfo<'_>) + 'static + Sync + Send>,
        threads_in_catch: NonZeroUsize,
    },
}

fn chillpill_panic_hook(info: &PanicHookInfo<'_>) {
    // If the panicking thread is not in a `chillpill::catch` call (and is some
    // unrelated panic that happened to occur while we've hijacked the global
    // panic hook), transparently delegate to the old panic hook.
    if PANIC_LOCATION_STACK.with_borrow(Vec::is_empty) {
        let PanicHookStatus::Replaced { previous_hook, .. } = &*PANIC_HOOK_STATUS.lock().unwrap()
        else {
            // We're currently executing the chillpill custom panic hook, so if
            // PANIC_HOOK_STATUS isn't `Replaced`, that's a logic error
            unreachable!()
        };

        previous_hook(info);

        return;
    }

    // Smuggle out the panic location, storing it in `PANIC_LOCATION_STACK` to
    // be extracted later.
    PANIC_LOCATION_STACK.with_borrow_mut(|stack| {
        *stack.last_mut().unwrap() = info.location().map(|location| PanicLocation {
            file: location.file().to_string(),
            line: location.line(),
            col: location.column(),
        });
    });
}
