rline
=====

- [Documentation][docs-rs]
- [Changelog](CHANGELOG.md)

**rline** is a crate providing a convenient wrapper around libreadline's
["Alternate Interface"][libreadline]. It provides the goodness of
libreadline (its powerful support for inputting text as well as its
configurability) while leaving the actual character input in the hands
of a user (or developer). This can be a powerful tool for terminal based
(but not necessarily command line based) applications that want to
handle input on their own terms and only harness libreadline for a
subset of their text input needs. But even graphical applications, which
typically are not using file stream based input (or a terminal for that
matter) at all, are enabled to use libreadline for providing an input
experience as usually only known on the command line.


Example Usage
-------------

The [notnow][notnow] program is (optionally) using the **rline** crate
for the input of text in its terminal based UI. An example integration
can be seen there.

[docs-rs]: https://docs.rs/crate/rline
[libreadline]: https://tiswww.case.edu/php/chet/readline/readline.html#SEC41
[notnow]: https://crates.io/crates/notnow
