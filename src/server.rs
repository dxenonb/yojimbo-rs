use std::ffi::{c_void, CStr, CString};

use crate::config::{ClientServerConfig, NETCODE_KEY_BYTES};
use crate::connection::{Connection, ConnectionErrorLevel};
use crate::{bindings::*, gf_init_default};

pub struct Server {
    // ///< Allocator passed in to constructor.
    // ///< The block of memory backing the global allocator. Allocated with m_allocator.
    // ///< The block of memory backing the per-client allocators. Allocated with m_allocator.
    // ///< The global allocator. Used for allocations that don't belong to a specific client.
    // ///< Array of per-client allocator. These are used for allocations related to connected clients.
    // ///< Buffer used when writing packets.
    /// Base client/server config.
    config: ClientServerConfig,
    // ///< The adapter specifies the allocator to use, and the message factory class.
    // // TODO: adapter: Adapter,
    // ///< Optional serialization context.
    // // TODO: context: void*,
    /// Maximum number of clients supported.
    max_clients: usize,
    /// True if server is currently running, eg. after "Start" is called, before "Stop".
    running: bool,
    /// Current server time in seconds.
    time: f64,
    // ///< Array of per-client message factories. This silos message allocations per-client slot.
    // client_message_factory: Vec<MessageFactory>,
    /// Array of per-client connection classes. This is how messages are exchanged with clients.
    client_connection: Vec<Connection>,
    /// Array of per-client reliable.io endpoints.
    client_endpoint: Vec<*mut reliable_endpoint_t>,
    /// The network simulator used to simulate packet loss, latency, jitter etc. Optional.
    network_simulator: Option<()>,

    address: String,
    server: *mut netcode_server_t,
    private_key: [u8; NETCODE_KEY_BYTES],
}

impl Server {
    pub fn new(
        private_key: &[u8; NETCODE_KEY_BYTES],
        address: String,
        config: ClientServerConfig,
        time: f64,
    ) -> Server {
        assert_eq!(private_key.len(), NETCODE_KEY_BYTES);

        Server {
            config,
            max_clients: 0,
            running: false,
            time,
            client_endpoint: Vec::new(),
            client_connection: Vec::new(),
            network_simulator: None,

            server: std::ptr::null_mut(),
            address,
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
            // TODO: network simulator

            /* PORT:
                initialize adapter
                create message factory
            */

            assert!(self.client_connection.is_empty());
            assert!(self.client_endpoint.is_empty());
            for i in 0..max_clients {
                self.client_connection.push(Connection::new());

                let mut reliable_config =
                    gf_init_default!(reliable_config_t, reliable_default_config);

                let name = CStr::from_bytes_with_nul(b"server endpoint\0").unwrap();
                reliable_config.name[..name.to_bytes_with_nul().len()].copy_from_slice(unsafe {
                    &*(name.to_bytes_with_nul() as *const [u8] as *const [i8])
                });
                reliable_config.context = std::ptr::null_mut();
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
                reliable_config.transmit_packet_function = Some(static_transmit_packet_function);
                reliable_config.process_packet_function = Some(static_process_packet_function);

                reliable_config.allocator_context = std::ptr::null_mut();
                reliable_config.allocate_function = None;
                reliable_config.free_function = None;

                unsafe {
                    let endpoint = reliable_endpoint_create(&mut reliable_config, self.time);
                    self.client_endpoint.push(endpoint);
                    reliable_endpoint_reset(*self.client_endpoint.last().unwrap());
                }
            }
            // TODO: allocate packet buffer

            let mut netcode_config =
                gf_init_default!(netcode_server_config_t, netcode_default_server_config);
            netcode_config.protocol_id = self.config.protocol_id;
            netcode_config
                .private_key
                .copy_from_slice(&self.private_key);

            // TODO: default_server_config sets a default allocate function (reliable.io checks if the function is null... more often)
            // netcode_config.allocator_context = std::ptr::null_mut();
            // netcode_config.allocate_function = None;
            // netcode_config.free_function = None;

            netcode_config.callback_context = std::ptr::null_mut(); // TODO
            netcode_config.connect_disconnect_callback = None; // TODO
            netcode_config.send_loopback_packet_callback = None; // TODO

            let server_address = CString::new(self.address.clone()).unwrap();

            self.server = unsafe {
                // TODO: netcode really better not touch it... is this cast OK?
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

            // TODO: get the bound address
        }
    }

    pub fn stop(&mut self) {
        // TODO: review this after server is functioning

        if !self.server.is_null() {
            unsafe {
                netcode_server_stop(self.server);
                netcode_server_destroy(self.server);
            }
            self.server = std::ptr::null_mut();
        }

        {
            /* yojimbo BaseServer::Stop */
            if !self.running {
                self.max_clients = 0;
                return;
            }

            for endpoint in &mut self.client_endpoint {
                unsafe { reliable_endpoint_destroy(*endpoint) };
                *endpoint = std::ptr::null_mut();
            }
            self.client_endpoint.clear();
            self.client_connection.clear();

            self.running = false;
            self.max_clients = 0;
        }
    }

    pub fn send_packets(&mut self) {}

    pub fn receive_packets(&mut self) {}

    pub fn advance_time(&mut self, new_time: f64) {
        if !self.server.is_null() {
            unsafe { netcode_server_update(self.server, self.time) }
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
                    // TODO: eek disconnecting
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

    pub fn running(&self) -> bool {
        self.running
    }
}

impl Server {
    fn transmit_packet_function(
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

    fn process_packet_function(
        &mut self,
        client_index: i32,
        _packet_sequence: u16,
        _packet_data: *mut u8,
        _packet_bytes: i32,
    ) -> i32 {
        let _connection = self.client_connection_mut(client_index);
        // TODO: connection.process_packet()
        0
    }

    fn client_connection_mut(&mut self, client_index: i32) -> &mut Connection {
        assert!(self.running());
        assert!(client_index > 0);
        let client_index = client_index as usize;
        assert!(client_index < self.max_clients);
        assert!(client_index < self.client_connection.len());

        &mut self.client_connection[client_index]
    }
}

unsafe extern "C" fn static_transmit_packet_function(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) {
    let server = context as *mut Server;
    server.as_mut().unwrap().transmit_packet_function(
        index,
        packet_sequence,
        packet_data,
        packet_bytes,
    );
}

unsafe extern "C" fn static_process_packet_function(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) -> i32 {
    let server = context as *mut Server;
    server.as_mut().unwrap().process_packet_function(
        index,
        packet_sequence,
        packet_data,
        packet_bytes,
    )
}
