use std::{backtrace::Backtrace, panic::PanicHookInfo, sync::Once};

use crate::{
    panic_data::PanicLocation,
    thread_local_catch_stack::{CaptureBacktrace, THREAD_LOCAL_CATCH_STACK},
};

type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs the chillpill panic hook if it is not already installed.
///
/// # Errors
///
/// Returns an error without attempting to modify the panic hook if the current thread is panicking.
pub fn install_if_not_installed() -> Result<(), ()> {
    static CHILLPILL_HOOK_INSTALLED: Once = Once::new();

    // If the current thread is panicking, we cannot install the panic hook.
    //
    // There's fun tricks you can do with spawning a new thread and installing the hook there, but
    // unfortunately it's not a good idea in practice. Spawning threads can fail, so it would really
    // only make rare errors even rarer, but in a less controllable way. Not worth it, despite the
    // cleverness.
    if std::thread::panicking() {
        return Err(());
    }

    CHILLPILL_HOOK_INSTALLED.call_once(|| {
        // TODO(ijchen): use `std::panic::update_hook` once stable (#92649)
        let old_hook = std::panic::take_hook();
        let new_hook = Box::new(make_chillpill_panic_hook(old_hook));
        std::panic::set_hook(new_hook);
    });

    Ok(())
}

fn make_chillpill_panic_hook(
    previous_hook: PanicHook,
) -> impl Fn(&PanicHookInfo<'_>) + Send + Sync {
    move |info| {
        // Grab the top frame from `THREAD_LOCAL_CATCH_STACK` (or if it's empty, transparently
        // delegate to the previous panic hook)
        THREAD_LOCAL_CATCH_STACK.with_borrow_mut(|stack| {
            match stack.last_mut() {
                Some(top_frame) => {
                    // Smuggle out the panic location and backtrace, storing them in
                    // `THREAD_LOCAL_CATCH_STACK` to be extracted later.
                    top_frame.location = info.location().map(|location| PanicLocation {
                        file: location.file().to_string(),
                        line: location.line(),
                        col: location.column(),
                    });
                    top_frame.backtrace = match top_frame.capture_backtrace {
                        CaptureBacktrace::Always => Backtrace::force_capture(),
                        CaptureBacktrace::Default => Backtrace::capture(),
                        CaptureBacktrace::Never => Backtrace::disabled(),
                    };
                }

                // If `THREAD_LOCAL_CATCH_STACK` is empty, the panicking thread is not in a
                // `chillpill::catch` call - transparently delegate to the previous panic hook.
                None => previous_hook(info),
            }
        });
    }
}
