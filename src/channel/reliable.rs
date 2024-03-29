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

// }

use crate::{
    channel::{channel_packet_data::MeasureSink, CONSERVATIVE_MESSAGE_HEADER_BITS},
    config::{ChannelConfig, ChannelType},
    message::NetworkMessage,
};

use super::{
    processor::Processor,
    sequence_buffer::{sequence_greater_than, sequence_less_than, SequenceBuffer},
    ChannelPacketData,
};

pub(crate) struct Reliable<M> {
    time: f64,
    config: ChannelConfig,

    send_message_id: u16,
    receive_message_id: u16,
    /// Represents the next message we need to send (by ID).
    ///
    /// Call `update_oldest_unacked_message_id` after messages are acked.
    oldest_unacked_message_id: u16,

    /// List of message ids per sent connection packet.
    ///
    /// See `SentPacketEntry` (each instance references a subset of this buffer).
    sent_packet_message_ids: Vec<u16>,

    sent_packets: SequenceBuffer<SentPacketEntry>,
    message_send_queue: SequenceBuffer<MessageSendQueueEntry<M>>,
    message_receive_queue: SequenceBuffer<MessageReceiveQueueEntry<M>>,
}

impl<M> Reliable<M> {
    pub(crate) fn new(config: ChannelConfig, time: f64) -> Reliable<M> {
        assert!(matches!(config.kind, ChannelType::ReliableOrdered));

        let sent_packets = SequenceBuffer::new(config.sent_packet_buffer_size);
        let sent_packet_message_ids =
            vec![0u16; config.max_messages_per_packet * config.sent_packet_buffer_size];
        let message_send_queue = SequenceBuffer::new(config.message_send_queue_size);
        let message_receive_queue = SequenceBuffer::new(config.message_receive_queue_size);

        // TODO: blocks

        Reliable {
            time,
            config,

            send_message_id: 0,
            receive_message_id: 0,
            oldest_unacked_message_id: 0,

            sent_packet_message_ids,

            sent_packets,
            message_send_queue,
            message_receive_queue,
        }
    }
}

impl<M: NetworkMessage> Reliable<M> {
    /// Find all messages in the send queue (respecting channel config) that need to be sent.
    ///
    /// A message is considered for sending if:
    ///  - it should fit in the available bits based on `measured_bits`
    ///  - `message_resend_time` has elapsed or the [message has never been sent]*
    ///  - there are more than 4 bytes available
    ///
    /// * a `time_last_sent` of -1.0 should satisfy this unless resend time is large
    ///
    /// The number of messages we can send is limited by the smaller of the
    /// send and receive queues, as well as `config.packet_budget` and
    /// `config.max_messages_per_packet`.
    ///
    /// Assumes has_messages_to_send (oldest unacked != last message sent) is true.
    fn get_messages_to_send(&mut self, mut available_bits: usize) -> (Vec<u16>, usize) {
        assert!(self.has_messages_to_send());

        let mut message_ids = Vec::new(); // TODO: allocation

        available_bits = self
            .config
            .packet_budget
            .map(|bytes| std::cmp::min(bytes * 8, available_bits))
            .unwrap_or(available_bits);

        let give_up_bits = 4 * 8;
        let message_limit = std::cmp::min(
            self.message_receive_queue.capacity(),
            self.message_send_queue.capacity(),
        );

        let mut used_bits = CONSERVATIVE_MESSAGE_HEADER_BITS;
        let mut give_up_counter = 0;

        for i in 0..message_limit {
            if available_bits - used_bits < give_up_bits {
                break;
            }

            if give_up_counter > self.message_send_queue.capacity() {
                break;
            }

            let message_id = self.oldest_unacked_message_id.wrapping_add(i as u16);

            let Some(entry) = self.message_send_queue.get_mut(message_id) else { continue };

            if entry.time_last_sent + self.config.message_resend_time <= self.time
                && available_bits >= entry.measured_bits
            {
                let mut message_bits = entry.measured_bits;

                // TODO: serialize message id relative to previous message
                message_bits += 2 * 8; // we will serialize a u16 for the message ID

                if used_bits + message_bits > available_bits {
                    give_up_counter += 1;
                    continue;
                }

                used_bits += message_bits;
                message_ids.push(message_id);
                entry.time_last_sent = self.time;
            }

            if message_ids.len() >= self.config.max_messages_per_packet {
                break;
            }
        }

        (message_ids, used_bits)
    }

