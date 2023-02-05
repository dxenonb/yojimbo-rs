use std::ffi::{c_void, CString};
use std::usize;

use crate::config::{ClientServerConfig, NETCODE_KEY_BYTES};
use crate::connection::Connection;
use crate::{bindings::*, gf_init_default, PRIVATE_KEY_BYTES};

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum ClientState {
    Error = -1,
    Disconnected = 0,
    Connecting,
    Connected,
}

impl ClientState {
    fn try_from_i32(val: i32) -> Option<ClientState> {
        // TODO: I think we can derive this with FromPrimitive in num-traits
        let mapped = match val {
            -1 => ClientState::Error,
            0 => ClientState::Disconnected,
            1 => ClientState::Connecting,
            2 => ClientState::Connected,
            _ => {
                return None;
            }
        };
        assert_eq!(val, mapped as _);
        Some(mapped)
    }
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

    pub fn insecure_connect(
        &mut self,
        private_key: &[u8; NETCODE_KEY_BYTES],
        client_id: u64,
        server_addresses: &[&str],
    ) {
        assert!(server_addresses.len() > 0);
        assert!(server_addresses.len() <= NETCODE_MAX_SERVERS_PER_CONNECT as usize);

        self.disconnect();
        self.connect_internal();
        self.client_id = client_id;
        self.create_client();
        if self.client.is_null() {
            self.disconnect();
            return;
        }
        let mut connect_token = match generate_insecure_connect_token(
            &self.config,
            private_key,
            client_id,
            server_addresses,
        ) {
            Some(connect_token) => connect_token,
            None => {
                log::error!("failed to generate insecure connect token");
                self.client_state = ClientState::Error;
                return;
            }
        };
        unsafe { netcode_client_connect(self.client, connect_token.as_mut_ptr()) };
        self.client_state = ClientState::Connecting;
    }

    /// Called regardless of connection security
    fn connect_internal(&mut self) {
        let connection = Connection::new();
        self.connection = Some(connection);
        if let Some(_) = self.network_simulator {
            unimplemented!();
        }

        let mut reliable_config = self.config.new_reliable_config(
            "client endpoint",
            None,
            transmit_packet,
            process_packet,
        );
        unsafe {
            let endpoint = reliable_endpoint_create(&mut reliable_config, self.time);
            self.endpoint = endpoint;
            reliable_endpoint_reset(endpoint);
        }
    }

    /// Initialize the `client` field (with the `address` field).
    fn create_client(&mut self) {
        self.destroy_client();
        let mut netcode_config =
            gf_init_default!(netcode_client_config_t, netcode_default_client_config);
        // TODO: allocator support
        netcode_config.callback_context = self as *mut _ as *mut c_void;
        netcode_config.state_change_callback = Some(state_change_callback);
        netcode_config.send_loopback_packet_callback = None; // TODO
        let address = CString::new(self.address.as_str()).unwrap();
        self.client = unsafe {
            netcode_client_create(address.as_ptr() as *mut i8, &mut netcode_config, self.time)
        };

        if !self.client.is_null() {
            // TODO: get bound address
        }
    }

    fn destroy_client(&mut self) {
        // TODO
    }

    fn state_change_callback(&mut self, previous: ClientState, current: ClientState) {
        // TODO: do we need this (not rusty, meant for inheritance)? callers can poll
        // TODO: remove debug message
        println!("client state changed from: {:?} to {:?}", previous, current);
    }

    pub fn disconnect(&mut self) {
        // TODO
    }
}

extern "C" fn transmit_packet(
    _context: *mut c_void,
    _index: i32,
    _packet_sequence: u16,
    _packet_data: *mut u8,
    _packet_bytes: i32,
) {
}

extern "C" fn process_packet(
    _context: *mut c_void,
    _index: i32,
    _packet_sequence: u16,
    _packet_data: *mut u8,
    _packet_bytes: i32,
) -> i32 {
    0
}

fn generate_insecure_connect_token(
    config: &ClientServerConfig,
    private_key: &[u8; PRIVATE_KEY_BYTES],
    client_id: u64,
    server_addresses: &[&str],
) -> Option<[u8; NETCODE_CONNECT_TOKEN_BYTES as _]> {
    // TODO: validate that netcode doesn't read MaxAddressLength (that it stops at the null byte or earlier)
    let mut server_address_strings = Vec::new();
    let mut server_address_string_pointers = Vec::new();
    for addr in server_addresses {
        server_address_strings.push(CString::new(*addr).unwrap());
        let ptr = server_address_strings.last().unwrap().as_ptr();
        server_address_string_pointers.push(ptr);
    }

    let mut user_data = [0u8; 256];
    let mut connect_token = [0u8; NETCODE_CONNECT_TOKEN_BYTES as _];

    let ok = unsafe {
        netcode_generate_connect_token(
            server_addresses.len() as i32,
            server_address_string_pointers.as_ptr() as *mut *mut i8,
            server_address_string_pointers.as_ptr() as *mut *mut i8,
            config.timeout,
            config.timeout,
            client_id,
            config.protocol_id,
            private_key.as_ptr() as *mut u8,
            user_data.as_mut_ptr(),
            connect_token.as_mut_ptr(),
        ) == (NETCODE_OK as i32)
    };

    if !ok {
        return None;
    }

    Some(connect_token)
}

extern "C" fn state_change_callback(context: *mut c_void, previous: i32, current: i32) {
    let client = context as *mut Client;
    let previous = ClientState::try_from_i32(previous).unwrap();
    let current = ClientState::try_from_i32(current).unwrap();
    unsafe {
        client
            .as_mut()
            .unwrap()
            .state_change_callback(previous, current);
    }
}
