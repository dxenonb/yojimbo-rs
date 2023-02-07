use std::error::Error;

pub mod bindings;
pub mod channel;
pub mod client;
pub mod config;
pub mod connection;
pub mod server;

pub const PRIVATE_KEY_BYTES: usize = config::NETCODE_KEY_BYTES;

pub fn initialize() -> Result<(), Box<dyn Error>> {
    unsafe {
        if bindings::netcode_init() != bindings::NETCODE_OK as _ {
            return Err("failed to initialize netcode".into());
        }
        if bindings::reliable_init() != bindings::RELIABLE_OK as _ {
            return Err("failed to initialize reliable".into());
        }
        // TODO:
        // if bindings::sodium_init() != -1 { }
    }
    Ok(())
}

#[derive(Debug, Copy, Clone)]
#[repr(i32)]
pub enum LogLevel {
    None = 0,
    Error = 1,
    Info = 2,
    Debug = 3,
}

pub fn log_level(level: LogLevel) {
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

/**
 * Get a high precision time in seconds since the application has started.
 *
 * Please store time in f64 so you retain sufficient precision as time increases.
 */
pub fn time() -> f64 {
    // TODO
    0.0
}

// TODO: sequence buffer
// TODO: bit writer, bit reader

pub trait BaseStream {}
pub struct WriteStream {}
pub struct ReadStream {}

// TODO: message factory macros

// NEXT: ReliableOrderedChannel
