use std::{collections::VecDeque, io::Cursor, mem::size_of};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    config::{ChannelConfig, ChannelType, ConnectionConfig},
    message::NetworkMessage,
};

pub(crate) const CONSERVATIVE_MESSAGE_HEADER_BITS: usize = 32;
// pub(crate) const CONSERVATIVE_FRAGMENT_HEADER_BITES: usize = 64;
pub(crate) const CONSERVATIVE_CHANNEL_HEADER_BITS: usize = 32;
pub(crate) const CONSERVATIVE_PACKET_HEADER_BITS: usize = 16;

// TODO: channel counters

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelErrorLevel {
    /// No error. All is well.
    None,
    /// This channel has desynced. This means that the connection protocol has desynced and cannot recover. The client should be disconnected.
    Desync,
    /// The user tried to send a message but the send queue was full. This will assert out in development, but in production it sets this error on the channel.
    SendQueueFull,
    /// The channel received a packet containing data for blocks, but this channel is configured to disable blocks. See ChannelConfig::disableBlocks.
    BlocksDisabled,
    /// Serialize read failed for a message sent to this channel. Check your message serialize functions, one of them is returning false on serialize read. This can also be caused by a desync in message read and write.
    FailedToSerialize,
    /// The channel tried to allocate some memory but couldn't.
    OutOfMemory,
}

pub struct Channel<M> {
    config: ChannelConfig,
    channel_index: usize,
    error_level: ChannelErrorLevel,
    processor: Unreliable<M>,
    // TODO: uint64_t m_counters[CHANNEL_COUNTER_NUM_COUNTERS],
}

impl<M: NetworkMessage> Channel<M> {
    pub(crate) fn new(config: ChannelConfig, channel_index: usize, _time: f64) -> Channel<M> {
        if !matches!(config.kind, ChannelType::UnreliableUnordered) {
            unimplemented!("reliable ordered channels not implemented");
        }
        let processor = Unreliable::new(&config);
        Channel {
            config,
            channel_index,
            error_level: ChannelErrorLevel::None,
            processor,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.set_error_level(ChannelErrorLevel::None);
        self.processor.reset();
        // COUNTERS: reset
    }

    /// Advance channel time.
    ///
    /// Called by Connection::advance_time for each channel configured on the connection.
    pub(crate) fn advance_time(&mut self, time: f64) {
        self.processor.advance_time(time);
    }

    /// Get channel packet data for this channel.
    pub(crate) fn packet_data(
        &mut self,
        packet_sequence: u16,
        available_bits: usize,
    ) -> (ChannelPacketData<M>, usize) {
        self.processor.packet_data(
            &self.config,
            self.channel_index,
            packet_sequence,
            available_bits,
        )
    }

    pub(crate) fn process_packet_data(
        &mut self,
        packet_data: ChannelPacketData<M>,
        packet_sequence: u16,
    ) {
        if self.error_level() != ChannelErrorLevel::None {
            return;
        }
        // TODO: detect failed_to_serialize (maybe do this in the connection?)
        self.processor
            .process_packet_data(packet_data, packet_sequence);
    }

    pub(crate) fn process_ack(&mut self, _packet_sequence: u16) {
        // TODO: implement (only needed for reliable)
    }

    pub(crate) fn error_level(&self) -> ChannelErrorLevel {
        self.error_level
    }

    pub(crate) fn can_send_message(&self) -> bool {
        self.processor.can_send_message()
    }

    pub(crate) fn has_messages_to_send(&self) -> bool {
        self.processor.has_messages_to_send()
    }

    pub(crate) fn send_message(&mut self, message: M) {
        if self.error_level() != ChannelErrorLevel::None {
            return;
        }

        if !self.can_send_message() {
            self.set_error_level(ChannelErrorLevel::SendQueueFull);
            return;
        }

        self.processor.send_message(message);

        // TODO: counters
    }

    pub(crate) fn receive_message(&mut self) -> Option<M> {
        if self.error_level() != ChannelErrorLevel::None {
            return None;
        }

        self.processor.receive_message()

        // TODO: counters
    }

    // TODO: get_counter/counter, reset_counters

    /// All errors go through this function to make debug logging easier.
    fn set_error_level(&mut self, level: ChannelErrorLevel) {
        if self.error_level != level && level != ChannelErrorLevel::None {
            log::error!("channel went into error state: {:?}", level);
        }
        self.error_level = level;
    }

    // /// Queue a message to be sent across this channel.
    // fn send_message(&mut self, message: &Message);

    // /// Pops the next message off the receive queue if one is available.
    // fn receive_message(&mut self) -> Option<Message>;
}

/// Messages sent across this channel are not guaranteed to arrive, and may be received in a different order than they were sent.
/// This channel type is best used for time critical data like snapshots and object state.
struct Unreliable<M = ()> {
    message_send_queue: VecDeque<M>,
    message_receive_queue: VecDeque<M>,
}

impl<M: NetworkMessage> Unreliable<M> {
    fn new(config: &ChannelConfig) -> Unreliable<M> {
        debug_assert_eq!(config.kind, ChannelType::UnreliableUnordered);

        let send_capacity = std::cmp::max(config.message_send_queue_size / size_of::<M>(), 1);
        let receive_capacity = std::cmp::max(config.message_receive_queue_size / size_of::<M>(), 1);

        Unreliable {
            message_send_queue: VecDeque::with_capacity(send_capacity),
            message_receive_queue: VecDeque::with_capacity(receive_capacity),
        }
    }

