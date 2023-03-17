use std::error::Error;

pub mod bindings;
pub mod channel;
pub mod client;
pub mod config;
pub mod connection;
pub mod message;
pub mod network_info;
pub mod network_simulator;
pub mod server;

pub const PRIVATE_KEY_BYTES: usize = bindings::NETCODE_KEY_BYTES as usize;
pub const CONNECT_TOKEN_BYTES: usize = bindings::NETCODE_CONNECT_TOKEN_BYTES as usize;

#[derive(Debug, Copy, Clone)]
#[repr(i32)]
pub enum BindingsLogLevel {
    None = 0,
    Error = 1,
    Info = 2,
    Debug = 3,
}

/// Initialize the bindings.
///
/// TODO: Consider initializing as part of Server/Client initialization?
pub fn initialize() -> Result<(), Box<dyn Error>> {
    unsafe {
        if bindings::netcode_init() != bindings::NETCODE_OK as _ {
            return Err("failed to initialize netcode".into());
        }
        if bindings::reliable_init() != bindings::RELIABLE_OK as _ {
            return Err("failed to initialize reliable".into());
        }
        // Ideally: (netcode does this, low priority) bindings::sodium_init() (OK if != -1)
    }
    Ok(())
}

/// Sets the log level for the bindings.
///
/// If this is not called, the default is None.
pub fn set_bindings_log_level(level: BindingsLogLevel) {
    unsafe {
        bindings::netcode_log_level(level as _);
        bindings::reliable_log_level(level as _);
    }
}

pub fn shutdown() {
    unsafe {
        bindings::reliable_term();
        bindings::netcode_term();
    }
}
