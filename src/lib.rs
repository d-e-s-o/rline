// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

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

use std::cell::RefCell;
use std::cell::RefMut;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::fmt::Error;
use std::fmt::Formatter;
use std::mem::MaybeUninit;
use std::ptr::addr_of;
use std::ptr::addr_of_mut;
use std::ptr::null;
use std::ptr::null_mut;
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
  static mut rl_undo_list: *mut c_void;

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

  fn rl_free_undo_list();
}


/// A helper function for loading a `readline_state` object.
fn load_state(state: *mut readline_state) {
  let result = unsafe { rl_save_state(state) };
  assert_eq!(result, 0);
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
    load_state(self)
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
  #[allow(clippy::match_like_matches_macro)]
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
struct ReadlineGuard<'data> {
  _guard: MutexGuard<'data, Id>,
  state: RefMut<'data, Box<readline_state>>,
}

impl Drop for ReadlineGuard<'_> {
  fn drop(&mut self) {
    // Before unlocking (by virtue of dropping the embedded guard)
    // always make sure to read back the most recent version of the
    // state from the globals.
    self.state.load()
  }
}


/// A type representing a single key. A key is a sequence of bytes which
/// can be anything from a single byte representing an ASCII character
/// or a terminal escape sequence.
type Key = [u8];


/// A struct representing a context for reading a line using libreadline.
#[derive(Debug)]
pub struct Readline {
  id: Id,
  state: RefCell<Box<readline_state>>,
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

