// user-configuration.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
// *                                                                       *
// * This program is free software: you can redistribute it and/or modify  *
// * it under the terms of the GNU General Public License as published by  *
// * the Free Software Foundation, either version 3 of the License, or     *
// * (at your option) any later version.                                   *
// *                                                                       *
// * This program is distributed in the hope that it will be useful,       *
// * but WITHOUT ANY WARRANTY; without even the implied warranty of        *
// * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
// * GNU General Public License for more details.                          *
// *                                                                       *
// * You should have received a copy of the GNU General Public License     *
// * along with this program.  If not, see <http://www.gnu.org/licenses/>. *
// *************************************************************************

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
  let line = CString::new("\"jk\": vi-movement-mode")
    .unwrap()
    .into_bytes_with_nul()
    .as_mut_ptr();

  let result = unsafe { rl_parse_and_bind(line as *mut c_char) };
  assert_eq!(result, 0);

  assert_eq!(rl.feed('a' as u8), None);
  assert_eq!(rl.feed('b' as u8), None);
  assert_eq!(rl.feed('j' as u8), None);
  assert_eq!(rl.feed('k' as u8), None);
  assert_eq!(rl.feed('a' as u8), None);
  assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("ab").unwrap());

  rl = Readline::new();

  assert_eq!(rl.feed('a' as u8), None);
  assert_eq!(rl.feed('b' as u8), None);
  assert_eq!(rl.feed('j' as u8), None);
  assert_eq!(rl.feed('k' as u8), None);
  assert_eq!(rl.feed('a' as u8), None);
  assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("ab").unwrap());
}
