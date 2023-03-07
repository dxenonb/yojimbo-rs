extern crate bindgen;

use std::env;
use std::path::PathBuf;

struct Library {
    link_path: PathBuf,
    include_path: PathBuf,
}

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

    let sodium = libsodium();

    // build reliable
    cc::Build::new()
        .include("lib/reliable")
        .files(&["lib/reliable/reliable.c"])
        .define(reliable_profile, None)
        .compile("reliable");

    // build netcode
    cc::Build::new()
        .include("lib/netcode")
        .include(sodium.include_path.to_str().unwrap())
        .files(&["lib/netcode/netcode.c"])
        .define(netcode_profile, None)
        .compile("netcode");

    // let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    // println!("cargo:rustc-link-search=native={:?}", out_path);

    println!(
        "cargo:rustc-link-search=native={}",
        sodium.link_path.display()
    );
    println!("cargo:rustc-link-lib=static=sodium");

    bindings();
}

fn bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=lib/bindings.h");

    let bindings = bindgen::Builder::default()
        .header("lib/bindings.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

#[cfg(windows)]
fn libsodium() -> Library {
    let mut path = env::current_dir().unwrap();
    path.push("lib/windows");
    Library {
        link_path: path.clone(),
        include_path: path.clone(),
    }
}

#[cfg(unix)]
fn libsodium() -> Library {
    let libsodium = pkg_config::Config::new()
        .atleast_version("1.0.18")
        .probe("libsodium")
        .unwrap();
    Library {
        link_path: libsodium.link_paths[0].clone(),
        include_path: libsodium.include_paths[0].clone(),
    }
}
