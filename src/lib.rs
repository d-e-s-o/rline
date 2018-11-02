// lib.rs

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

#![allow(
  unknown_lints,
  block_in_if_condition_stmt,
  redundant_field_names,
)]
#![deny(
  future_incompatible,
  missing_debug_implementations,
  missing_docs,
  rust_2018_compatibility,
  rust_2018_idioms,
  unstable_features,
  unused_import_braces,
  unused_qualifications,
  unused_results,
  warnings,
)]

//! A crate for reading a line using libreadline. Contrary to many other
//! crates, a character-based interface for inputting text is provided,
//! which allows for externally managed reading of input. That is, by
//! default libreadline takes ownership of stdin, stdout, and the
//! terminal. We try as hard as we can to set things up to not have it
//! do that. With that, the only interface that is necessary is one for
//! feeding of a single character, that could have been retrieved by any
//! means (including through events in X11 or other graphical
//! environments).
//!
//! Note that libreadline does not have a clear separation between the
//! core logic of handling input (based on characters) and displaying
//! them. It is highly questionable whether this crate achieved a 100%
//! isolation.

extern crate libc;
extern crate uid;

use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::fmt::Error;
use std::fmt::Formatter;
use std::mem::replace;
use std::mem::uninitialized;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::null;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::Once;
use std::sync::TryLockError;

use libc::c_char;
use libc::c_int;
use libc::c_void;
use libc::calloc;
use libc::free;

use uid::Id as IdT;

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct T(());

type Id = IdT<T>;


#[allow(non_camel_case_types)]
type rl_voidfunc_t = extern "C" fn();
#[allow(non_camel_case_types)]
type rl_vintfunc_t = extern "C" fn(c_int);
#[allow(non_camel_case_types)]
type rl_vcpfunc_t = unsafe extern "C" fn(*mut c_char);
#[allow(non_camel_case_types)]
type rl_hook_func_t = extern "C" fn() -> c_int;


// Declarations as provided by libreadline.
extern "C" {
  static mut rl_line_buffer: *mut c_char;
  static mut rl_line_buffer_len: c_int;
  static mut rl_point: c_int;
  static mut rl_end: c_int;

  static mut rl_executing_keyseq: *mut c_char;
  static mut rl_key_sequence_length: c_int;

  static mut rl_input_available_hook: *mut rl_hook_func_t;

  static mut rl_catch_signals: c_int;
  static mut rl_catch_sigwinch: c_int;

  static mut rl_redisplay_function: *mut rl_voidfunc_t;
  static mut rl_prep_term_function: *mut rl_vintfunc_t;
  static mut rl_deprep_term_function: *mut rl_voidfunc_t;

  fn rl_callback_handler_install(prompt: *const c_char, handler: *mut rl_vcpfunc_t);
  fn rl_stuff_char(c: c_int) -> c_int;
  fn rl_callback_read_char();
  fn rl_replace_line(text: *const c_char, clear_undo: c_int);

  fn rl_save_state(state: *mut readline_state) -> c_int;
  // Note that the actual prototype accepts a mutable pointer to
  // `readline_state`. Const correctness is not easy...
  fn rl_restore_state(state: *const readline_state) -> c_int;
}


/// A rough approximation of libreadline's `readline_state`. We treat
/// the content as opaque. We are vastly over estimating the size of the
/// actual struct. The size is not constant (due to usage of `c_int`).
/// We are not interested in accessing individual fields.
#[repr(C, align(8))]
#[derive(Clone)]
struct readline_state([u8; 512]);

impl readline_state {
  /// Load the state from libreadline's globals.
  fn load(&mut self) {
    let result = unsafe { rl_save_state(self) };
    assert_eq!(result, 0);
  }

  /// Save the state into libreadline's globals.
  fn save(&self) {
    let result = unsafe { rl_restore_state(self) };
    assert_eq!(result, 0);
  }
}

impl Debug for readline_state {
  fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
    f.debug_struct("readline_state").finish()
  }
}


trait Locked {
  /// Check whether a lock is currently held.
  fn is_locked(&self) -> bool;
}

