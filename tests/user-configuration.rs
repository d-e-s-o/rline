// Copyright (C) 2018-2023 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CString;

use libc::c_char;
use libc::c_int;

use rline::Readline;


extern "C" {
  fn rl_parse_and_bind(line: *mut c_char) -> c_int;
}


#[test]
fn with_user_configuration() {
  let mut rl = Readline::new();

  // Configure libreadline to accept the character sequence "jk" as an
  // indication to exit edit mode and switch into vi-movement-mode. Note
  // that it is of vital importance that this call be made after the
  // first `Readline` object was created. As part of this call
  // libreadline seems to be reading its global configuration, and with
  // that overwrite all customizations made beforehand.
  let mut line = CString::new("\"jk\": vi-movement-mode")
    .unwrap()
    .into_bytes_with_nul();

  let result = unsafe { rl_parse_and_bind(line.as_mut_ptr() as *mut c_char) };
  assert_eq!(result, 0);

  assert_eq!(rl.feed(b"abjka"), None);
  assert_eq!(rl.feed(b"\n").unwrap(), CString::new("ab").unwrap());

  rl = Readline::new();

  assert_eq!(rl.feed(b"abjka"), None);
  assert_eq!(rl.feed(b"\n").unwrap(), CString::new("ab").unwrap());
}
