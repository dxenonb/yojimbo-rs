use std::ffi::{c_void, CStr, CString};
use std::mem::size_of;

use crate::channel::ChannelCounters;
use crate::config::ClientServerConfig;
use crate::connection::{Connection, ConnectionErrorLevel};
use crate::message::NetworkMessage;
use crate::network_info::NetworkInfo;
use crate::{bindings::*, gf_init_default, PRIVATE_KEY_BYTES};

pub struct Server<M> {
    /// Base client/server config.
    config: ClientServerConfig,
    /// Maximum number of clients supported.
    max_clients: usize,
    /// True if server is currently running, eg. after "Start" is called, before "Stop".
    running: bool,
    /// Current server time in seconds.
    time: f64,
    /// Array of per-client connection classes. This is how messages are exchanged with clients.
    client_connection: Vec<Connection<M>>,
    /// Array of per-client reliable.io endpoints.
    client_endpoint: Vec<*mut reliable_endpoint_t>,
    /// The network simulator used to simulate packet loss, latency, jitter etc. Optional.
    network_simulator: Option<()>,
    /// Buffer used when writing packets.
    packet_buffer: Vec<u8>,

    address: String,
    bound_port: Option<u16>,
    server: *mut netcode_server_t,
    private_key: [u8; PRIVATE_KEY_BYTES],
}

impl<M: NetworkMessage> Server<M> {
    pub fn new(
        private_key: &[u8; PRIVATE_KEY_BYTES],
        address: String,
        config: ClientServerConfig,
        time: f64,
    ) -> Server<M> {
        assert_eq!(private_key.len(), PRIVATE_KEY_BYTES);

        assert_ne!(
            size_of::<M>(),
            0,
            "Zero sized message types are not supported"
        );

        Server {
            config,
            max_clients: 0,
            running: false,
            time,
            client_endpoint: Vec::new(),
            client_connection: Vec::new(),
            network_simulator: None,
            packet_buffer: Vec::new(),

            server: std::ptr::null_mut(),
            address,
            bound_port: None,
            private_key: *private_key,
        }
    }

    pub fn start(&mut self, max_clients: usize) {
        {
            /* yojimbo BaseServer::Start { */
            if self.running() {
                self.stop();
            }

            self.running = true;
            self.max_clients = max_clients;
            if self.config.network_simulator {
                unimplemented!("initialize network simulator");
            }

            assert!(self.client_connection.is_empty());
            assert!(self.client_endpoint.is_empty());
            for i in 0..max_clients {
                self.client_connection
                    .push(Connection::new(self.config.connection.clone(), self.time));

                let mut reliable_config =
                    gf_init_default!(reliable_config_t, reliable_default_config);

                let name = CStr::from_bytes_with_nul(b"server endpoint\0").unwrap();
                reliable_config.name[..name.to_bytes_with_nul().len()].copy_from_slice(unsafe {
                    &*(name.to_bytes_with_nul() as *const [u8] as *const [i8])
                });
                reliable_config.context = self as *mut _ as *mut _;
                reliable_config.index = i as _;
                reliable_config.max_packet_size = self.config.connection.max_packet_size as _;
                reliable_config.fragment_above = self.config.fragment_packets_above as _;
                reliable_config.max_fragments = self.config.max_packet_fragments as _;
                reliable_config.fragment_size = self.config.packet_fragment_size as _;
                reliable_config.ack_buffer_size = self.config.acked_packets_buffer_size as _;
                reliable_config.received_packets_buffer_size =
                    self.config.received_packets_buffer_size as _;
                reliable_config.fragment_reassembly_buffer_size =
                    self.config.packet_reassembly_buffer_size as _;
                reliable_config.rtt_smoothing_factor = self.config.rtt_smoothing_factor;
                reliable_config.transmit_packet_function = Some(transmit_packet::<M>);
                reliable_config.process_packet_function = Some(process_packet::<M>);

                // do not override `reliable`'s default allocator
                // reliable_config.allocator_context = std::ptr::null_mut();
                // reliable_config.allocate_function = None;
                // reliable_config.free_function = None;

                unsafe {
                    let endpoint = reliable_endpoint_create(&mut reliable_config, self.time);
                    self.client_endpoint.push(endpoint);
                    reliable_endpoint_reset(*self.client_endpoint.last().unwrap());
                }
            }
            self.packet_buffer = vec![0u8; self.config.connection.max_packet_size];

            let mut netcode_config =
                gf_init_default!(netcode_server_config_t, netcode_default_server_config);
            netcode_config.protocol_id = self.config.protocol_id;
            netcode_config
                .private_key
                .copy_from_slice(&self.private_key);

            // do not override `netcode`'s default allocator
            // netcode_config.allocator_context = std::ptr::null_mut();
            // netcode_config.allocate_function = None;
            // netcode_config.free_function = None;

            netcode_config.callback_context = self as *mut _ as *mut c_void;
            netcode_config.connect_disconnect_callback = Some(connect_disconnect_callback::<M>);
            netcode_config.send_loopback_packet_callback = None; // TODO

            let server_address = CString::new(self.address.clone()).unwrap();

            self.server = unsafe {
                netcode_server_create(
                    server_address.as_ptr() as *mut _,
                    &mut netcode_config,
                    self.time,
                )
            };

            if self.server.is_null() {
                self.stop();
                // TODO: emit some kind of error?
                return;
            }

            unsafe { netcode_server_start(self.server, max_clients.try_into().unwrap()) };

            self.bound_port = Some(unsafe { netcode_server_get_port(self.server) });
        }
    }

