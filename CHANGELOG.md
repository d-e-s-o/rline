0.3.4
-----
- Honor system bitness when configuring native library search path
- Adjusted crate to use Rust Edition 2021
- Bumped minimum required Rust version to `1.63.0`


0.3.3
-----
- Decreased `Readline` object size by heap allocating more state
- Fixed potential memory leak caused by undo lists
- Bumped minimum supported Rust version to `1.38`


0.3.2
-----
- Introduced `static` feature for linking to `libreadline` statically
- Added GitHub Actions workflow for publishing the crate


0.3.1
-----
- Re-run build script on changes to `CARGO_CFG_TARGET_OS` env var
- Switched to using GitHub Actions as CI provider
- Updated example to use `termion` `2.0`


0.3.0
-----
- Bumped minimum required Rust version to `1.36.0`
- Replaced deprecated `std::mem::uninitialized` with usage of
  `std::mem::MaybeUninit`
- Added code coverage collection and reporting to CI pipeline
- Downgraded `deny` crate-level lints to `warn`


0.2.1
-----
- Adjusted `peek` method to no longer require a mutable object


0.2.0
-----
- Renamed `inspect` method to `peek`
- Adjusted `feed` to accept a `[u8]` instead of a `c_int`
  - Properly support multi-byte inputs
- Added `links` manifest key to `Cargo.toml`


0.1.4
-----
- Fixed use-after-free bug in `user-configuration` test
- Enabled CI pipeline comprising building, testing, and linting of the
  project
- Added badges indicating pipeline status, current `crates.io` published
  version of the crate, current `docs.rs` published version of the
  documentation, and minimum version of `rustc` required


0.1.3
-----
- Added example illustrating basic usage of `Readline` objects in a
  terminal application
- Adjusted program to use Rust Edition 2018
- Removed `#![deny(warnings)]` attribute and demoted lints prone to
  future changes from `deny` to `warn`


0.1.2
-----
- Fixed wrong lifetime being used for `&CStr` parameter in function
  passed to `Readline::inspect` method
- Added categories to Cargo.toml


0.1.1
-----
- Fixed bug causing user configuration to be only active for the first
  `Readline` context created
- Made sure to release initially `libreadline` allocated buffers to
  prevent one-time memory leak
- Implemented `Default` trait for `Readline` struct


0.1.0
-----
- Initial release
