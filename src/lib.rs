use config::ChannelConfig;

pub mod config;

pub fn initialize() {
    // TODO
}

pub fn shutdown() {
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