    pub fn stop(&mut self) {
        if !self.server.is_null() {
            self.bound_port = None;
            unsafe {
                netcode_server_stop(self.server);
                netcode_server_destroy(self.server);
            }
            self.server = std::ptr::null_mut();
        }

        {
            /* yojimbo BaseServer::Stop */
            if self.running {
                for endpoint in &mut self.client_endpoint {
                    unsafe { reliable_endpoint_destroy(*endpoint) };
                    *endpoint = std::ptr::null_mut();
                }
                self.client_endpoint.clear();
                self.client_connection.clear();
            }

            self.running = false;
            self.max_clients = 0;
            self.packet_buffer = Vec::new();
        }
    }

    pub fn send_packets(&mut self) {
        if self.server.is_null() {
            return;
        }
        for (client_index, (endpoint, conn)) in self
            .client_endpoint
            .iter_mut()
            .zip(self.client_connection.iter_mut())
            .enumerate()
        {
            if !is_client_connected(self.server, client_index) {
                continue;
            }
            let packet_sequence = unsafe { reliable_endpoint_next_packet_sequence(*endpoint) };
            let packet_data_size = self.packet_buffer.len();
            assert_eq!(packet_data_size, self.config.connection.max_packet_size);
            let written_bytes = conn.generate_packet(packet_sequence, &mut self.packet_buffer[..]);

            if written_bytes > 0 {
                let written_slice = &mut self.packet_buffer[..written_bytes];
                unsafe {
                    reliable_endpoint_send_packet(
                        *endpoint,
                        written_slice.as_mut_ptr(),
                        written_slice.len().try_into().unwrap(),
                    );
                }
            }
        }
    }

    pub fn receive_packets(&mut self) {
        if self.server.is_null() {
            return;
        }
        for (client_index, endpoint) in &mut self.client_endpoint.iter().enumerate() {
            loop {
                unsafe {
                    let mut packet_bytes: i32 = 0;
                    let mut packet_sequence: u64 = 0;
                    let packet_data = netcode_server_receive_packet(
                        self.server,
                        client_index as _,
                        &mut packet_bytes,
                        &mut packet_sequence,
                    );
                    if packet_data.is_null() {
                        break;
                    }
                    reliable_endpoint_receive_packet(*endpoint, packet_data, packet_bytes);
                    netcode_server_free_packet(self.server, packet_data as *mut _);
                }
            }
        }
    }

    pub fn send_message(&mut self, client_index: usize, channel_index: usize, message: M) {
        self.client_connection[client_index].send_message(channel_index, message);
    }

    pub fn receive_message(&mut self, client_index: usize, channel_index: usize) -> Option<M> {
        self.client_connection[client_index].receive_message(channel_index)
    }

    pub fn advance_time(&mut self, new_time: f64) {
        if !self.server.is_null() {
            unsafe { netcode_server_update(self.server, self.time) };
        }

        {
            /* yojimbo BaseServer::AdvanceTime */
            self.time = new_time;
            if !self.running() {
                return;
            }
            assert_eq!(self.client_connection.len(), self.client_endpoint.len());
            for (i, (conn, endpoint)) in self
                .client_connection
                .iter_mut()
                .zip(self.client_endpoint.iter_mut())
                .enumerate()
            {
                conn.advance_time(self.time);
                if conn.error_level() != ConnectionErrorLevel::None {
                    log::error!(
                        "client {} connection is in error state. disconnecting client",
                        i
                    );
                    unsafe { disconnect_client(self.server, i, *endpoint, conn) };
                    continue;
                }
                unsafe {
                    reliable_endpoint_update(*endpoint, self.time);
                    let mut num_acks = 0;
                    let acks = reliable_endpoint_get_acks(*endpoint, &mut num_acks);
                    conn.process_acks(acks, num_acks);
                    reliable_endpoint_clear_acks(*endpoint);
                }
                if let Some(_) = &self.network_simulator {
                    unimplemented!("advance network simulator time");
                }
            }
        }

        if let Some(_) = &self.network_simulator {
            unimplemented!("push packets through the network simulator");
        }
    }

    pub fn client_id(&self, client_index: usize) -> Option<u64> {
        if !self.is_client_connected(client_index) {
            return None;
        }
        Some(unsafe { netcode_server_client_id(self.server, client_index as _) })
    }

