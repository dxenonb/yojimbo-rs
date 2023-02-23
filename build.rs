extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    let profile_is_release = env::var("PROFILE").unwrap() == "RELEASE";
    let reliable_profile = if profile_is_release {
        "RELIABLE_RELEASE"
    } else {
        "RELIABLE_DEBUG"
    };
    let netcode_profile = if profile_is_release {
        "NETCODE_RELEASE"
    } else {
        "NETCODE_DEBUG"
    };

    // build reliable
    cc::Build::new()
        .include("lib/reliable")
        .files(&["lib/reliable/reliable.c"])
        .define(reliable_profile, None)
        .compile("reliable");

    // build netcode
    cc::Build::new()
        .include("lib/netcode")
        .include("lib/windows")
        .files(&["lib/netcode/netcode.c"])
        .define(netcode_profile, None)
        .compile("netcode");

    // let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    // println!("cargo:rustc-link-search=native={:?}", out_path);

    let mut sodium_path = env::current_dir().unwrap();
    sodium_path.push("lib/windows");
    println!("cargo:rustc-link-search=native={}", sodium_path.display());
    println!("cargo:rustc-link-lib=static=sodium");

    bindings();
}

fn bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=lib/reliable.h");
    println!("cargo:rerun-if-changed=lib/netcode.h");

    let bindings = bindgen::Builder::default()
        .header("lib/reliable.h")
        .header("lib/netcode.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
