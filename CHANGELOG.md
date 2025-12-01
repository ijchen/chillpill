# Major Version 0

## v0.2.1 (IN DEVELOPMENT - ON_RELEASE: ensure entry is updated, remove this note)

- Added backtrace support
  - `PanicData` now includes a `backtrace` field
  - `catch` now captures a backtrace (depending on environment variable configuration)
  - Added `catch_force_backtrace` and `catch_never_backtrace` functions
- Fixed a rare race condition that could cause a deadlock if the last active `catch` call was
  cleaning up at the same time as an unrelated panic on another thread
  - As a result, the API requirements for `catch` (and family) have changed - `catch` panics in
    fewer scenarios, but the chillpill panic hook is no longer uninstalled and reinstalled when
    there are no active `catch` calls
- Implemented `Display` for `PanicLocation`

## v0.2.0

- Added backtrace support
  - `PanicData` now includes a `backtrace` field
  - `catch` now captures a backtrace (depending on environment variable configuration)
  - Added `catch_force_backtrace` and `catch_never_backtrace` functions
- Fixed a rare race condition that could cause a deadlock if the last active `catch` call was
  cleaning up at the same time as an unrelated panic on another thread
  - As a result, the API requirements for `catch` (and family) have changed - `catch` panics in
    fewer scenarios, but the chillpill panic hook is no longer uninstalled and reinstalled when
    there are no active `catch` calls
- Implemented `Display` for `PanicLocation`

## v0.1.0

Initial release. Defines the core API, including:

- Defined `catch` function
- Defined `PanicData` struct
- Defined `PanicLocation` struct
- Defined `Result<T>` type alias