    pub fn is_client_connected(&self, client_index: usize) -> bool {
        is_client_connected(self.server, client_index)
    }

    pub fn disconnect_client(&mut self, client_index: usize) {
        let endpoint = self.client_endpoint[client_index];
        let connection = &mut self.client_connection[client_index];
        unsafe {
            disconnect_client(self.server, client_index, endpoint, connection);
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn bound_port(&self) -> Option<u16> {
        self.bound_port
    }

    pub fn can_send_message(&self, client_index: usize, channel_index: usize) -> bool {
        self.client_connection[client_index].can_send_message(channel_index)
    }

    pub fn has_messages_to_send(&self, client_index: usize, channel_index: usize) -> bool {
        self.client_connection[client_index].has_messages_to_send(channel_index)
    }

    /// Take a snapshot of the current network state.
    ///
    /// Returns None if the client is not connected.
    pub fn snapshot_network_info(&self, client_index: usize) -> Option<NetworkInfo> {
        assert!(self.running);
        assert!(client_index < self.client_connection.len());

        if !self.is_client_connected(client_index) {
            return None;
        }

        let endpoint = self.client_endpoint[client_index];
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

    /// Get the counters for client `client_index` and channel `channel_index`.
    pub fn channel_counters(&self, client_index: usize, channel_index: usize) -> &ChannelCounters {
        let connection = &self.client_connection[client_index];
        connection.channel_counters(channel_index)
    }

    pub fn connected_client_count(&self) -> usize {
        if self.server.is_null() {
            return 0;
        }
        if self.client_connection.len() == 0 {
            return 0;
        }
        let count = unsafe { netcode_server_num_connected_clients(self.server) };
        assert!(count >= 0);
        count as usize
    }

    pub fn client_address(&self, client_index: usize) -> Option<NetcodeAddress> {
        if !self.is_client_connected(client_index) {
            return None;
        }
        Some(unsafe {
            let address = netcode_server_client_address(self.server, client_index as _);
            if address.is_null() {
                return None;
            }
            NetcodeAddress::new(address)
        })
    }

    // TODO: loopback

    fn transmit_packet(
        &mut self,
        client_index: i32,
        _packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) {
        if let Some(_) = self.network_simulator {
            unimplemented!();
        }
        unsafe {
            netcode_server_send_packet(self.server, client_index, packet_data, packet_bytes);
        }
    }

    fn process_packet(
        &mut self,
        client_index: i32,
        packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) -> i32 {
        let connection = self.client_connection_mut(client_index);
        let result =
            unsafe { connection.process_packet(packet_sequence, packet_data, packet_bytes as _) };
        if result {
            1
        } else {
            0
        }
    }

    fn handle_connect_disconnect(&mut self, client_index: i32, connected: bool) {
        if connected {
            log::debug!("client connected: {}", client_index);
        } else {
            log::debug!("client disconnected: {}", client_index);
            unsafe {
                reliable_endpoint_reset(self.client_endpoint[client_index as usize]);
            }
            self.client_connection[client_index as usize].reset();
            if let Some(_) = &self.network_simulator {
                unimplemented!("discard client packets");
            }
        }
    }

    fn client_connection_mut(&mut self, client_index: i32) -> &mut Connection<M> {
        assert!(self.running());
        let client_index = client_index as usize;
        assert!(client_index < self.max_clients);
        assert!(client_index < self.client_connection.len());

        &mut self.client_connection[client_index]
    }
}

unsafe fn disconnect_client<M>(
    server: *mut netcode_server_t,
    client_index: usize,
    endpoint: *mut reliable_endpoint_t,
    _connection: &mut Connection<M>,
) {
    // TODO: on disconnect, clear send queue https://github.com/networkprotocol/yojimbo/issues/129
    assert!(!server.is_null());
    assert!(!endpoint.is_null());
    netcode_server_disconnect_client(server, client_index as _);
}

unsafe extern "C" fn transmit_packet<M: NetworkMessage>(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) {
    let server = context as *mut Server<M>;
    server
        .as_mut()
        .unwrap()
        .transmit_packet(index, packet_sequence, packet_data, packet_bytes);
}

unsafe extern "C" fn process_packet<M: NetworkMessage>(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) -> i32 {
    let server = context as *mut Server<M>;
    server
        .as_mut()
        .unwrap()
        .process_packet(index, packet_sequence, packet_data, packet_bytes)
}

fn is_client_connected(server: *mut netcode_server_t, client_index: usize) -> bool {
    unsafe { netcode_server_client_connected(server, client_index as _) != 0 }
}

unsafe extern "C" fn connect_disconnect_callback<M: NetworkMessage>(
    context: *mut c_void,
    client_index: i32,
    connected: i32,
) {
    let server = context as *mut Server<M>;
    server
        .as_mut()
        .unwrap()
        .handle_connect_disconnect(client_index, connected == 1);
}
