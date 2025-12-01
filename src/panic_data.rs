use std::{any::Any, backtrace::Backtrace, borrow::Cow, fmt::Display};

/// The payload and source code location of a panic.
pub struct PanicData {
    /// The payload associated with the panic.
    pub payload: Box<dyn Any + Send + 'static>,

    /// The source code location of the panic, or [`None`] if no source location was available.
    ///
    /// The current implementation of [`std::panic::PanicHookInfo::location`] always returns
    /// [`Some`], and as a result the current implementation of [`chillpill::catch`]
    /// always returns [`Some`]. This may change in a future Rust version, or even in a minor or
    /// patch update to `chillpill`.
    ///
    /// [`chillpill::catch`]: crate::catch
    pub location: Option<PanicLocation>,

    /// A backtrace captured at the time of the panic (specifically, within the panic hook).
    ///
    /// Note that although this field isn't an [`Option`], it may be a disabled backtrace. This is
    /// always the case when calling [`chillpill::catch_never_backtrace`], and can also occur when
    /// calling [`chillpill::catch`] depending on environment variable configuration.
    ///
    /// [`chillpill::catch_never_backtrace`]: crate::catch_never_backtrace
    /// [`chillpill::catch`]: crate::catch
    pub backtrace: Backtrace,
}

impl std::fmt::Debug for PanicData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanicData")
            .field(
                "payload",
                match self.payload_as_string().as_ref() {
                    Some(msg) => msg,
                    None => &self.payload,
                },
            )
            .field("location", &self.location)
            .field("backtrace", &self.backtrace)
            .finish()
    }
}

impl PanicData {
    /// Attempts to convert the panic payload to a string (either [`&str`](str) or [`String`]),
    /// returning [`None`] if the payload was neither `&str` nor `String`.
    pub fn payload_as_string(&self) -> Option<&str> {
        // Try downcasting to a &str
        if let Some(s) = self.payload.downcast_ref::<&str>() {
            return Some(s);
        }

        // Downcasting to a &str failed, try downcasting to a String
        if let Some(s) = self.payload.downcast_ref::<String>() {
            return Some(s);
        }

        // Downcasting to a String failed, give up and return None
        None
    }

    /// Attempts to convert the panic payload to a string (either [`&str`](str) or [`String`]).
    ///
    /// # Errors
    ///
    /// Returns `self` back if the panic payload was neither a `&str` nor a `String`.
    pub fn payload_into_string(self) -> Result<Cow<'static, str>, Self> {
        let Self {
            payload,
            location,
            backtrace,
        } = self;

        // Try downcasting to a &str
        let payload = match payload.downcast::<&str>() {
            Ok(s) => return Ok(Cow::Borrowed(*s)),
            Err(any) => any,
        };

        // Downcasting to a &str failed, try downcasting to a String
        let payload = match payload.downcast::<String>() {
            Ok(s) => return Ok(Cow::Owned(*s)),
            Err(any) => any,
        };

        // Downcasting to a String failed, give up and return a re-created Self
        Err(Self {
            payload,
            location,
            backtrace,
        })
    }
}

/// The source code location of a panic.
//
// TODO(ichen): I'd really like this to be Copy and hold `file: &'static str`, but that is blocked
// on https://github.com/rust-lang/rust/pull/146561
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanicLocation {
    /// The source code file name where the panic was triggered.
    pub file: String,

    /// The source code line number where the panic was triggered.
    pub line: u32,

    /// The source code column number where the panic was triggered.
    pub col: u32,
}