impl<T> Locked for Mutex<T> {
  fn is_locked(&self) -> bool {
    self.try_lock().err().map_or(false, |x| {
      match x {
        TryLockError::WouldBlock => true,
        _ => false,
      }
    })
  }
}


/// A wrapper for `MutexGuard` ensuring that our libreadline state is read back before dropping.
struct ReadlineGuard<'data, 'slf> {
  _guard: MutexGuard<'data, Id>,
  rl: &'slf mut Readline,
}

impl<'data, 'slf> Deref for ReadlineGuard<'data, 'slf> {
  type Target = Readline;

  fn deref(&self) -> &Self::Target {
    &self.rl
  }
}

impl<'data, 'slf> DerefMut for ReadlineGuard<'data, 'slf> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.rl
  }
}

impl<'data, 'slf> Drop for ReadlineGuard<'data, 'slf> {
  fn drop(&mut self) {
    // Before unlocking (by virtue of dropping the embedded guard)
    // always make sure to read back the most recent version of the
    // state from the globals.
    self.rl.state.load()
  }
}


/// A struct representing a context for reading a line using libreadline.
#[derive(Debug)]
pub struct Readline {
  id: Id,
  state: readline_state,
}

impl Readline {
  /// Stub used as a terminal preparation function.
  extern "C" fn initialize_term(_: c_int) {}

  /// Stub used as a terminal "unpreparation" function.
  extern "C" fn uninitialize_term() {}

  /// Stub used as a display function.
  extern "C" fn display() {}

  /// Stub used as a callback to check whether new input is available.
  /// We explicitly feed input on demand, so we never want libreadline
  /// to read from stdin.
  extern "C" fn input_available() -> c_int {
    // We feed input explicitly, so there is never ever something
    // available on our input streams.
    0
  }

  /// A callback invoked when libreadline has completed a line.
  ///
  /// This function can only be invoked indirectly through the `feed`
  /// method. As such, we are guaranteed mutual exclusion with respect
  /// to global libreadline state.
  extern "C" fn handle_line(line: *mut c_char) {
    debug_assert!(Self::mutex().is_locked());

    if line.is_null() {
      let _ = replace(Self::line(), Some(CString::new("").unwrap()));
    } else {
      unsafe {
        let _ = replace(Self::line(), Some(CStr::from_ptr(line).into()));
        free(line as *mut c_void);
      }
    }
  }

  /// Create a new `Readline` instance.
  ///
  /// # Panics
  ///
  /// Panics on failure to allocate internally used C objects.
  pub fn new() -> Self {
    let mut rl = Self {
      id: Id::new(),
      state: Self::initial().clone(),
    };

    {
      // Make sure that the new state is activated.
      // TODO: Strictly speaking we could omit the load operation
      //       happening when the guard leaves the scope. We know that
      //       the state is current, so it just wastes cycles.
      let mut guard = rl.activate();

      unsafe {
        debug_assert!(rl_line_buffer.is_null());
        debug_assert!(rl_executing_keyseq.is_null());

        // Unfortunately `readline_state` contains some data that is
        // allocated by libreadline itself, as part of its
        // initialization. Because we create a new context we need to
        // reinitialize this data.
        rl_line_buffer = calloc(1, 64) as *mut c_char;
        rl_line_buffer_len = 64;
        rl_executing_keyseq = calloc(1, 16) as *mut c_char;
        rl_key_sequence_length = 16;

        // We use similar behavior to default Rust and panic on
        // allocation failure.
        assert!(!rl_line_buffer.is_null(), "failed to allocate rl_line_buffer");
        assert!(!rl_executing_keyseq.is_null(), "failed to allocate rl_executing_keyseq");
      }

      // We allocated some memory with the new addresses going directly
      // into libreadline's globals. So make sure to read back that
      // state to have an up-to-date snapshot.
      guard.state.load();
      // Believe it or not, but libreadline aliases the line buffer
      // internally with a pointer, and only storing the state back into
      // the global will update this pointer. So we need this additional
      // save here. Yes, that one is a pearl.
      guard.state.save();
    }

    rl
  }

