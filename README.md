# A Rust Game Networking Library

This is a (work-in-progress) port of [yojimbo](https://github.com/networkprotocol/yojimbo) to Rust. Please note, this project is not endorsed, or in any way associated with `yojimbo`'s authors.

This implementation links directly to [netcode](https://github.com/networkprotocol/netcode) and [reliable](https://github.com/networkprotocol/reliable) libraries (in C).

## Current Status

Current status: untested, incomplete.

I'm developing this mainly to familiarize myself with `netcode` and `reliable`. Currently this is a more or less 1-1 port of `yojimbo` to Rust, following the C++ API as close as possible (but minimizing exposed unsafe).

`yojimbo`'s API is somewhat of a poor fit for a safe and idiomatic Rust API, so over time this may drift from being a port to being a more idiomatic Rust-specific library. (in that case, I will archive this and move it to a new repo)

If you are looking for more information on how to use `netcode` and `reliable`, reading this library's source should be a good place to start (the `yojimbo` source is also great!). I recommend starting by reading the client and server examples, and working backwards from there.

## Building

Currently tested on Windows.

 - rustc 1.65.0 or above
 - clang (required by `bindgen`, used to generate the Rust bindings to `reliable` and `netcode`)

Please note: The build script is not well tested. `cc` should select a suitable compiler based on your system. Please open an issue if you run into anything.

You can compile the examples using:

```sh
cargo build --example <example>

# start with:
cargo build --example server
# and in a second terminal:
cargo build --example client
```

If you want to build `netcode` and `reliable` separately, please view the build instructions in the respective repo. (if you are on Windows using MSVC with Rust, you don't need a full Visual Studio install, you can use the VS command line tools' `msbuild` command after generating the MSVC project files with `premake5`)

## License

This library is currently unlicensed, please open an issue if you would like to use it!

Parts of this library are directly modified from `yojimbo` and hence governed under the [yojimbo license](https://github.com/networkprotocol/yojimbo/blob/master/LICENCE).