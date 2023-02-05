#![allow(unused)]

const MAX_CLIENTS: u32 = 64;
const MAX_CHANNELS: usize = 64;
const KEY_BYTES: usize = 32;
const CONNECT_TOKEN_BYTES: usize = 2048;
const SERIALIZE_CHECK_VALUE: u32 = 0x12345678;
const CONSERVATIVE_MESSAGE_HEADER_BITS: usize = 32;
const CONSERVATIVE_FRAGMENT_HEADER_BITES: usize = 64;
const CONSERVATIVE_CHANNEL_HEADER_BITS: usize = 32;
const CONSERVATIVE_PACKET_HEADER_BITS: usize = 16;
const YOJIMBO_DEFAULT_TIMEOUT: i32 = 5;

pub struct ConnectionConfig {
    pub num_channels: usize,
    pub max_packet_size: usize,
    pub channels: [ChannelConfig; MAX_CHANNELS],
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        ConnectionConfig {
            num_channels: 1,
            max_packet_size: 8 * 1024,
            channels: [ChannelConfig::new(ChannelType::ReliableOrdered); MAX_CHANNELS],
        }
    }
}

// TODO: figure out where ConnectionConfig is used and this is not
// (yojimbo has CSC inherit from ConnectionConfig)
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
    /// If true then a network simulator is created for simulating latency, jitter, packet loss and duplicates.
    pub network_simulator: bool,
    /// Maximum number of packets that can be stored in the network simulator. Additional packets are dropped.
    pub max_simulator_packets: usize,
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

impl Default for ClientServerConfig {
    fn default() -> Self {
        let connection = ConnectionConfig::default();
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
            network_simulator: true,
            max_simulator_packets: 4 * 1024,
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

#[derive(Clone, Copy)]
pub struct ChannelConfig {
    kind: ChannelType,
    disable_blocks: bool,
    sent_packet_buffer_size: usize,
    message_send_queue_size: usize,
    message_receive_queue_size: usize,
    max_messages_per_packet: usize,
    packet_budget: i32,
    max_block_size: usize,
    block_fragment_size: usize,
    message_resend_time: f32,
    block_fragment_resend_time: f32,
}

impl ChannelConfig {
    pub fn new(kind: ChannelType) -> Self {
        ChannelConfig {
            kind,
            disable_blocks: false,
            sent_packet_buffer_size: 1024,
            message_send_queue_size: 1024,
            message_receive_queue_size: 1024,
            max_messages_per_packet: 256,
            packet_budget: -1,
            max_block_size: 256 * 1024,
            block_fragment_size: 1024,
            message_resend_time: 0.1,
            block_fragment_resend_time: 0.25,
        }
    }

    pub fn max_fragments_per_block(&self) -> usize {
        self.max_block_size / self.block_fragment_size
    }
}

/// Determines the reliability and ordering guarantees for a channel.
#[derive(Clone, Copy)]
pub enum ChannelType {
    ReliableOrdered,
    UnreliableUnordered,
}