    // SAFETY: Our global mutex is locked (as per assertion above) and
    //         we only call the function once.
    let line_ref = unsafe { Self::line() };
    if line.is_null() {
      let _prev = line_ref.replace(CString::new("").unwrap());
    } else {
      unsafe {
        let _prev = line_ref.replace(CStr::from_ptr(line).into());
        free(line.cast());
      }
    }
  }

  /// Create a new `Readline` instance.
  ///
  /// # Panics
  ///
  /// Panics on failure to allocate internally used C objects.
  pub fn new() -> Self {
    let rl = Self {
      id: Id::new(),
      state: RefCell::new(Box::new(Self::initial().clone())),
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
        rl_line_buffer = calloc(1, rl_line_buffer_len as _).cast();
        rl_executing_keyseq = calloc(1, rl_key_sequence_length as _).cast();

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
  #[allow(static_mut_refs)]
  fn initial() -> &'static readline_state {
    // We effectively cache a version of `readline_state` as it was set
    // by libreadline before anything could have changed. This state
    // acts as the template for all the states we create later on.
    static mut STATE: MaybeUninit<readline_state> = MaybeUninit::uninit();
    static ONCE: Once = Once::new();

    // We should be safe *not* using our all-protecting mutex here
    // because this functionality is invoked only as the very first
    // interaction with libreadline, by virtue of being used only in
    // the constructor of objects of the one struct that has exclusive
    // access to libreadline's global state.
    ONCE.call_once(|| unsafe {
      // Disable a bunch of libreadline stuff that would mess up things
      // we don't want messed up, most prominently signal handler state
      // and terminal state.
      // This is all state that is part of `readline_state`, so we make
      // those changes once for the template and don't have to worry
      // about them again.
      rl_catch_signals = 0;
      rl_catch_sigwinch = 0;
      rl_input_available_hook = Self::input_available as *mut _;
      rl_redisplay_function = Self::display as *mut _;
      rl_prep_term_function = Self::initialize_term as *mut _;
      rl_deprep_term_function = Self::uninitialize_term as *mut _;

      // Note that we do not ever invoke rl_callback_handler_remove.
      // This crate's assumption is that it is the sole user of
      // libreadline meaning nobody else will mess with global state. As
      // such, and because we set the same handler for all contexts,
      // there is no point in doing additional work to remove it. In
      // addition, due to the retardedness of libreadline and it not
      // capturing even all of its own global state, we could not even
      // remove the handler if we wanted to, because activating a
      // `readline_state` object would not set the handler. Sigh.
      rl_callback_handler_install(null(), Self::handle_line as *mut _);

      // libreadline already has buffers allocated but we won't be using
      // them.
      free(rl_line_buffer.cast());
      free(rl_executing_keyseq.cast());

      rl_line_buffer = null_mut();
      rl_executing_keyseq = null_mut();
      rl_undo_list = null_mut();

      load_state(STATE.as_mut_ptr());
    });

    // `STATE` is guaranteed to be initialized after the above call to
    // `load_state`, so it should be safe to create a reference to the
    // data now.
    unsafe { &*STATE.as_ptr() }
  }

  /// Retrieve a reference to the `Mutex` protecting all accesses to
  /// libreadline's global state.
  fn mutex() -> &'static Mutex<Id> {
    static mut MUTEX: Option<Mutex<Id>> = None;
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe { MUTEX = Some(Mutex::new(Id::new())) });

    // SAFETY: We never ever hand out mutable references to `MUTEX` or
    //         use one beyond this point and so it's always safe to
    //         create a shared one.
    match unsafe { &*addr_of!(MUTEX) } {
      Some(mutex) => mutex,
      None => unreachable!(),
    }
  }

  /// A reference to the global line storage.
  ///
  /// # Safety
  /// Callers must ensure that the global mutex is held for the duration
  /// of the usage of the returned reference and are not allowed to call
  /// this function while another such reference is active.
  unsafe fn line() -> &'static mut Option<CString> {
    static mut LINE: Option<CString> = None;
    debug_assert!(Self::mutex().is_locked());

    // SAFETY: As per the function contract, callers need to hold the
    //         global mutex and may only keep around a single mutable
    //         reference being returned.
    unsafe { &mut *addr_of_mut!(LINE) }
  }

  /// Activate this context.
  fn activate(&self) -> ReadlineGuard<'_> {
    let mut guard = Self::mutex().lock().unwrap();
    let state = self.state.borrow_mut();

    // Activate our state if necessary.
    if *guard != self.id {
      state.save();
      *guard = self.id;
    }

    ReadlineGuard {
      _guard: guard,
      state,
    }
  }

  /// Feed a key to libreadline.
  ///
  /// The provided buffer should comprise not more than a single key,
  /// which may be a single byte only or an escape sequence.
  ///
  /// # Panics
  ///
  /// Panics if too many bytes are supplied. libreadline's internal
  /// buffer is said to hold 512 bytes, so any slice of equal or greater
  /// size may cause a panic.
  pub fn feed(&mut self, key: impl AsRef<Key>) -> Option<CString> {
    fn feed_impl(rl: &Readline, key: &Key) -> Option<CString> {
      if key.is_empty() {
        return None
      }

      let _guard = rl.activate();

      for &b in key {
        // This call will only fail if there is not enough space available
        // to push the given character (with libreadline specifying a
        // buffer size large enough for 512 characters). As we feed one
        // character at a time and process (i.e., consume) it immediately
        // afterwards, there is no risk of us ever hitting this limit.
        //
        // Note that despite `rl_stuff_char` accepting a `c_int`, it
        // actually casts that value down to a single byte internally,
        // which is why we provide a saner interface that directly just
        // accepts bytes.
        let result = unsafe { rl_stuff_char(c_int::from(b)) };
        // There is nothing we can do about this error. Heck, not even the
        // user can do anything about this problem *after* hitting it. We
        // cannot safely call `rl_callback_read_char` without risking
        // cutting off input in the middle of an escape sequence,
        // resulting in what effectively is corrupted input. We also
        // cannot revert the buffer back to its previous state because
        // there is no API to do that. Holy crap what a mess.
        assert_ne!(result, 0, "libreadline's input buffer overflowed");
      }

      unsafe { rl_callback_read_char(); }
      // SAFETY: `_guard` will outlive the returned reference and we
      //         only call the function once.
      let line_ref = unsafe { Readline::line() };
      line_ref.take()
    }

    feed_impl(self, key.as_ref())
  }

  /// Reset libreadline's line state to the given line with the given
  /// cursor position. If `clear_undo` is set, the undo list associated
  /// with the current line is cleared
  ///
  /// Note that this method does not deal with input related modes. For
  /// example, libreadline always starts in input mode, but, depending
  /// on user configuration, it can be transitioned to vi-movement-mode
  /// in which key bindings behave differently (other similar modes
  /// exist). When resetting the line using this method the input mode
  /// is unaffected. If you truly need to manually force libreadline
  /// into input mode, a new `Readline` will help:
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
  pub fn reset<S>(&mut self, line: S, cursor: usize, clear_undo: bool)
  where
    S: AsRef<CStr>,
  {
    fn reset_impl(rl: &Readline, s: &CStr, cursor: usize, clear_undo: bool) {
      assert!(cursor <= s.to_bytes().len(), "invalid cursor position");

      let _guard = rl.activate();
      unsafe {
        rl_replace_line(s.as_ptr(), clear_undo.into());
        rl_point = cursor as _;
      }
    }

    reset_impl(self, line.as_ref(), cursor, clear_undo)
  }

  /// Peek at the current line state through a closure.
  pub fn peek<F, R>(&self, peeker: F) -> R
  where
    F: FnOnce(&CStr, usize) -> R,
  {
    let _guard = self.activate();
    let (s, pos, len) = unsafe {
      debug_assert!(rl_end >= 0);
      debug_assert!(rl_point >= 0);

      let buf = rl_line_buffer;
      let len = rl_end as _;
      let pos = rl_point as _;

      (CStr::from_ptr(buf), pos, len)
    };

    debug_assert_eq!(s.to_bytes().len(), len);
    peeker(s, pos)
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

    // Make sure to release the memory we or libreadline allocated.
    unsafe {
      rl_free_undo_list();
      free(rl_executing_keyseq.cast());
      free(rl_line_buffer.cast());
    }
  }
}


