# A Rust Game Networking Library

This is a (work-in-progress) port of [yojimbo](https://github.com/networkprotocol/yojimbo) to Rust. 

This implementation links directly to the [netcode](https://github.com/networkprotocol/netcode) and [reliable](https://github.com/networkprotocol/reliable) C libraries.

Please note, this project is not endorsed, vetted, or in any way associated with `yojimbo`'s authors.

## Current Status

MVP tasks:

 - [x] Impl client and server connection
 - [x] Impl unreliable channels
 - [ ] Impl reliable channels
 - [x] Impl message serialization
 - [ ] Add CI/CD
 - [ ] Add Automated Tests
 - [ ] Update dependencies (netcode, reliable, and libsodium)
 - [ ] Review unsafe code (some unsafe blocks are a bit cavalier, and unsafety may be exposed in safe APIs)
 - [ ] Review error handling and use Option/Result (need to resolve some panics still)
 - [ ] Impl client Matcher service

Currently this is more or less a 1-1 port of `yojimbo` to Rust, following the C++ API as close as possible, with some ommissions:

 - There is serialization framework included in this library (you're probably going to use serde)
 - There is no bit packer (for now)
 - There is no API for specifying any allocators (yet)
 - The Matcher is not ported yet, so there is no included way to securely get a private key/connect token to your client out-of-the-box.

`yojimbo`'s API is not an idiomatic Rust API, so over time the API may drift from `yojimbo`'s C++ API to one tailored for Rust.

Additional tasks in the backlog:

 - consider a more RAII oriented API, refactor for Rust
 - reconsider how channels are handled
 - review some other C-like things that feel odd in Rust
 - fix https://github.com/networkprotocol/yojimbo/issues/170
 - implement an example Matchmaking backend

If you are looking for more information on how to use `netcode` and `reliable`, reading this library's source should be a good place to start (the `yojimbo` source is also great). I recommend starting by reading the client and server examples, and working backwards from there. Netcode's client and server examples are also very straightforward.

## Building

Currently developed on Windows.

Requirements:

 - rustc 1.65.0 or above
 - clang (required by `bindgen`, used to generate the Rust bindings to `reliable` and `netcode`)

Please note: The build script is not well tested. The `cc` crate should select a suitable compiler based on your system. Please open an issue if you run into anything.

You can compile the examples using:

```sh
cargo build --example <example>

# start with:
cargo build --example server
# and in a second terminal:
cargo build --example client
```

If you want to build `netcode` and `reliable` separately, please view the build instructions in the respective repo.

Helpful hint: if you are on Windows using MSVC with Rust, you don't need a full Visual Studio install, you can use the VS command line tools' `msbuild` command after generating the MSVC project files with `premake5` (again, see the repos for details).

## License

This library is currently unlicensed, please open an issue if you would like to use it!

Parts of this library are directly modified from `yojimbo` and hence governed under the [yojimbo license](https://github.com/networkprotocol/yojimbo/blob/master/LICENCE).