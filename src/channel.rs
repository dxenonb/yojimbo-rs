use std::marker::PhantomData;

use crate::config::ChannelConfig;

// TODO
type SequenceBuffer<T> = PhantomData<T>;

// TODO: channel counters

#[derive(Debug, Clone)]
pub enum ChannelErrorLevel {
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

pub trait Channel {
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

pub struct ReliableOrderedChannel {
    /// Id of the next message to be added to the send queue.
    send_message_id: u16,
    /// Id of the next message to be added to the receive queue.
    receive_message_id: u16,
    /// Id of the oldest unacked message in the send queue.
    oldest_unacked_message_id: u16,
    /// Stores information per sent connection packet about messages and block data included in each packet. Used to walk from connection packet level acks to message and data block fragment level acks.
    sent_packets: SequenceBuffer<SentPacketEntry>,
    /// Message send queue.
    message_send_queue: SequenceBuffer<MessageSendQueueEntry>,
    /// Message receive queue.
    message_receive_queue: SequenceBuffer<MessageReceiveQueueEntry>,
    /// Array of n message ids per sent connection packet. Allows the maximum number of messages per-packet to be allocated dynamically.
    sent_packet_message_ids: Vec<u16>,
    /// Data about the block being currently sent.
    send_block: SendBlockData,
    /// Data about the block being currently received.
    receive_block: ReceiveBlockData,
}

struct MessageSendQueueEntry {
    message: Message,
    time_last_sent: f64,
    measured_bits: u32, // TODO: default: 31 - the number of bits the message takes up in a bit stream
    block: u32, // TODO: 1 if this is a block message. Block messages are treated differently to regular messages when sent over a reliable-ordered channel.
}

struct MessageReceiveQueueEntry {
    message: Message,
}

struct SentPacketEntry {
    /// The time the packet was sent. Used to estimate round trip time.
    time_sent: f64,
    /// Array of message ids. Dynamically allocated because the user can configure the maximum number of messages in a packet per-channel with ChannelConfig::maxMessagesPerPacket.
    message_ids: Vec<u16>,
    /// The number of message ids in in the array.
    num_message_ids: u32,
    /// 1 if this packet has been acked.
    acked: u32,
    /// 1 if this packet contains a fragment of a block message.
    block: u64,
    /// The block message id. Valid only if "block" is 1.
    block_message_id: u64,
    /// The block fragment id. Valid only if "block" is 1.
    block_fragment_id: u16,
}

struct SendBlockData {
    /// True if we are currently sending a block.
    active: bool,
    /// The size of the block (bytes).
    block_size: i32,
    /// Number of fragments in the block being sent.
    num_fragments: i32,
    /// Number of acked fragments in the block being sent.
    num_acked_fragments: i32,
    /// The message id the block is attached to.
    block_message_id: u16,
    /// Has fragment n been received?
    acked_fragment: BitArray,
    /// Last time fragment was sent.
    fragment_send_time: f64,
}

struct ReceiveBlockData {
    /// True if we are currently receiving a block.
    active: bool,
    /// The number of fragments in this block
    num_fragments: i32,
    /// The number of fragments received.
    num_received_fragments: i32,
    /// The message id corresponding to the block.
    message_id: u16,
    /// Message type of the block being received.
    message_type: i32,
    /// Block size in bytes.
    block_size: u32,
    /// Has fragment n been received?
    received_fragment: BitArray,
    /// Block data for receive.
    block_data: u32,
    /// Block message (sent with fragment 0).
    block_message: BlockMessage,
}

impl ReceiveBlockData {
    fn new() -> ReceiveBlockData {
        ReceiveBlockData {
            active: true,
            num_fragments: 0,
            num_received_fragments: BitArray::new(),
            message_id: 0,
            message_type: 0,
            block_size: 0,
            received_fragment: BitArray::new(),
            block_data: BlockData::new(),
            block_message: (),
        }
    }

    fn reset(&mut self) {
        self.active = false;
        self.num_fragments = 0;
        self.num_received_fragments = 0;
        self.message_id = 0;
        self.message_type = 0;
        self.block_size = 0;
    }
}

pub struct ChannelPacketData {
    // TODO: types are a bit different in rust?
    data: ChannelPacketDataInner,
}

struct MessageData {}

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

// TODO: unreliable ordered channel
