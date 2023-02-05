use std::ffi::c_void;
use std::mem::MaybeUninit;

use crate::config::ClientServerConfig;
use crate::reliable::*;

pub struct Server {
    ///< Allocator passed in to constructor.
    ///< The adapter specifies the allocator to use, and the message factory class.
    ///< Optional serialization context.
    ///< The block of memory backing the global allocator. Allocated with m_allocator.
    ///< The block of memory backing the per-client allocators. Allocated with m_allocator.
    ///< The global allocator. Used for allocations that don't belong to a specific client.
    ///< Array of per-client allocator. These are used for allocations related to connected clients.
    ///< Array of per-client message factories. This silos message allocations per-client slot.
    ///< Array of per-client connection classes. This is how messages are exchanged with clients.
    ///< Array of per-client reliable.io endpoints.
    ///< The network simulator used to simulate packet loss, latency, jitter etc. Optional.
    ///< Buffer used when writing packets.

    /// Base client/server config.
    config: ClientServerConfig,
    // // TODO: adapter: Adapter,
    // // TODO: context: void*,
    /// Maximum number of clients supported.
    max_clients: u32,
    /// True if server is currently running, eg. after "Start" is called, before "Stop".
    running: bool,
    /// Current server time in seconds.
    time: f64,
    // client_message_factory: Vec<MessageFactory>,
    // client_connection: Vec<Connection>,
    // TODO client_endpoint: reliable_endpoint_t,
    network_simulator: Option<()>,
}

const PK_BYTES: usize = 128;

type Address = ();

impl Server {
    pub fn new(
        private_key: Vec<u8>,
        address: &Address,
        config: ClientServerConfig,
        time: f64,
    ) -> Server {
        assert_eq!(private_key.len(), PK_BYTES);

        Server {
            config,
            max_clients: 0,
            running: false,
            time,
            network_simulator: None,
        }
    }

    pub fn start(&mut self, max_clients: u32) {
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

            for i in 0..max_clients {
                let mut reliable_config = unsafe {
                    let mut reliable_config = MaybeUninit::uninit();
                    reliable_default_config(reliable_config.as_mut_ptr());
                    reliable_config.assume_init()
                };
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
                reliable_config.transmit_packet_function = Some(static_transmit_packet_function); // TODO
                reliable_config.process_packet_function = None; // TODO
                reliable_config.allocator_context = std::ptr::null_mut();
                reliable_config.allocate_function = None;
                reliable_config.free_function = None;
            }
        }
    }

    pub fn stop(&mut self) {}

    pub fn send_packets(&mut self) {}

    pub fn receive_packets(&mut self) {}

    pub fn advance_time(&mut self, time: f64) {
        self.time += time;
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
        packet_data: *const u8,
        packet_bytes: i32,
    ) {
        if let Some(_) = self.network_simulator {
            unimplemented!(); // TODO
        }
        // netcode_server_send_packet(self.server, client_index, packet_data, packet_bytes);
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
