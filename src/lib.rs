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

/// Determines the reliability and ordering guarantees for a channel.
#[derive(Clone, Copy)]
pub enum ChannelType {
    ReliableOrdered,
    UnreliableUnordered,
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

pub struct ConnectionConfig {
    num_channels: usize,
    max_packet_size: usize,
    channels: [ChannelConfig; MAX_CHANNELS],
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
    connection: ConnectionConfig,
    /// Clients can only connect to servers with the same protocol id. Use this for versioning.
    protocol_id: u64,
    /// Timeout value in seconds. Set to negative value to disable timeouts (for debugging only).
    timeout: i32,
    /// Memory allocated inside Client for packets, messages and stream allocations (bytes)
    client_memory: usize,
    /// Memory allocated inside Server for global connection request and challenge response packets (bytes)
    server_global_memory: usize,
    /// Memory allocated inside Server for packets, messages and stream allocations per-client (bytes)
    server_per_client_memory: usize,
    /// If true then a network simulator is created for simulating latency, jitter, packet loss and duplicates.
    network_simulator: bool,
    /// Maximum number of packets that can be stored in the network simulator. Additional packets are dropped.
    max_simulator_packets: usize,
    /// Packets above this size (bytes) are split apart into fragments and reassembled on the other side.
    fragment_packets_above: usize,
    /// Size of each packet fragment (bytes).
    packet_fragment_size: usize,
    /// Maximum number of fragments a packet can be split up into.
    max_packet_fragments: usize,
    /// Number of packet entries in the fragmentation reassembly buffer.
    packet_reassembly_buffer_size: usize,
    /// Number of packet entries in the acked packet buffer. Consider your packet send rate and aim to have at least a few seconds worth of entries.
    acked_packets_buffer_size: usize,
    /// Number of packet entries in the received packet sequence buffer. Consider your packet send rate and aim to have at least a few seconds worth of entries.
    received_packets_buffer_size: usize,
    /// Round-Trip Time (RTT) smoothing factor over time.
    rtt_smoothing_factor: f32,
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

fn initialize() {
    // TODO
}

fn shutdown() {
    // TODO
}

/**
 * Get a high precision time in seconds since the application has started.
 *
 * Please store time in f64 so you retain sufficient precision as time increases.
 */
fn time() -> f64 {
    // TODO
    0.0
}

// TODO: sequence buffer
// TODO: bit writer, bit reader

pub trait BaseStream {}
pub struct WriteStream {}
pub struct ReadStream {}

// TODO: message factory macros

pub struct ChannelPacketData {
    // TODO: types are a bit different in rust?
    data: ChannelPacketDataInner,
}

pub struct MessageData {}

struct BlockData {}

enum ChannelPacketDataInner {
    Message(MessageData),
    Block(BlockData),
}

impl ChannelPacketData {
    fn new() {}
    // TODO: fn free(message factory?) {}

    // TODO: serialize template method
}

// TODO: channel counters

#[derive(Debug, Clone)]
enum ChannelErrorLevel {
    ///< No error. All is well.
    None,
    ///< This channel has desynced. This means that the connection protocol has desynced and cannot recover. The client should be disconnected.
    Desync,
    ///< The user tried to send a message but the send queue was full. This will assert out in development, but in production it sets this error on the channel.
    SendQueueFull,
    ///< The channel received a packet containing data for blocks, but this channel is configured to disable blocks. See ChannelConfig::disableBlocks.
    BlocksDisabled,
    ///< Serialize read failed for a message sent to this channel. Check your message serialize functions, one of them is returning false on serialize read. This can also be caused by a desync in message read and write.
    FailedToSerialize,
    ///< The channel tried to allocate some memory but couldn't.
    OutOfMemory,
}

trait Channel {
    fn reset(&mut self);

    fn can_send_message(&self) -> bool;
    fn has_messages_to_send(&self) -> bool;

    /// Queue a message to be sent across this channel.
    fn send_message(&mut self, message: &Message, context: TODO);

    /// Pops the next message off the receive queue if one is available.
    ///
    /// TODO: Yojimbo says caller takes ownership, but returns a pointer.
    fn receive_message(&mut self) -> Option<Message>;

    /// Advance channel time.
    ///
    /// Called by Connection::advance_time for each channel configured on the connection.
    fn advance_time(&mut self, time: f64);

    /// Get channel packet data for this channel.
    ///
    /// TODO: this sounds v unsafe!
    fn packet_data(
        &mut self,
        packet_sequence: u16,
        available_bits: usize,
        data: &mut ChannelPacketData,
    ) -> i32;
    fn process_packet_data(&mut self);

    fn process_ack(&mut self, packet_sequence: u16);

    fn error_level(&self) -> ChannelErrorLevel;

    fn channel_index(&self) -> i32;

    // TODO: get_counter/counter, reset_counters

    /// TODO: determine if we want this on the trait
    ///
    /// Fielder's docs say "All errors go through this function to make debug logging easier."
    fn set_error_level(&mut self, level: ChannelErrorLevel);
}

pub struct BaseChannel {
    // TODO: allocator
    config: ChannelConfig,
    channel_index: i32,
    time: f64,
    error_level: ChannelErrorLevel,
    message_factory: MessageFactory,
    // TODO: uint64_t m_counters[CHANNEL_COUNTER_NUM_COUNTERS],
}

// NEXT: ReliableOrderedChannel