  /// Retrieve the pristine initial `readline_state` as it was set by libreadline.
  fn initial() -> &'static readline_state {
    // We effectively cache a version of `readline_state` as it was set
    // by libreadline before anything could have changed. This state
    // acts as the template for all the states we create later on.
    static mut STATE: Option<readline_state> = None;
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
      // We should be safe *not* using our all-protecting mutex here
      // because this functionality is invoked only as the very first
      // interaction with libreadline, by virtue of being used only in
      // the constructor of objects of the one struct that has exclusive
      // access to libreadline's global state.
      let mut state = readline_state(uninitialized());

      // Disable a bunch of libreadline stuff that would mess up things
      // we don't want messed up, most prominently signal handler state
      // and terminal state.
      // This is all state that is part of `readline_state`, so we make
      // those changes once for the template and don't have to worry
      // about them again.
      rl_catch_signals = 0;
      rl_catch_sigwinch = 0;
      rl_input_available_hook = Self::input_available as *mut rl_hook_func_t;
      rl_redisplay_function = Self::display as *mut rl_voidfunc_t;
      rl_prep_term_function = Self::initialize_term as *mut rl_vintfunc_t;
      rl_deprep_term_function = Self::uninitialize_term as *mut rl_voidfunc_t;

      // Note that we do not ever invoke rl_callback_handler_remove.
      // This crate's assumption is that it is the sole user of
      // libreadline meaning nobody else will mess with global state. As
      // such, and because we set the same handler for all contexts,
      // there is no point in doing additional work to remove it. In
      // addition, due to the retardedness of libreadline and it not
      // capturing even all of its own global state, we could not even
      // remove the handler if we wanted to, because activating a
      // `readline_state` object would not set the handler. Sigh.
      rl_callback_handler_install(null::<c_char>(), Self::handle_line as *mut rl_vcpfunc_t);

      // libreadline already has buffers allocated but we won't be using
      // them.
      free(rl_line_buffer as *mut c_void);
      free(rl_executing_keyseq as *mut c_void);

      rl_line_buffer = null::<c_char>() as *mut c_char;
      rl_executing_keyseq = null::<c_char>() as *mut c_char;

      state.load();

      STATE = Some(state);
    });

    match unsafe { &STATE } {
      Some(state) => state,
      None => unreachable!(),
    }
  }

  /// Retrieve a reference to the `Mutex` protecting all accesses to
  /// libreadline's global state.
  fn mutex() -> &'static Mutex<Id> {
    static mut MUTEX: Option<Mutex<Id>> = None;
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe { MUTEX = Some(Mutex::new(Id::new())) });

    match unsafe { &MUTEX } {
      Some(mutex) => mutex,
      None => unreachable!(),
    }
  }

  /// A reference to the global line storage.
  fn line() -> &'static mut Option<CString> {
    static mut LINE: Option<CString> = None;
    debug_assert!(Self::mutex().is_locked());

    unsafe { &mut LINE }
  }

  /// Activate this context.
  fn activate<'slf, 'data: 'slf>(&'slf mut self) -> ReadlineGuard<'data, 'slf> {
    let mut guard = Self::mutex().lock().unwrap();

    // Activate our state if necessary.
    if *guard != self.id {
      self.state.save();
      *guard = self.id;
    }

    ReadlineGuard {
      _guard: guard,
      rl: self,
    }
  }

  /// Feed a character to libreadline.
  pub fn feed<C>(&mut self, c: C) -> Option<CString>
  where
    C: Into<c_int>,
  {
    let _guard = self.activate();

    unsafe {
      // This call will only fail if there is not enough space available
      // to push the given character (with libreadline specifying a
      // buffer size large enough for 512 characters). As we feed one
      // character at a time and process (i.e., consume) it immediately
      // afterwards, there is no risk of us ever hitting this limit.
      let result = rl_stuff_char(c.into());
      assert_ne!(result, 0);
      rl_callback_read_char();
    }

    Self::line().take()
  }

  /// Reset libreadline's line state to the given line with the given
  /// cursor position. If `clear_undo` is set, the undo list associated
  /// with the current line is cleared
  ///
  /// Note that this method does not deal with input related modes. For
  /// example, libreadline always starts in input mode, but, depending
  /// on user configuration, it can be transitioned to vi-movement-mode
  /// in which key bindings behave differently (other similar modes
  /// exist). When reseting the line using this method the input mode is
  /// unaffected. If you truly need to manually force libreadline into
  /// input mode, a new `Readline` will help:
  /// ```rust
  /// # use rline::Readline;
  /// # let mut current = Readline::new();
  /// current = Readline::new();
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if the cursor is not less than or equal to the number of
  /// characters in the given line.
  pub fn reset<'slf, S>(&'slf mut self, line: S, cursor: usize, clear_undo: bool)
  where
    S: AsRef<CStr>,
  {
    let s = line.as_ref();
    assert!(cursor <= s.to_bytes().len(), "invalid cursor position");

    let _guard = self.activate();
    unsafe {
      rl_replace_line(s.as_ptr(), clear_undo.into());
      rl_point = cursor as c_int;
    }
  }

  /// Inspect the current line state through a closure.
  // TODO: Really should not have to be a mutable method.
  pub fn inspect<F, R>(&mut self, inspector: F) -> R
  where
    F: FnOnce(&CStr, usize) -> R,
  {
    let _guard = self.activate();
    let (s, pos, len) = unsafe {
      debug_assert!(rl_end >= 0);
      debug_assert!(rl_point >= 0);

      let buf = rl_line_buffer as *const c_char;
      let len = rl_end as usize;
      let pos = rl_point as usize;

      (CStr::from_ptr(buf), pos, len)
    };

    debug_assert_eq!(s.to_bytes().len(), len);
    inspector(s, pos)
  }
}

