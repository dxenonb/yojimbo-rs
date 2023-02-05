// pub mod channel;
pub mod bindings;
pub mod config;
pub mod server;

pub fn initialize() {
    // TODO
}

pub fn shutdown() {
    // TODO
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
