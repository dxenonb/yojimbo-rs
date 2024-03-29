use crate::bindings::*;
use crate::gf_init_default;
use crate::network_simulator::NetworkSimulatorConfig;
use std::ffi::c_void;
use std::ffi::CString;

const YOJIMBO_DEFAULT_TIMEOUT: i32 = 5;

#[derive(Debug, Clone)]
pub struct ClientServerConfig {
    pub connection: ConnectionConfig,
    /// Clients can only connect to servers with the same protocol id. Use this for versioning.
    pub protocol_id: u64,
    /// Timeout value in seconds. Set to negative value to disable timeouts (for debugging only).
    pub timeout: i32,
    /// Memory allocated inside Client for packets, messages and stream allocations (bytes)
    pub client_memory: usize,
    /// Memory allocated inside Server for global connection request and challenge response packets (bytes)
    pub server_global_memory: usize,
    /// Memory allocated inside Server for packets, messages and stream allocations per-client (bytes)
    pub server_per_client_memory: usize,
    /// If Some, then a network simulator is allocated for simulating latency, jitter, packet loss and duplicates.
    ///
    /// If None, nothing is created, and `with_network_simulator` calls are ignored.
    ///
    /// *IMPORTANT:* Supplying a network simulator config does not make the network simulator active. You have
    /// to call the `set_{property}` methods via one of `network_simulator_mut` or `with_network_simulator`
    /// methods on the client and server for the network simulator to be active/have any affect.
    pub network_simulator: Option<NetworkSimulatorConfig>,
    /// Packets above this size (bytes) are split apart into fragments and reassembled on the other side.
    pub fragment_packets_above: usize,
    /// Size of each packet fragment (bytes).
    pub packet_fragment_size: usize,
    /// Maximum number of fragments a packet can be split up into.
    pub max_packet_fragments: usize,
    /// Number of packet entries in the fragmentation reassembly buffer.
    pub packet_reassembly_buffer_size: usize,
    /// Number of packet entries in the acked packet buffer. Consider your packet send rate and aim to have at least a few seconds worth of entries.
    pub acked_packets_buffer_size: usize,
    /// Number of packet entries in the received packet sequence buffer. Consider your packet send rate and aim to have at least a few seconds worth of entries.
    pub received_packets_buffer_size: usize,
    /// Round-Trip Time (RTT) smoothing factor over time.
    pub rtt_smoothing_factor: f32,
}

impl ClientServerConfig {
    pub fn new(channels: usize) -> Self {
        let connection = ConnectionConfig::new(channels);
        let packet_fragment_size = 1024;
        let max_packet_fragments =
            (connection.max_packet_size as f64 / packet_fragment_size as f64).ceil() as _;
        ClientServerConfig {
            connection,
            protocol_id: 0,
            timeout: YOJIMBO_DEFAULT_TIMEOUT,
            client_memory: 10 * 1024 * 1024,
            server_global_memory: 10 * 1024 * 1024,
            server_per_client_memory: 10 * 1024 * 1024,
            network_simulator: None,
            fragment_packets_above: 1024,
            packet_fragment_size: 1024,
            max_packet_fragments,
            packet_reassembly_buffer_size: 64,
            acked_packets_buffer_size: 256,
            received_packets_buffer_size: 256,
            rtt_smoothing_factor: 0.0025,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub max_packet_size: usize,
    pub channels: Vec<ChannelConfig>,
}

impl ConnectionConfig {
    fn new(channels: usize) -> Self {
        let channels = vec![ChannelConfig::new(ChannelType::ReliableOrdered); channels];
        ConnectionConfig {
            max_packet_size: 8 * 1024,
            channels,
        }
    }
}

pub(crate) type ReliableTransmitPacketFn = unsafe extern "C" fn(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
);

pub(crate) type ReliableProcessPacketFn = unsafe extern "C" fn(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) -> i32;

impl ClientServerConfig {
    pub(crate) fn new_reliable_config(
        &self,
        context: *mut c_void,
        name: &str,
        client_index: Option<usize>,
        transmit_packet: ReliableTransmitPacketFn,
        process_packet: ReliableProcessPacketFn,
    ) -> reliable_config_t {
        let mut reliable_config = gf_init_default!(reliable_config_t, reliable_default_config);

        let name = CString::new(name).unwrap();
        reliable_config.name[..name.to_bytes_with_nul().len()]
            .copy_from_slice(unsafe { &*(name.to_bytes_with_nul() as *const [u8] as *const [i8]) });
        reliable_config.context = context;
        reliable_config.index = client_index.unwrap_or(0) as _;
        reliable_config.max_packet_size = self.connection.max_packet_size as _;
        reliable_config.fragment_above = self.fragment_packets_above as _;
        reliable_config.max_fragments = self.max_packet_fragments as _;
        reliable_config.fragment_size = self.packet_fragment_size as _;
        reliable_config.ack_buffer_size = self.acked_packets_buffer_size as _;
        reliable_config.received_packets_buffer_size = self.received_packets_buffer_size as _;
        reliable_config.fragment_reassembly_buffer_size = self.packet_reassembly_buffer_size as _;
        reliable_config.rtt_smoothing_factor = self.rtt_smoothing_factor;
        reliable_config.transmit_packet_function = Some(transmit_packet);
        reliable_config.process_packet_function = Some(process_packet);

        // don't override `reliable`'s default allocator
        // reliable_config.allocator_context = std::ptr::null_mut();
        // reliable_config.allocate_function = None;
        // reliable_config.free_function = None;

        reliable_config
    }
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub kind: ChannelType,
    pub sent_packet_buffer_size: usize,
    pub message_send_queue_size: usize,
    pub message_receive_queue_size: usize,
    /// Maximum number of messages per packet.
    ///
    /// Note that this currently has a limitation of 256 due to the way that
    /// messages are serialized ([message count - 1] is serialized as a byte). If you
    /// feel like implementing dynamic integer serialization, go for it! PRs welcome!
    pub max_messages_per_packet: usize,
    /// Maximum amount of message data to write to the packet for this channel (bytes). Specifying None means the channel can use up to the rest of the bytes remaining in the packet.
    pub packet_budget: Option<usize>,
    pub message_resend_time: f64,
    pub block_fragment_resend_time: f64,
    // TODO: blocks: pub max_block_size: usize, pub block_fragment_size: usize, pub disable_blocks: bool,
}

impl ChannelConfig {
    pub fn new(kind: ChannelType) -> Self {
        ChannelConfig {
            kind,
            sent_packet_buffer_size: 1024,
            message_send_queue_size: 1024,
            message_receive_queue_size: 1024,
            max_messages_per_packet: 256,
            packet_budget: None,
            message_resend_time: 0.1,
            block_fragment_resend_time: 0.25,
            // TODO: blocks:
            // disable_blocks: false,
            // max_block_size: 256 * 1024,
            // block_fragment_size: 1024,
        }
    }

    // pub fn max_fragments_per_block(&self) -> usize {
    //     self.max_block_size / self.block_fragment_size
    // }
}

/// Determines the reliability and ordering guarantees for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    ReliableOrdered,
    UnreliableUnordered,
}