    /// Generate ChannelPacketData by copying all messages in the send queue
    /// with an ID in `message_ids`.
    fn get_message_packet_data(
        &mut self,
        channel_index: usize,
        message_ids: &[u16],
    ) -> ChannelPacketData<M> {
        let mut messages = Vec::with_capacity(message_ids.len());

        for id in message_ids {
            let message = self.message_send_queue.get(*id).unwrap().message.clone();
            messages.push((*id, message));
        }

        ChannelPacketData {
            channel_index,
            messages,
        }
    }

    /// Add an entry for this sequence number to `sent_packets`.
    fn add_message_packet_entry(&mut self, message_ids: &[u16], packet_sequence: u16) {
        let message_ids_index = ((packet_sequence as usize) % self.config.sent_packet_buffer_size)
            * self.config.max_messages_per_packet;
        let message_ids_run = message_ids.len();
        let message_ids_ref = (message_ids_index, message_ids_run);
        self.sent_packets.insert_with(packet_sequence, || {
            // only write this if this callback runs
            for (i, id) in message_ids.iter().enumerate() {
                self.sent_packet_message_ids[message_ids_index + i] = *id;
            }
            SentPacketEntry {
                acked: false,
                time_sent: self.time,
                message_ids: message_ids_ref,
            }
        });
    }
}

impl<M: NetworkMessage> Processor<M> for Reliable<M> {
    fn advance_time(&mut self, new_time: f64) {
        self.time = new_time;
    }

    fn reset(&mut self) {
        self.send_message_id = 0;
        self.receive_message_id = 0;
        self.oldest_unacked_message_id = 0;

        self.sent_packets.reset();
        self.message_send_queue.reset();
        self.message_receive_queue.reset();

        // TODO: blocks
    }

    /// There are messages to send if oldest_unacked_message_id is "less than"
    /// send_message_id (considering that the ID wraps).
    fn has_messages_to_send(&self) -> bool {
        self.oldest_unacked_message_id != self.send_message_id
    }

    /// New messags can be sent if there is space in the send queue.
    fn can_send_message(&self) -> bool {
        self.message_send_queue.available(self.send_message_id)
    }

    fn send_message(&mut self, message: M) {
        // TODO: return Err if can_send_message is false
        assert!(self.can_send_message());

        // TODO: blocks

        let result = self
            .message_send_queue
            .insert_with(self.send_message_id, || {
                let mut sink = MeasureSink::new();
                message.serialize(&mut sink).unwrap();
                let measured_bits = 8 * sink.bytes;

                MessageSendQueueEntry {
                    message_id: self.send_message_id,
                    message,
                    measured_bits,
                    time_last_sent: -1.0,
                }
            });

        assert!(result, "can_send_message should make this impossible");

        self.send_message_id = self.send_message_id.wrapping_add(1);
    }

    fn receive_message(&mut self) -> Option<(u16, M)> {
        let entry = match self.message_receive_queue.take(self.receive_message_id) {
            Some(entry) => entry,
            None => return None,
        };
        assert_eq!(entry.message_id, self.receive_message_id);

        self.receive_message_id = self.receive_message_id.wrapping_add(1);

        Some((entry.message_id, entry.message))
    }

    fn packet_data(
        &mut self,
        _config: &ChannelConfig,
        channel_index: usize,
        packet_sequence: u16,
        available_bits: usize,
    ) -> (ChannelPacketData<M>, usize) {
        if !self.has_messages_to_send() {
            return (ChannelPacketData::empty(), 0);
        }

        // TODO: blocks

        let (message_ids, message_bits) = self.get_messages_to_send(available_bits);

        if !message_ids.is_empty() {
            let packet_data = self.get_message_packet_data(channel_index, &message_ids[..]);
            self.add_message_packet_entry(&message_ids[..], packet_sequence);
            (packet_data, message_bits)
        } else {
            (ChannelPacketData::empty(), 0)
        }
    }

