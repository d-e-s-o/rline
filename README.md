[![pipeline](https://github.com/d-e-s-o/rline/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/d-e-s-o/rline/actions/workflows/test.yml)
[![coverage](https://codecov.io/gh/d-e-s-o/rline/branch/main/graph/badge.svg)](https://codecov.io/gh/d-e-s-o/rline)
[![crates.io](https://img.shields.io/crates/v/rline.svg)](https://crates.io/crates/rline)
[![Docs](https://docs.rs/rline/badge.svg)](https://docs.rs/rline)
[![rustc](https://img.shields.io/badge/rustc-1.36+-blue.svg)](https://blog.rust-lang.org/2019/07/04/Rust-1.36.0.html)

rline
=====

- [Documentation][docs-rs]
- [Changelog](CHANGELOG.md)

**rline** is a crate providing a convenient wrapper around
`libreadline`'s ["Alternate Interface"][libreadline]. It provides the
goodness of `libreadline` (its ubiquity, powerful support for inputting
text, as well as configurability) while leaving the actual character
input in the hands of a user (or developer).

This can be a powerful tool for terminal based (but not necessarily
command line based) applications that want to handle input on their own
terms and only harness `libreadline` for a subset of their text input
needs.
But even graphical applications, which typically are not using file
stream based input (or a terminal for that matter) at all, are enabled
to use `libreadline` for providing an input experience as usually only
known on the command line.


Usage
-----

Integration of the crate into an application or library generally
contains the following parts:
```rust
// Create a "context" for interacting with libreadline. Contexts are
// isolated from each other, such that you can keep input state and
// history around on a per-object basis (for example, per text input
// field).
let mut rl = rline::Readline::new();

// ...

// Feed data to libreadline and check what the result is. Globally
// configured settings (e.g., via /etc/inputrc) are honored. The result
// is either a completed line or `None`, if editing is still in
// progress.
if let Some(line) = rl.feed(&raw_input) {
  // We got a completed line. Work with to it.
  my_process(&line);
} else {
  // Editing is still in progress. Depending on the use-case we may want
  // to look at the current line input so far as well as the cursor
  // position, for example, to update the screen accordingly.
  rl.peek(|line_so_far, cursor| {
    my_display(&line_so_far, cursor)
  });
};

// ...

// If the user supplied text out-of-band, e.g., by pasting it via a
// cursor based input device, the libreadline state can be adjusted
// accordingly:
let new_line = CStr::from_bytes_with_nul(b"copied-and-pasted\0").unwrap();
let new_cursor = 6;
let clear_undo = true;
rl.reset(new_line, new_cursor, clear_undo);
```

Please note that **rline** requires `libreadline` to be available on the
system. It does not support alternative line-editing implementations
such as `libedit`.


Examples
--------

A [ready-to-compile example][rline-example] showing the usage of the
crate to get line-editing support for a command line based applications
is available.

The [notnow][notnow] program is (optionally) using the **rline** crate
for the input of text in its terminal based UI. Another example
integration can be found there.

[docs-rs]: https://docs.rs/crate/rline
[libreadline]: https://tiswww.case.edu/php/chet/readline/readline.html#SEC41
[rline-example]: https://github.com/d-e-s-o/rline/blob/main/examples/basic.rs
[notnow]: https://crates.io/crates/notnow