// Note that libreadline is pretty much fully configurable. With
// specific configurations it is possible that some tests fail (although
// we mostly use functionality that is pretty basic and unlikely to have
// been reconfigured by the user). While it would be possible to
// override the user configuration for the purpose of testing, that is
// not trivial and out of the scope of this crate.
#[cfg(test)]
mod tests {
  use super::*;

  use std::mem::align_of;

  use test_fork::fork;


  /// Exercise the `Debug` representation of various types.
  #[test]
  fn debug_repr() {
    assert_ne!(format!("{:?}", T(())), "");

    let rl = Readline::default();
    assert_ne!(format!("{rl:?}"), "");
  }

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

    assert!(rl.feed(b"").is_none())
  }

  #[test]
  fn empty_line_input() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed(b"\n").unwrap(), CString::new("").unwrap())
  }

  #[test]
  fn multiple_inputs() {
    let mut rl = Readline::new();

    assert!(rl.feed(b"first").is_none());
    assert_eq!(rl.feed(b"\n").unwrap(), CString::new("first").unwrap());

    assert!(rl.feed(b"second").is_none());
    assert_eq!(rl.feed(b"\n").unwrap(), CString::new("second").unwrap());
  }

  #[test]
  fn cursor() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed(b"a"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("a").unwrap(), 1));

    assert_eq!(rl.feed(b"b"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("ab").unwrap(), 2));

    assert_eq!(rl.feed(b"c"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("abc").unwrap(), 3));
  }

  #[test]
  fn reset() {
    let mut rl = Readline::new();

    assert_eq!(rl.feed(b"xyz"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("xyz").unwrap(), 3));

    rl.reset(CString::new("abc").unwrap(), 1, true);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("abc").unwrap(), 1));

    assert_eq!(rl.feed(b"x"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("axbc").unwrap(), 2));
    assert_eq!(rl.feed(b"\n").unwrap(), CString::new("axbc").unwrap());

    rl.reset(CString::new("123").unwrap(), 3, true);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("123").unwrap(), 3));

    assert_eq!(rl.feed(b"y"), None);
    assert_eq!(rl.peek(|s, p| (s.to_owned(), p)), (CString::new("123y").unwrap(), 4));
  }

  /// Make sure that we can mix usage of different `Readline` instances.
  #[test]
  fn multi_instance() {
    let mut rl1 = Readline::new();
    assert_eq!(rl1.feed(b"abcdefg"), None);

    let mut rl2 = Readline::new();
    assert_eq!(rl2.feed(b"efghijl"), None);

    rl1.reset(CString::new("abc").unwrap(), 1, false);

    assert_eq!(rl1.feed(b"\n").unwrap(), CString::new("abc").unwrap());
    assert_eq!(rl2.feed(b"\n").unwrap(), CString::new("efghijl").unwrap());
  }

  #[test]
  #[should_panic(expected = "invalid cursor position")]
  fn reset_panic() {
    Readline::new().reset(CString::new("abc").unwrap(), 4, true);
  }

  #[fork]
  #[test]
  fn with_user_configuration() {
    extern "C" {
      fn rl_parse_and_bind(line: *mut c_char) -> c_int;
    }

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
}
