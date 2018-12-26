// basic.rs

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

//! An example illustrating how to use the `rline` crate's `Readline`
//! object.
//! The relevant logic resides inside the `process_input` function.

use std::io::Read;
use std::io::Result;
use std::io::stdin;
use std::io::stdout;
use std::io::Write;

use termion::clear;
use termion::cursor;
use termion::raw::IntoRawMode;

use rline::Readline;

/// ASCII end-of-text indicator.
const EOT: u8 = 0x04;


/// Read and process data from the given `Read` object.
///
/// The bool wrapped inside the result is an indication whether to quit
/// the application or not.
fn process_input<R, W>(mut r: R, mut w: W, rl: &mut Readline, line: &mut u16) -> Result<bool>
where
  R: Read,
  W: Write,
{
  let mut buffer = [0u8; 16];
  let n = r.read(&mut buffer)?;

  // We quit input processing when seeing an end-of-text indicator.
  if n >= 1 && buffer[0] == EOT {
    return Ok(true)
  }

  // Always clear the current line and reset the cursor to the beginning
  // before outputting something on our own.
  write!(w, "{}{}", clear::CurrentLine, cursor::Goto(1, *line))?;

  // Check whether our `Readline` object has completed a line given
  // the user-provided input. If so, check whether the user typed
  // "quit" and exit. If not just print it, move to the next line,
  // and continue accepting input.
  if let Some(text) = rl.feed(&buffer[..n]) {
    if text.as_bytes() == b"quit" {
      return Ok(true)
    }

    *line += 1;
    w.write_all(text.as_bytes())?;
    write!(w, "{}", cursor::Goto(1, *line))?
  } else {
    // Take a peek at the text libreadline has in its internal buffer
    // and take measures to display that on the screen, along with the
    // cursor.
    rl.peek(|text, cursor| {
      w.write_all(text.to_bytes())?;
      // Normalize the cursor position as per `termion`'s rules.
      write!(w, "{}", cursor::Goto(cursor as u16 + 1, *line))
    })?
  };

  w.flush()?;
  Ok(false)
}

fn main() -> Result<()> {
  // Transition terminal into raw mode to disable line buffering and
  // allow for one-byte-at-a-time reading.
  let mut w = stdout().into_raw_mode()?;

  write!(w, "{}{}", clear::All, cursor::Goto(1, 1))?;
  write!(w, "> Your system's readline configuration is in effect.\n\r")?;
  write!(w, "> Please enter some text. Use Ctrl-D (EOT) or type \"quit\" to exit.\n\r")?;
  write!(w, "\n\r")?;

  // We start displaying input at line four. Note that `termion` starts
  // counting at one, not zero.
  let mut line = 4;
  write!(w, "{}", cursor::Goto(1, line))?;
  w.flush()?;

  // We have a single readline instance that we use for all input
  // matters. This instance supports undo operations within a line and
  // history navigation over text entered in the past.
  let mut rl = Readline::new();

  loop {
    if process_input(stdin(), &mut w, &mut rl, &mut line)? {
      write!(w, "> Bye.\n\r")?;
      break Ok(())
    }
  }
}
