extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    // build reliable
    cc::Build::new()
        .include("lib/reliable")
        .files(&["lib/reliable/reliable.c"])
        .compile("reliable");

    // Tell cargo to look for shared libraries in the specified directory
    println!("cargo:rustc-link-search=lib/reliable");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=lib/reliable.h");

    let bindings = bindgen::Builder::default()
        .header("lib/reliable.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("reliable_bindings.rs"))
        .expect("Couldn't write bindings!");
}