impl Default for Readline {
  fn default() -> Self {
    Self::new()
  }
}

impl Drop for Readline {
  fn drop(&mut self) {
    let _guard = self.activate();

    // Make sure to release the memory we allocated.
    unsafe {
      free(rl_executing_keyseq as *mut c_void);
      free(rl_line_buffer as *mut c_void);
    }
  }
}


// Note that libreadline is pretty much fully configurable. With
// specific configurations it is possible that some tests fail (although
// we mostly use functionality that is pretty basis and unlikely to have
// been reconfigured by the user). While it would be possible to
// override the user configuration for the purpose of testing, that is
// not trivial and out of the scope of this crate.
#[cfg(test)]
mod tests {
  use super::*;

  use std::mem::align_of;


  #[test]
  fn is_locked() {
    let mutex = Mutex::<u64>::new(42);
    assert!(!mutex.is_locked());
    {
      let _guard = mutex.lock().unwrap();
      assert!(mutex.is_locked());
    }
    assert!(!mutex.is_locked());
  }

  #[test]
  fn alignment() {
    assert_eq!(align_of::<readline_state>(), 8);
  }

  #[test]
  fn empty_input() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("").unwrap())
  }

  #[test]
  fn multiple_inputs() {
    let mut rl = Readline::new();

    "first"
      .chars()
      .for_each(|c| assert!(rl.feed(c as u8).is_none()));
    assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("first").unwrap());

    "second"
      .chars()
      .for_each(|c| assert!(rl.feed(c as u8).is_none()));
    assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("second").unwrap());
  }

  #[test]
  fn cursor() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed('a' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("a").unwrap(), 1));

    assert_eq!(rl.feed('b' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("ab").unwrap(), 2));

    assert_eq!(rl.feed('c' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("abc").unwrap(), 3));
  }

  #[test]
  fn reset() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed('x' as u8), None);
    assert_eq!(rl.feed('y' as u8), None);
    assert_eq!(rl.feed('z' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("xyz").unwrap(), 3));

    rl.reset(&CString::new("abc").unwrap(), 1, true);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("abc").unwrap(), 1));

    assert_eq!(rl.feed('x' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("axbc").unwrap(), 2));
    assert_eq!(rl.feed('\n' as u8).unwrap(), CString::new("axbc").unwrap());

    rl.reset(&CString::new("123").unwrap(), 3, true);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("123").unwrap(), 3));

    assert_eq!(rl.feed('y' as u8), None);
    assert_eq!(rl.inspect(|s, p| (s.to_owned(), p)), (CString::new("123y").unwrap(), 4));
  }

  #[test]
  #[should_panic(expected = "invalid cursor position")]
  fn reset_panic() {
    Readline::new().reset(&CString::new("abc").unwrap(), 4, true);
  }
}
