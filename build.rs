// Copyright (C) 2018-2023 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::env::var;


fn main() {
  println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");

  match var("CARGO_CFG_TARGET_OS").unwrap().as_ref() {
    "linux" => println!("cargo:rustc-link-lib=readline"),
    os => panic!("unknown target OS {}", os),
  }
}