    fn process_packet_data(&mut self, packet_data: ChannelPacketData<M>, _packet_sequence: u16) {
        // TODO: blocks
        {
            let min_message_id = self.receive_message_id;
            let max_message_id = self
                .receive_message_id
                .wrapping_add((self.message_receive_queue.capacity() - 1) as u16);

            /* yojimbo ReliableOrderedChannel::ProcessPacketMessages */
            for (id, message) in packet_data.messages {
                if sequence_less_than(id, min_message_id) {
                    continue;
                }
                if sequence_greater_than(id, max_message_id) {
                    // Did you forget to dequeue messages on the receiver?
                    panic!("TODO: return a desync error (1), recieved {} but the latest we can handle is {}; are your handling client messages?", id, max_message_id);
                }

                let result =
                    self.message_receive_queue
                        .insert_with(id, || MessageReceiveQueueEntry {
                            message_id: id,
                            message,
                        });

                if !result {
                    // The message we got was too old; are we sending acks?
                    // This should generally be unreachable, SendQueueFull
                    // typically happens first.
                    panic!("TODO: return a desync error (2), received {} but the oldest we can handle is {}", id, min_message_id);
                }
            }
        }
    }

    fn process_ack(&mut self, ack: u16) {
        // figure out which packet was acked
        // (return if this ack appears to be too old/not relevant to this channel)
        let Some(entry) = self.sent_packets.get_mut(ack) else { return; };

        assert!(!entry.acked);
        entry.acked = true;

        // remove all the acked messages from the send queue
        let (first_message, message_count) = entry.message_ids;
        let last_message = first_message + message_count;

        for message_id in &mut self.sent_packet_message_ids[first_message..last_message] {
            let mut take_success = false;
            if let Some(entry) = self.message_send_queue.take(*message_id) {
                assert_eq!(entry.message_id, *message_id);
                take_success = true;
            } // else: this message was probably acked in another packet
            if take_success {
                self.oldest_unacked_message_id = update_oldest_unacked_message_id(
                    self.oldest_unacked_message_id,
                    &self.message_send_queue,
                );
            }
        }

        // TODO: blocks
    }
}

struct MessageSendQueueEntry<M> {
    message_id: u16,
    message: M,
    time_last_sent: f64,
    measured_bits: usize,
}

struct MessageReceiveQueueEntry<M> {
    message_id: u16,
    message: M,
}

struct SentPacketEntry {
    // TODO: investigate the unused warning more
    /// The time the packet was sent. Used to estimate round trip time.
    #[allow(unused)]
    time_sent: f64,
    /// References `sent_packet_message_ids`, in the format (start index, run length)
    message_ids: (usize, usize),
    /// True if this packet has been acked
    acked: bool,
}

/// Advance `oldest_unacked_message_id` until it references
/// something still in the send queue or refers to the next message ID we
/// should use.
///
/// `oldest_unacked_message_id` should be the current (possibly stale)
/// value. Returns the updated value.
fn update_oldest_unacked_message_id<M>(
    mut oldest_unacked_message_id: u16,
    message_send_queue: &SequenceBuffer<MessageSendQueueEntry<M>>,
) -> u16 {
    let stop_message_id = message_send_queue.sequence_pointer();
    loop {
        if oldest_unacked_message_id == stop_message_id
            || message_send_queue.exists(oldest_unacked_message_id)
        {
            break;
        }
        oldest_unacked_message_id = oldest_unacked_message_id.wrapping_add(1);
    }
    assert!(!sequence_greater_than(
        oldest_unacked_message_id,
        stop_message_id
    ));
    oldest_unacked_message_id
}

// TODO: fix https://github.com/networkprotocol/yojimbo/issues/138
