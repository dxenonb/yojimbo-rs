use std::usize;

use crate::bindings::*;
use crate::config::ClientServerConfig;
use crate::connection::Connection;

pub enum ClientState {
    Error,
    Disconnected,
    Connecting,
    Connected,
}

pub struct Client {
    config: ClientServerConfig,
    endpoint: *mut reliable_endpoint_t,
    connection: Option<Connection>,
    network_simulator: Option<()>,
    client_state: ClientState,
    client_index: usize,
    time: f64,

    client: *mut netcode_client_t,
    address: String,
    // bound_address: TODO
    client_id: u64,
}

impl Client {
    pub fn new(address: String, config: ClientServerConfig, time: f64) -> Client {
        Client {
            config,
            endpoint: std::ptr::null_mut(),
            connection: None,
            network_simulator: None,
            client_state: ClientState::Disconnected,
            client_index: usize::MAX,
            time,

            client: std::ptr::null_mut(),
            address,
            client_id: 0,
        }
    }
}