impl Display for PanicLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a [`PanicData`] from an unboxed payload.
    fn make_panic_data<T: Any + Send + 'static>(
        payload: T,
        location: Option<PanicLocation>,
        backtrace: Backtrace,
    ) -> PanicData {
        PanicData {
            payload: Box::new(payload),
            location,
            backtrace,
        }
    }

    /// Produces something with a [`std::fmt::Debug`] impl matching what [`PanicData`] would be if
    /// it had a `#[derive(Debug)]` that formats the payload as `expected_payload`
    fn panic_data_debug_match(
        expected_payload: impl std::fmt::Debug + 'static,
        location: Option<PanicLocation>,
        backtrace: Backtrace,
    ) -> impl std::fmt::Debug {
        /// A struct with an identical structure to [`super::PanicData`], used to make sure our
        /// manual [`std::fmt::Debug`] impl matches what `#[derive(...)]` would generate (besides
        /// our custom payload formatting)
        #[derive(Debug)]
        struct PanicData {
            #[expect(dead_code, reason = "we actually care about the derived Debug")]
            payload: Box<dyn std::fmt::Debug>,
            #[expect(dead_code, reason = "we actually care about the derived Debug")]
            location: Option<PanicLocation>,
            #[expect(dead_code, reason = "we actually care about the derived Debug")]
            backtrace: Backtrace,
        }

        PanicData {
            payload: Box::new(expected_payload),
            location,
            backtrace,
        }
    }

    /// Helper function to assert that two `impl std::fmt::Debug`s format identically across a range
    /// of format arguments.
    fn assert_debugs_match(a: impl std::fmt::Debug, b: impl std::fmt::Debug) {
        // Normal debug (and pretty-print)
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        assert_eq!(format!("{a:#?}"), format!("{b:#?}"));

        // Hex integers (lowercase and uppercase)
        // See: https://doc.rust-lang.org/std/fmt/index.html#formatting-traits
        assert_eq!(format!("{a:x?}"), format!("{b:x?}"));
        assert_eq!(format!("{a:X?}"), format!("{b:X?}"));
        assert_eq!(format!("{a:#x?}"), format!("{b:#x?}"));
        assert_eq!(format!("{a:#X?}"), format!("{b:#X?}"));

        // Getting crazy with it (I'm not gonna test every combination, but I'm down to just throw a
        // bunch of random stuff at it and make sure that works out)
        //
        // The "🦀^+#12.5?" means: ferris fill, center aligned, with sign, pretty printed, no "0"
        // option integer formatting (would override fill/align), width 12, 5 digits of precision,
        // debug formatted.
        //
        // See: https://doc.rust-lang.org/std/fmt/index.html#formatting-parameters
        assert_eq!(format!("{a:🦀^+#12.5?}"), format!("{b:🦀^+#12.5?}"));
        assert_eq!(format!("{a:🦀^+#12.5?}"), format!("{b:🦀^+#12.5?}"));
    }

    /// This test ensures that [`PanicData`]'s manual [`std::fmt::Debug`] impl behaves identically
    /// to a `#[derive(..)]`'d impl, except that it formats [`&str`](str) payloads as a string.
    #[test]
    fn debug_formats_str_payload_correctly() {
        let location = Some(PanicLocation {
            file: String::from("example_file.rs"),
            line: 42,
            col: 7,
        });
        let data = make_panic_data("&str payload", location.clone(), Backtrace::disabled());

        assert_debugs_match(
            data,
            panic_data_debug_match("&str payload", location, Backtrace::disabled()),
        );
    }

    /// This test ensures that [`PanicData`]'s manual [`std::fmt::Debug`] impl behaves identically
    /// to a `#[derive(..)]`'d impl, except that it formats [`String`] payloads as a string.
    #[test]
    fn debug_formats_string_payload_correctly() {
        let location = Some(PanicLocation {
            file: String::from("example_file.rs"),
            line: 42,
            col: 7,
        });
        let data = make_panic_data(
            String::from("String payload"),
            location.clone(),
            Backtrace::disabled(),
        );

        assert_debugs_match(
            data,
            panic_data_debug_match("String payload", location, Backtrace::disabled()),
        );
    }

    /// This test ensures that [`PanicData`]'s manual [`std::fmt::Debug`] impl behaves identically
    /// to a `#[derive(..)]`'d impl, including formatting the payload as a Box<dyn Any> when it is
    /// neither a [`&str`](str) nor a [`String`].
    #[test]
    fn debug_formats_non_string_payload_correctly() {
        let location = Some(PanicLocation {
            file: String::from("example_file.rs"),
            line: 42,
            col: 7,
        });
        let data = make_panic_data((), location.clone(), Backtrace::disabled());

        assert_debugs_match(
            data,
            panic_data_debug_match(
                Box::new(()) as Box<dyn Any>,
                location,
                Backtrace::disabled(),
            ),
        );
    }

    /// This test ensures [`PanicData::payload_as_string`] correctly extracts a [`&str`](str)
    /// payload.
    #[test]
    fn payload_as_string_str() {
        let panic_data = make_panic_data("static str", None, Backtrace::disabled());

        assert_eq!(panic_data.payload_as_string(), Some("static str"));
    }

    /// This test ensures [`PanicData::payload_as_string`] correctly extracts a [`String`] payload.
    #[test]
    fn payload_as_string_string() {
        let panic_data = make_panic_data(String::from("owned string"), None, Backtrace::disabled());

        assert_eq!(panic_data.payload_as_string(), Some("owned string"));
    }

    /// This test ensures [`PanicData::payload_as_string`] correctly returns [`None`] for a payload
    /// that is neither a [`&str`](str) nor a [`String`].
    #[test]
    fn payload_as_string_non_string() {
        let panic_data = make_panic_data(42u8, None, Backtrace::disabled());

        assert_eq!(panic_data.payload_as_string(), None);
    }

    /// This test ensures [`PanicData::payload_into_string`] correctly extracts a [`&str`](str)
    /// payload.
    #[test]
    fn payload_into_string_str() {
        let panic_data = make_panic_data("static str", None, Backtrace::disabled());
        let result = panic_data.payload_into_string();

        assert!(matches!(result, Ok(Cow::Borrowed("static str"))));
    }

    /// This test ensures [`PanicData::payload_into_string`] correctly extracts a [`String`]
    /// payload.
    #[test]
    fn payload_into_string_string() {
        let panic_data = make_panic_data(String::from("owned string"), None, Backtrace::disabled());
        let result = panic_data.payload_into_string();

        assert!(matches!(result, Ok(Cow::Owned(ref s)) if s == "owned string"));
    }

    /// This test ensures [`PanicData::payload_into_string`] correctly returns [`None`] for a
    /// payload that is neither a [`&str`](str) nor a [`String`].
    #[test]
    fn payload_into_string_non_string() {
        let panic_data = make_panic_data(1234u32, None, Backtrace::disabled());
        let result = panic_data.payload_into_string().unwrap_err();

        assert_eq!(*result.payload.downcast::<u32>().unwrap(), 1234u32);
    }
}
