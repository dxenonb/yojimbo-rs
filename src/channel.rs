use crate::{
    config::{ChannelConfig, ChannelType},
    message::NetworkMessage,
};

use self::{processor::Processor, reliable::Reliable, unreliable::Unreliable};

mod channel_packet_data;
mod processor;
mod reliable;
mod sequence_buffer;
mod unreliable;

// TODO: encapsulate this better
pub(crate) use channel_packet_data::ChannelPacketData;

#[cfg(feature = "serialize_check")]
pub(crate) const SERIALIZE_CHECK_VALUE: u32 = 0x12345678;

pub(crate) const CONSERVATIVE_MESSAGE_HEADER_BITS: usize = 32;
// pub(crate) const CONSERVATIVE_FRAGMENT_HEADER_BITS: usize = 64;
pub(crate) const CONSERVATIVE_CHANNEL_HEADER_BITS: usize = 32;
pub(crate) const CONSERVATIVE_PACKET_HEADER_BITS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelErrorLevel {
    /// No error. All is well.
    None,
    /// This channel has desynced. This means that the connection protocol has
    /// desynced and cannot recover. The client should be disconnected.
    ///
    /// This happens mainly when you are not handling a client's messages. The
    /// channel falls behind, and has to reject messages. The messages cannot
    /// be dropped automatically because then you would not be guaranteed to
    /// receive messages.
    Desync,
    /// The user tried to send a message but the send queue was full.
    ///
    /// If you get this, you're generally sending messages too fast, or
    /// specifically in the case of reliable channels, not getting responses to
    /// any of your messages.
    ///
    /// If you're sending messages too fast, try increasing the send queue size
    /// or throttle your `send_message` calls.
    ///
    /// If your (reliable) messages don't need replies, the reciever still
    /// needs to send something in order for you to receive any acks. (Acks are
    /// required in order for a reliable channel to dequeue messages.)
    ///
    /// As long as the reciever sends something on *any channel* (within enough
    /// time to prevent your send queue overflowing), that is enough for the
    /// ack to be processed (acks are per packet, and packets contain
    /// information for all channels) and remove some messages from the send
    /// queue.
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
    processor: Box<dyn Processor<M>>,
    counters: ChannelCounters,
}

impl<M: NetworkMessage> Channel<M> {
    pub(crate) fn new(config: ChannelConfig, channel_index: usize, time: f64) -> Channel<M> {
        let processor: Box<dyn Processor<M>> = match config.kind {
            ChannelType::ReliableOrdered => Box::new(Reliable::new(config.clone(), time)),
            ChannelType::UnreliableUnordered => Box::new(Unreliable::new(&config)),
        };
        Channel {
            config,
            channel_index,
            error_level: ChannelErrorLevel::None,
            processor,
            counters: ChannelCounters::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.set_error_level(ChannelErrorLevel::None);
        self.processor.reset();
        self.reset_counters();
    }

    pub fn counters(&self) -> &ChannelCounters {
        &self.counters
    }

    pub fn reset_counters(&mut self) {
        self.counters.reset();
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

    pub(crate) fn process_ack(&mut self, packet_sequence: u16) {
        self.processor.process_ack(packet_sequence)
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

        self.counters.sent += 1;
    }

    pub(crate) fn receive_message(&mut self) -> Option<(u16, M)> {
        if self.error_level() != ChannelErrorLevel::None {
            return None;
        }

        let (id, result) = self.processor.receive_message()?;

        self.counters.received += 1;

        Some((id, result))
    }

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

#[derive(Debug, Copy, Clone, Default)]
pub struct ChannelCounters {
    pub sent: usize,
    pub received: usize,
}

impl ChannelCounters {
    fn reset(&mut self) {
        self.sent = 0;
        self.received = 0;
    }
}
