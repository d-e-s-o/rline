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
