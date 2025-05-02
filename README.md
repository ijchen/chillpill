# Chillpill

<!-- Badges -->
[![GitHub](https://img.shields.io/badge/Source-ijchen/chillpill-FFD639?labelColor=555555&logo=github)](https://github.com/ijchen/chillpill)
[![crates.io](https://img.shields.io/crates/v/chillpill?logo=rust)](https://crates.io/crates/chillpill)
[![docs.rs](https://img.shields.io/docsrs/chillpill?logo=docs.rs)](https://docs.rs/chillpill)
[![License](https://img.shields.io/crates/l/chillpill)](#)

A more powerful (and more restrictive) [`std::panic::catch_unwind`].

[`chillpill::catch`] is able to suppress the default panic message printed to
`stderr` and return the source code location (file, line, and column) of the
panic, at the cost of requiring that no other code modifies the
[global panic hook](https://doc.rust-lang.org/std/panic/fn.set_hook.html) during
its execution.

See the `chillpill::catch` documentation for a full list of differences from
`std::panic::catch_unwind`, and more information on the panic hook restriction.

# Example Usage

`chillpill::catch` is a drop-in replacement for `std::panic::catch_unwind`:

```rust
use chillpill::{PanicData, PanicLocation};

fn main() {
    // The API of `chillpill::catch` is the same as `std::panic::catch_unwind`
    //
    // The important differences are outlined in the documentation for `catch`,
    // but this example demonstrates that panic messages are suppressed, and the
    // location of the panic (file, line, and column) are available at runtime.
    let panic_result: Result<(), PanicData> = chillpill::catch(|| {
        // You won't see this message on stderr, chillpill prevents panic output
        panic!("Uh oh, I'm freaking out!!!");
    });

    let panic_data = panic_result.unwrap_err();
    assert_eq!(
        panic_data.payload_as_string(),
        Some("Uh oh, I'm freaking out!!!")
    );

    let PanicLocation { file, line, col } = panic_data.location.unwrap();
    println!("The panic occurred in {file} on line {line}, column {col}.");
}
```
```text
$ cargo run
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running `target/x86_64-unknown-linux-gnu/debug/example`
The panic occurred in src/main.rs on line 11, column 9.
```

# License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

# Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

<!--
docs.rs documentation links for rendered markdown (ex, on GitHub)
These are overridden when include_str!(..)'d in lib.rs
-->
[`std::panic::catch_unwind`]: https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
[`chillpill::catch`]: https://docs.rs/chillpill/0.1.0/chillpill/fn.catch.html
