use std::{collections::VecDeque, mem::size_of};

use crate::{
    channel::channel_packet_data::MeasureSink,
    config::{ChannelConfig, ChannelType},
    message::NetworkMessage,
};

use super::{
    channel_packet_data::ChannelPacketData, processor::Processor, CONSERVATIVE_MESSAGE_HEADER_BITS,
};

/// Messages sent across this channel are not guaranteed to arrive, and may be received in a different order than they were sent.
/// This channel type is best used for time critical data like snapshots and object state.
pub(crate) struct Unreliable<M = ()> {
    message_send_queue: VecDeque<M>,
    message_receive_queue: VecDeque<(u16, M)>,
}

impl<M> Unreliable<M> {
    pub(crate) fn new(config: &ChannelConfig) -> Unreliable<M> {
        debug_assert_eq!(config.kind, ChannelType::UnreliableUnordered);

        let send_capacity = std::cmp::max(config.message_send_queue_size / size_of::<M>(), 1);
        let receive_capacity = std::cmp::max(config.message_receive_queue_size / size_of::<M>(), 1);

        Unreliable {
            message_send_queue: VecDeque::with_capacity(send_capacity),
            message_receive_queue: VecDeque::with_capacity(receive_capacity),
        }
    }
}

impl<M: NetworkMessage> Processor<M> for Unreliable<M> {
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

    fn receive_message(&mut self) -> Option<(u16, M)> {
        self.message_receive_queue.pop_front()
    }

    fn packet_data(
        &mut self,
        config: &ChannelConfig,
        channel_index: usize,
        packet_sequence: u16,
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

            let mut sink = MeasureSink::new();
            message.serialize(&mut sink).unwrap();
            let message_bits = 8 * sink.bytes;

            if used_bits + message_bits > available_bits {
                continue;
            }

            used_bits += message_bits;

            assert!(used_bits <= available_bits);

            messages.push((packet_sequence, message));
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

    fn process_packet_data(&mut self, packet_data: ChannelPacketData<M>, packet_sequence: u16) {
        for (_, message) in packet_data.messages {
            if self.message_receive_queue.len() < self.message_receive_queue.capacity() {
                // the packet_sequence overrides any ID that may have been set
                self.message_receive_queue
                    .push_back((packet_sequence, message));
            }
        }
    }

    fn process_ack(&mut self, _ack: u16) {
        /* no-op for unreliable channels */
    }
}
