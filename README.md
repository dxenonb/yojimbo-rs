# A Rust Game Networking Library

This is a (work-in-progress) port of [yojimbo](https://github.com/networkprotocol/yojimbo) to Rust.

This implementation links directly to the [netcode](https://github.com/networkprotocol/netcode) and [reliable](https://github.com/networkprotocol/reliable) C libraries.

Please note, this project is not endorsed, vetted, or in any way associated with `yojimbo`'s authors.

## Current Status

MVP tasks:

 - [x] Impl client and server connection
 - [x] Impl unreliable channels
 - [x] Impl reliable channels
 - [x] Impl message serialization
 - [x] Add CI/CD
 - [x] Add Automated Tests
 - [ ] Update dependencies (netcode, reliable, and libsodium)
 - [ ] Expose message IDs (either via set/get on Network Message or receive_message_with_id)
 - [ ] Review unsafe code (**the client must be boxed**, to work around some UB, fix on the way)
 - [ ] Review error handling and use Option/Result (need to resolve some panics still)
 - [ ] Impl bit packer
 - [ ] Impl client Matcher service

This is more or less a 1-1 port of `yojimbo` to Rust, following the C++ API as close as possible, with some ommissions:

 - There is no support for blocks (open an issue if you need it)
 - There is no serialization framework included in this library (you're probably going to use serde or write your own serializer)
 - There is no bit packer (for now)
 - There is no API for specifying any allocators (yet)
 - The Matcher is not ported yet, so there is no included way to securely get a private key/connect token to your client out-of-the-box.

*In lieu of block support, serializing large messages may work. While not ideal, try sending the binary data in chunks over a reliable channel, and then copy the chunk from each message into your block buffer. This should work OK as a stop gap as long as you aren't sending blocks often, e.g. once at the start of a game.*

Additional tasks in the backlog:

 - fix unnecessary copying of `NetworkMessage` in reliable channels (remove `Clone` requirement for `NetworkMessage`)
 - fix https://github.com/networkprotocol/yojimbo/issues/170
 - use a port of reliable or rewrite it, which will remove most unsafe
 - make server/client generic over any networking backend, which removes the remaining unsafe
 - implement loopback support with netcode, and efficient, copy-free loopback

If you are looking for more information on how to use `netcode` and `reliable`, definitely read the [architecture](#architecture--usage) section below. After that, check out the client and server examples, both in this library and the original `yojimbo`. You can work backwards from there (both are very small libraries). Netcode's client and server examples are also very straightforward.

## Architecture & Usage

The user only needs to interface with config types and `Client` and `Server`, but here's a quick look at how things work, which is important to understand proper usage:

 - yojimbo provides a `Client` and `Server` which have a nice interface to send and receive messages (reliably or unreliably) on a set of "channels" you define.
 - the `Client` and `Server` use `netcode` as the network backend.
 - `netcode` takes care of establishing some semblence of a "connection" using its own protocol over UDP (e.g. handshake and keepalives), and comes with some security protections.
 - on send, the `Client` and `Server` hand your message to a channel on the relevant connection. The channel decides when to send/resend the message.
 - the `Connection` serializes all the available messages (from all the channels) into a single buffer, and notifies the caller (client or server).
 - when the `Connection` has a buffer ready, the caller (client or server) then sends the buffer to a `reliable_endpoint_t`, which computes acks for any previously received packets (and possibly fragments the buffer into multiple packets).
 - on recieve, this happens in reverse; `Connection` deserializes the buffer and hands each message to the relevant channels, where they sit until you call `receive_message` on the client or server.

There are two types of channels: `UnreliableUnordered` and `ReliableOrdered`. Unreliable never retransmits packets or holds back messages, making it great for things you need to send fast (like physics snapshots and position updates). `ReliableOrdered` buffers messages until all the preceding messages are available (and retransmits messages until they are acked), making it perfect for sending authoritative RPC messages, among anything else that needs to definitely happen and happen in order.

Yojimbo is single threaded, and expects you to be calling `advance_time`, `send_packets` and `receive_packets` continously. You can throttle sending by calling `send_packets` less frequently (e.g. only call it every 1/15, 1/30, or 1/60 seconds). `receive_packets` should be called about as often to prevent the message queues from overfilling (which will force a disconnect). `advance_time` needs to be called at least as often, and no less frequently than `ClientServerConfig::timeout` to make sure the connection stays alive.

> TODO: talk about fragmentation and reliable channels

How you choose to define channels is totally up to you:

 - You might have a channel for each player connected,
 - or have a channel for RPC and several unreliable channels with different message and size limits (to manage priority),
 - or have a channel for every entity/actor (*note that channel count is fixed at startup),
 - or have just two channels, and serialize the relevant entity/actor ID in your messages.

Finally, if you have one or more reliable channels, make sure any recievers are sending something back (it doesn't have to be the same channel), otherwise the reliable messages are never acked (this is generally not a problem unless you have some kind of fixed spectator). For all channel types, make sure you are handling messages so the recieve queues don't overflow.

## Building

CI runs on Windows and Linux. Mac support is not tested.

Requirements:

 - rustc 1.68.0 or above
 - clang (required by `bindgen`, used to generate the Rust bindings to `reliable` and `netcode`)
 - libsodium (bundled for windows, must be avialable to pkg-config on linux)

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