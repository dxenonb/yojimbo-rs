use std::ffi::{c_void, CString};
use std::usize;

use crate::channel::ChannelCounters;
use crate::config::{ClientServerConfig, NETCODE_KEY_BYTES};
use crate::connection::{Connection, ConnectionErrorLevel};
use crate::message::NetworkMessage;
use crate::network_info::NetworkInfo;
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
    fn try_from_netcode_client_state_i32(val: i32) -> Option<ClientState> {
        // REFACTOR: I think we can derive this with FromPrimitive in num-traits
        let mapped = match val {
            3 => ClientState::Connected,
            1 | 2 => ClientState::Connecting,
            0 => ClientState::Disconnected,
            x if x < 0 && x > -7 => ClientState::Error,
            _ => {
                return None;
            }
        };
        Some(mapped)
    }
}

pub struct Client<M> {
    config: ClientServerConfig,
    endpoint: *mut reliable_endpoint_t,
    connection: Option<Connection<M>>,
    network_simulator: Option<()>,
    packet_buffer: Vec<u8>,
    client_state: ClientState,
    #[allow(unused)]
    client_index: usize,
    time: f64,

    client: *mut netcode_client_t,
    address: String,
    bound_port: Option<u16>,
    client_id: u64,
}

impl<M: NetworkMessage> Client<M> {
    pub fn new(address: String, config: ClientServerConfig, time: f64) -> Client<M> {
        let packet_buffer = vec![0u8; config.connection.max_packet_size];
        Client {
            config,
            endpoint: std::ptr::null_mut(),
            connection: None,
            network_simulator: None,
            packet_buffer,
            client_state: ClientState::Disconnected,
            client_index: usize::MAX,
            time,

            client: std::ptr::null_mut(),
            address,
            bound_port: None,
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
        if !self.is_connected() {
            return;
        }
        assert!(!self.client.is_null());
        let packet_sequence = unsafe { reliable_endpoint_next_packet_sequence(self.endpoint) };
        if let Some(connection) = &mut self.connection {
            let written_bytes =
                connection.generate_packet(packet_sequence, &mut self.packet_buffer[..]);
            if written_bytes > 0 {
                unsafe {
                    let written_slice = &mut self.packet_buffer[..written_bytes];
                    reliable_endpoint_send_packet(
                        self.endpoint,
                        written_slice.as_mut_ptr(),
                        written_slice.len() as _,
                    );
                }
            }
        }
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

    pub fn send_message(&mut self, channel_index: usize, message: M) {
        self.connection
            .as_mut()
            .unwrap()
            .send_message(channel_index, message);
    }

    pub fn receive_message(&mut self, channel_index: usize) -> Option<M> {
        self.connection
            .as_mut()
            .unwrap()
            .receive_message(channel_index)
    }

    /// Check if this client is currently successfully connected.
    ///
    /// This means the client has finished the handshake and is
    /// able to send and receive messages.
    pub fn is_connected(&self) -> bool {
        // explicit match for exhaustiveness checking
        match self.client_state {
            ClientState::Connected => true,
            ClientState::Connecting => false,
            ClientState::Disconnected => false,
            ClientState::Error => false,
        }
    }

    /// Check if this client is currently disconnected and not connecting.
    ///
    /// This means the client has errored-out, lost connection, or
    /// disconnected and is not in the process of handshaking or sending
    /// and receiving messages.
    pub fn is_disconnected(&self) -> bool {
        // explicit match for exhaustiveness checking
        match self.client_state {
            ClientState::Connected => false,
            ClientState::Connecting => false,
            ClientState::Disconnected => true,
            ClientState::Error => true,
        }
    }

    pub fn can_send_message(&self, channel: usize) -> bool {
        self.connection
            .as_ref()
            .map(|c| c.can_send_message(channel))
            .unwrap_or(false)
    }

    pub fn has_messages_to_send(&self, channel: usize) -> bool {
        self.connection
            .as_ref()
            .map(|c| c.has_messages_to_send(channel))
            .unwrap_or(false)
    }

    /// Take a snapshot of the current network state.
    ///
    /// Returns None if the client is not connected.
    pub fn snapshot_network_info(&self) -> Option<NetworkInfo> {
        if !self.is_connected() {
            return None;
        }

        let endpoint = self.endpoint;
        assert!(!endpoint.is_null());

        unsafe {
            let mut sent_bandwidth = 0.0;
            let mut received_bandwidth = 0.0;
            let mut acked_bandwidth = 0.0;
            reliable_endpoint_bandwidth(
                endpoint,
                &mut sent_bandwidth,
                &mut received_bandwidth,
                &mut acked_bandwidth,
            );

            let counters = reliable_endpoint_counters(endpoint);
            let num_packets_sent =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_SENT as _);
            let num_packets_received =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_RECEIVED as _);
            let num_packets_acked =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_ACKED as _);

            Some(NetworkInfo {
                rtt: reliable_endpoint_rtt(endpoint),
                packet_loss: reliable_endpoint_packet_loss(endpoint),
                sent_bandwidth,
                received_bandwidth,
                acked_bandwidth,
                num_packets_sent,
                num_packets_received,
                num_packets_acked,
            })
        }
    }

    /// Get the counters for channel `channel_index`.
    pub fn channel_counters(&self, channel_index: usize) -> Option<&ChannelCounters> {
        Some(self.connection.as_ref()?.channel_counters(channel_index))
    }

    // TODO: client_index()

    pub fn connection_failed(&self) -> bool {
        matches!(self.client_state, ClientState::Error)
    }

    pub fn bound_port(&self) -> Option<u16> {
        self.bound_port
    }

    // TODO: loopback

    /// Called regardless of connection security
    fn connect_internal(&mut self) {
        let connection = Connection::new(self.config.connection.clone(), self.time);
        self.connection = Some(connection);
        if let Some(_) = self.network_simulator {
            unimplemented!();
        }

        let mut reliable_config = self.config.new_reliable_config(
            self as *const _ as *mut _,
            "client endpoint",
            None,
            transmit_packet::<M>,
            process_packet::<M>,
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
        netcode_config.callback_context = self as *mut _ as *mut c_void;
        netcode_config.state_change_callback = Some(state_change_callback::<M>);
        netcode_config.send_loopback_packet_callback = None; // TODO
        let address = CString::new(self.address.as_str()).unwrap();
        self.client = unsafe {
            netcode_client_create(address.as_ptr() as *mut i8, &mut netcode_config, self.time)
        };

        if !self.client.is_null() {
            self.bound_port = Some(unsafe { netcode_client_get_port(self.client) });
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
        // we could consider removing this callback entirely since it's just wasted performance
        log::debug!(
            "client state changed from: {:?} to {:?}",
            &previous,
            &current
        );
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

    unsafe fn process_packet(
        &mut self,
        packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) -> i32 {
        let result = self
            .connection
            .as_mut()
            .expect("client not connected")
            .process_packet(packet_sequence, packet_data, packet_bytes as _);
        if result {
            1
        } else {
            0
        }
    }

    pub fn disconnect(&mut self) {
        {
            /* yojimbo BaseClient::Disconnect */
            self.client_state = ClientState::Disconnected;
        }
        self.destroy_client();
        self.destroy_internal();
        self.client_id = 0;
    }

    fn destroy_internal(&mut self) {
        if !self.endpoint.is_null() {
            unsafe { reliable_endpoint_destroy(self.endpoint) };
            self.endpoint = std::ptr::null_mut();
        }
        self.network_simulator = None;
        self.connection = None;
    }
}

fn generate_insecure_connect_token(
    config: &ClientServerConfig,
    private_key: &[u8; PRIVATE_KEY_BYTES],
    client_id: u64,
    server_addresses: &[&str],
) -> Option<[u8; NETCODE_CONNECT_TOKEN_BYTES as _]> {
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
    } else if state == NETCODE_CLIENT_STATE_SENDING_CONNECTION_REQUEST as i32
        || state == NETCODE_CLIENT_STATE_SENDING_CONNECTION_RESPONSE as i32
    {
        ClientState::Connecting
    } else {
        ClientState::Connected
    }
}

unsafe extern "C" fn transmit_packet<M: NetworkMessage>(
    context: *mut c_void,
    _index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) {
    let client = context as *mut Client<M>;
    client
        .as_mut()
        .unwrap()
        .transmit_packet(packet_sequence, packet_data, packet_bytes);
}

unsafe extern "C" fn process_packet<M: NetworkMessage>(
    context: *mut c_void,
    _index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) -> i32 {
    let client = context as *mut Client<M>;
    client
        .as_mut()
        .unwrap()
        .process_packet(packet_sequence, packet_data, packet_bytes)
}

extern "C" fn state_change_callback<M: NetworkMessage>(
    context: *mut c_void,
    previous: i32,
    current: i32,
) {
    let client = context as *mut Client<M>;
    let previous = ClientState::try_from_netcode_client_state_i32(previous).unwrap();
    let current = ClientState::try_from_netcode_client_state_i32(current).unwrap();
    unsafe {
        client
            .as_mut()
            .unwrap()
            .state_change_callback(previous, current);
    }
}
