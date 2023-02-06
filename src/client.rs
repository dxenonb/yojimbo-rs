use std::ffi::{c_void, CString};
use std::usize;

use crate::config::{ClientServerConfig, NETCODE_KEY_BYTES};
use crate::connection::{Connection, ConnectionErrorLevel};
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
        // REFACTOR: I think we can derive this with FromPrimitive in num-traits
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

    pub fn advance_time(&mut self, new_time: f64) {
        self.time = new_time;

        {
            /* yojimbo BaseClient::AdvanceTime */
            if !self.endpoint.is_null() {
                if let Some(connection) = &mut self.connection {
                    connection.advance_time(self.time);
                    if connection.error_level() != ConnectionErrorLevel::None {
                        log::error!("connection error. disconnecting client");
                        self.disconnect();
                        return;
                    }
                    unsafe {
                        reliable_endpoint_update(self.endpoint, self.time);
                        let mut num_acks = 0;
                        let acks = reliable_endpoint_get_acks(self.endpoint, &mut num_acks);
                        connection.process_acks(acks, num_acks);
                        reliable_endpoint_clear_acks(self.endpoint);
                    }
                }
            }
            if let Some(_) = self.network_simulator {
                unimplemented!("advance network simulator time");
            }
        }

        if self.client.is_null() {
            return;
        }
        let state = unsafe {
            netcode_client_update(self.client, self.time);
            let state = netcode_client_state(self.client);
            client_state_from_netcode_state(state)
        };
        self.client_state = state;
        if matches!(state, ClientState::Disconnected | ClientState::Error) {
            self.disconnect();
        }
        if let Some(_) = self.network_simulator {
            unimplemented!("push packets through the network simulator");
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

    pub fn send_packets(&mut self) {
        // TODO
    }

    pub fn receive_packets(&mut self) {
        if !self.is_connected() {
            return;
        }
        assert!(!self.client.is_null());
        loop {
            unsafe {
                let mut packet_bytes: i32 = 0;
                let mut packet_sequence: u64 = 0;
                let packet_data = netcode_client_receive_packet(
                    self.client,
                    &mut packet_bytes,
                    &mut packet_sequence,
                );
                if packet_data.is_null() {
                    break;
                }
                reliable_endpoint_receive_packet(self.endpoint, packet_data, packet_bytes);
                netcode_client_free_packet(self.client, packet_data as *mut _);
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.client_state, ClientState::Connected)
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(
            self.client_state,
            ClientState::Error | ClientState::Disconnected
        )
    }

    pub fn connection_failed(&self) -> bool {
        matches!(self.client_state, ClientState::Error)
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
        if self.client.is_null() {
            return;
        }
        unsafe { netcode_client_destroy(self.client) };
        self.client = std::ptr::null_mut();
    }

    fn state_change_callback(&mut self, previous: ClientState, current: ClientState) {
        // TODO: do we need this (not rusty, meant for inheritance)? callers can poll
        // TODO: remove debug message
        println!("client state changed from: {:?} to {:?}", previous, current);
    }

    unsafe fn transmit_packet(
        &mut self,
        _packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) {
        if let Some(_) = &self.network_simulator {
            unimplemented!();
        } else {
            netcode_client_send_packet(self.client, packet_data, packet_bytes);
        }
    }

    pub fn disconnect(&mut self) {
        // TODO
    }
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

fn client_state_from_netcode_state(state: i32) -> ClientState {
    if state < NETCODE_CLIENT_STATE_DISCONNECTED as i32 {
        ClientState::Error
    } else if state == NETCODE_CLIENT_STATE_DISCONNECTED as i32 {
        ClientState::Disconnected
    } else if state == NETCODE_CLIENT_STATE_SENDING_CONNECTION_REQUEST as i32 {
        ClientState::Connecting
    } else {
        ClientState::Connected
    }
}

unsafe extern "C" fn transmit_packet(
    context: *mut c_void,
    _index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) {
    let client = context as *mut Client;
    client
        .as_mut()
        .unwrap()
        .transmit_packet(packet_sequence, packet_data, packet_bytes);
}

unsafe extern "C" fn process_packet(
    _context: *mut c_void,
    _index: i32,
    _packet_sequence: u16,
    _packet_data: *mut u8,
    _packet_bytes: i32,
) -> i32 {
    0
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