    fn advance_time(&mut self, _new_time: f64) {
        /* no-op for unreliable channels */
    }

    fn reset(&mut self) {
        self.message_send_queue.clear();
        self.message_receive_queue.clear();
    }

    fn can_send_message(&self) -> bool {
        debug_assert!(self.message_send_queue.capacity() > 0);
        self.message_send_queue.len() < self.message_send_queue.capacity()
    }

    fn has_messages_to_send(&self) -> bool {
        self.message_send_queue.is_empty()
    }

    fn send_message(&mut self, message: M) {
        self.message_send_queue.push_back(message)
    }

    fn receive_message(&mut self) -> Option<M> {
        self.message_receive_queue.pop_front()
    }

    fn packet_data(
        &mut self,
        config: &ChannelConfig,
        channel_index: usize,
        _packet_sequence: u16,
        mut available_bits: usize,
    ) -> (ChannelPacketData<M>, usize) {
        if self.message_send_queue.is_empty() {
            return (ChannelPacketData::empty(), 0);
        }

        if let Some(packet_budget) = config.packet_budget {
            if packet_budget == 0 {
                log::warn!("packet_budget is 0, so no messages can be written to this channel");
            }
            available_bits = std::cmp::min(packet_budget * 8, available_bits);
        }

        let mut used_bits = CONSERVATIVE_MESSAGE_HEADER_BITS;
        let give_up_bits = 4 * 8;

        let mut messages = Vec::new();

        loop {
            let message = match self.message_send_queue.pop_front() {
                Some(message) => message,
                None => break,
            };

            if available_bits.saturating_sub(used_bits) < give_up_bits {
                break;
            }

            if messages.len() == config.max_messages_per_packet {
                break;
            }

            // TODO: block message

            // TODO: something analagous to measure stream
            // - assuming Message is an enum (and does not wrap a pointer to something), it should always be the same size
            // - the (network) serialized version should generally be smaller
            let message_bits = size_of::<M>();

            if used_bits + message_bits > available_bits {
                continue;
            }

            used_bits += message_bits;

            assert!(used_bits <= available_bits);

            messages.push(message);
        }

        if messages.is_empty() {
            return (ChannelPacketData::empty(), 0);
        }

        let packet_data = ChannelPacketData {
            channel_index: channel_index as _,
            messages,
        };

        (packet_data, used_bits)
    }

    fn process_packet_data(&mut self, packet_data: ChannelPacketData<M>, _packet_sequence: u16) {
        for message in packet_data.messages {
            // TODO: set packet_sequence on Message
            if self.message_receive_queue.len() < self.message_receive_queue.capacity() {
                self.message_receive_queue.push_back(message);
            }
        }
    }
}

pub(crate) struct ChannelPacketData<M> {
    pub(crate) channel_index: usize,
    pub(crate) messages: Vec<M>,
}

impl<M: NetworkMessage> ChannelPacketData<M> {
    pub(crate) fn serialize(
        &self,
        config: &ConnectionConfig,
        dest: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        dest.write_u16::<LittleEndian>(self.channel_index.try_into().unwrap())
            .unwrap();

        // TODO: block messages

        // TODO: serialize reliable messages

        let config = &config.channels[self.channel_index];
        self.serialize_unordered(config, dest)?;
        Ok(())
    }

    pub(crate) fn deserialize(
        config: &ConnectionConfig,
        src: &mut Cursor<&[u8]>,
    ) -> Result<ChannelPacketData<M>, M::Error> {
        let channel_index = src.read_u16::<LittleEndian>().unwrap() as _;

        // TODO: block messages

        // TODO: deserialize reliable messages

        let config = &config.channels[channel_index];
        let messages = ChannelPacketData::deserialize_unordered(config, src).unwrap();

        Ok(ChannelPacketData {
            channel_index,
            messages,
        })
    }

    fn serialize_unordered(
        &self,
        config: &ChannelConfig,
        mut writer: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        let has_messages = self.messages.len() > 0;

        writer.write_u8(if has_messages { 1 } else { 0 }).unwrap();

        if !has_messages {
            return Ok(());
        }

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize,);
        writer
            .write_u8((self.messages.len() - 1).try_into().unwrap())
            .unwrap();

