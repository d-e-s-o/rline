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
