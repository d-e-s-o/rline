// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::env::var;
use std::env::var_os;
use std::path::Path;


fn main() {
  println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");

  let link_static = var_os("READLINE_STATIC").is_some() || cfg!(feature = "static");

  match var("CARGO_CFG_TARGET_OS").unwrap().as_ref() {
    "linux" => {
      if let Some(lib_dir) = var_os("READLINE_LIB_DIR") {
        let lib_dir = Path::new(&lib_dir);
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
      }
      // For the convenience of the user, we always include some
      // sensible (?) default search directories.
      match var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap().as_ref() {
        "32" => println!("cargo:rustc-link-search=native=/usr/lib/"),
        "64" => println!("cargo:rustc-link-search=native=/usr/lib64/"),
        _ => (),
      }

      println!(
        "cargo:rustc-link-lib={}readline",
        if link_static { "static=" } else { "" }
      );

      if link_static {
        // When linking statically we need to link with the transitive
        // `tinfo` library as well.
        if let Some(lib_dir) = var_os("TINFO_LIB_DIR") {
          let lib_dir = Path::new(&lib_dir);
          println!("cargo:rustc-link-search=native={}", lib_dir.display());
        }
        println!("cargo:rustc-link-lib=static=tinfo");
      }
    },
    os => panic!("unsupported target OS {os}"),
  }
}