        for message in &self.messages {
            message.serialize(&mut writer)?;
        }

        Ok(())
    }

    fn deserialize_unordered(
        config: &ChannelConfig,
        mut reader: &mut Cursor<&[u8]>,
    ) -> Result<Vec<M>, M::Error> {
        let has_messages = reader.read_u8().unwrap() == 1;

        if !has_messages {
            return Ok(Vec::new());
        }

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize);
        let message_count = 1 + reader.read_u8().unwrap() as usize;
        let mut messages = Vec::with_capacity(message_count);

        for _ in 0..message_count {
            messages.push(M::deserialize(&mut reader)?);
        }

        Ok(messages)
    }

    fn empty() -> ChannelPacketData<M> {
        ChannelPacketData {
            channel_index: usize::MAX,
            messages: Vec::new(),
        }
    }
}

// struct Reliable {
//     /// Id of the next message to be added to the send queue.
//     send_message_id: u16,
//     /// Id of the next message to be added to the receive queue.
//     receive_message_id: u16,
//     /// Id of the oldest unacked message in the send queue.
//     oldest_unacked_message_id: u16,
//     /// Stores information per sent connection packet about messages and block data included in each packet. Used to walk from connection packet level acks to message and data block fragment level acks.
//     // sent_packets: SequenceBuffer<SentPacketEntry>,
//     /// Message send queue.
//     // message_send_queue: SequenceBuffer<MessageSendQueueEntry>,
//     /// Message receive queue.
//     // message_receive_queue: SequenceBuffer<MessageReceiveQueueEntry>,
//     /// Array of n message ids per sent connection packet. Allows the maximum number of messages per-packet to be allocated dynamically.
//     sent_packet_message_ids: Vec<u16>,
//     /// Data about the block being currently sent.
//     // send_block: SendBlockData,
//     /// Data about the block being currently received.
//     // receive_block: ReceiveBlockData,
// }

// impl Reliable {
//     fn new() -> Reliable {
//         Reliable {}
//     }
// }

// struct MessageSendQueueEntry {
//     message: Message,
//     time_last_sent: f64,
//     measured_bits: u32,
//     block: u32,
// }

// struct MessageReceiveQueueEntry {
//     message: Message,
// }

// struct SentPacketEntry {
//     /// The time the packet was sent. Used to estimate round trip time.
//     time_sent: f64,
//     /// Array of message ids. Dynamically allocated because the user can configure the maximum number of messages in a packet per-channel with ChannelConfig::maxMessagesPerPacket.
//     message_ids: Vec<u16>,
//     /// The number of message ids in in the array.
//     num_message_ids: u32,
//     /// 1 if this packet has been acked.
//     acked: u32,
//     /// 1 if this packet contains a fragment of a block message.
//     block: u64,
//     /// The block message id. Valid only if "block" is 1.
//     block_message_id: u64,
//     /// The block fragment id. Valid only if "block" is 1.
//     block_fragment_id: u16,
// }

// struct SendBlockData {
//     /// True if we are currently sending a block.
//     active: bool,
//     /// The size of the block (bytes).
//     block_size: i32,
//     /// Number of fragments in the block being sent.
//     num_fragments: i32,
//     /// Number of acked fragments in the block being sent.
//     num_acked_fragments: i32,
//     /// The message id the block is attached to.
//     block_message_id: u16,
//     /// Has fragment n been received?
//     acked_fragment: BitArray,
//     /// Last time fragment was sent.
//     fragment_send_time: f64,
// }

// struct ReceiveBlockData {
//     /// True if we are currently receiving a block.
//     active: bool,
//     /// The number of fragments in this block
//     num_fragments: i32,
//     /// The number of fragments received.
//     num_received_fragments: i32,
//     /// The message id corresponding to the block.
//     message_id: u16,
//     /// Message type of the block being received.
//     message_type: i32,
//     /// Block size in bytes.
//     block_size: u32,
//     /// Has fragment n been received?
//     received_fragment: BitArray,
//     /// Block data for receive.
//     block_data: u32,
//     /// Block message (sent with fragment 0).
//     block_message: BlockMessage,
// }

// impl ReceiveBlockData {
//     fn new() -> ReceiveBlockData {
//         ReceiveBlockData {
//             active: true,
//             num_fragments: 0,
//             num_received_fragments: BitArray::new(),
//             message_id: 0,
//             message_type: 0,
//             block_size: 0,
//             received_fragment: BitArray::new(),
//             block_data: BlockData::new(),
//             block_message: (),
//         }
//     }

//     fn reset(&mut self) {
//         self.active = false;
//         self.num_fragments = 0;
//         self.num_received_fragments = 0;
//         self.message_id = 0;
//         self.message_type = 0;
//         self.block_size = 0;
//     }
// }

// TODO: fix https://github.com/networkprotocol/yojimbo/issues/138
