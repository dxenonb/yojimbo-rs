use std::{io::Cursor, slice};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    channel::{
        Channel, ChannelCounters, ChannelErrorLevel, ChannelPacketData,
        CONSERVATIVE_CHANNEL_HEADER_BITS, CONSERVATIVE_PACKET_HEADER_BITS,
    },
    config::ConnectionConfig,
    message::NetworkMessage,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionErrorLevel {
    /// No error. All is well.
    None,
    /// A channel is in an error state.
    Channel,
    /// Failed to read packet. Received an invalid packet?     
    ReadPacketFailed,
}

/// Sends and receives messages across a set of user defined channels.
pub(crate) struct Connection<M> {
    config: ConnectionConfig,
    channels: Vec<Channel<M>>,
    error_level: ConnectionErrorLevel,
}

impl<M: NetworkMessage> Connection<M> {
    pub(crate) fn new(config: ConnectionConfig, time: f64) -> Connection<M> {
        assert!(!config.channels.is_empty());

        let mut channels = Vec::with_capacity(config.channels.len());
        for (channel_index, channel_config) in config.channels.iter().enumerate() {
            channels.push(Channel::new(channel_config.clone(), channel_index, time));
        }

        Connection {
            config,
            channels,
            error_level: ConnectionErrorLevel::None,
        }
    }

    pub(crate) fn advance_time(&mut self, new_time: f64) {
        for channel in &mut self.channels {
            channel.advance_time(new_time);

            if channel.error_level() != ChannelErrorLevel::None {
                self.error_level = ConnectionErrorLevel::Channel;
                return; // VERIFY: should this definitely be a return?
            }
        }
    }

    pub(crate) fn error_level(&self) -> ConnectionErrorLevel {
        self.error_level
    }

    pub(crate) unsafe fn process_acks(&mut self, acks: *mut u16, num_acks: i32) {
        for i in 0..(num_acks as isize) {
            for channel in &mut self.channels {
                channel.process_ack(*acks.offset(i));
            }
        }
    }

    pub(crate) unsafe fn process_packet(
        &mut self,
        packet_sequence: u16,
        packet_data: *const u8,
        packet_bytes: usize,
    ) -> bool {
        if self.error_level() != ConnectionErrorLevel::None {
            log::debug!("failed to read packet because connection is in error state");
            return false;
        }

        let mut packet = ConnectionPacket::new(Vec::new());

        {
            /* yojimbo Connection::ReadPacket */
            assert!(!packet_data.is_null());
            assert!(packet_bytes > 0);

            packet
                .deserialize(&self.config, packet_data, packet_bytes)
                .expect("failed to deserialize");
            // TODO: error handling
        }

        for entry in packet.channel_data {
            let channel_index = entry.channel_index;
            if channel_index > self.channels.len() {
                log::error!(
                    "server received packet for channel that does not exist: {}",
                    entry.channel_index
                );
                continue;
            }
            let channel = &mut self.channels[entry.channel_index];
            channel.process_packet_data(entry, packet_sequence);
            if channel.error_level() != ChannelErrorLevel::None {
                log::debug!(
                    "failed to read packet because channel {} is in error state",
                    channel_index
                );
                return false;
            }
        }

        true
    }

    /// Generate a packet, writing to packet_data.
    ///
    /// Returns the *number of bytes* written (not bits, which are tracked in the function body).
    ///
    /// Caller should call `reliable_endpoint_send_packet` after this if bytes were written.
    /// Reliable will then call the `transmit_packet` callback as appropriate (possibly
    /// fragmenting the generated packet).
    pub(crate) fn generate_packet(
        &mut self,
        packet_sequence: u16,
        packet_data: &mut [u8],
    ) -> usize {
        if self.channels.is_empty() {
            return 0;
        }

        // REFACTOR: consider caching
        let mut channel_data = Vec::new();

        assert!(!packet_data.is_empty());
        let mut available_bits = packet_data.len() * 8 - CONSERVATIVE_PACKET_HEADER_BITS;

        for channel in &mut self.channels {
            let (packet_data, packet_data_bits) =
                channel.packet_data(packet_sequence, available_bits);
            if packet_data_bits > 0 {
                #[cfg(feature = "soak_debugging_asserts")]
                {
                    assert!(
                        packet_data_bits + CONSERVATIVE_CHANNEL_HEADER_BITS < available_bits,
                        "available: {}, packet + header = {} + {}",
                        available_bits,
                        packet_data_bits,
                        CONSERVATIVE_CHANNEL_HEADER_BITS
                    );
                }
                available_bits -= CONSERVATIVE_CHANNEL_HEADER_BITS;
                available_bits -= packet_data_bits;
                channel_data.push(packet_data);
            }
        }

        if !channel_data.is_empty() {
            let packet = ConnectionPacket::new(channel_data);
            packet
                .serialize(&self.config, packet_data)
                .expect("failed to deserialize")
            // TODO: error handling
        } else {
            0
        }
    }

    pub(crate) fn reset(&mut self) {
        self.error_level = ConnectionErrorLevel::None;
        for channel in &mut self.channels {
            channel.reset();
        }
    }

    pub(crate) fn channel_counters(&self, channel: usize) -> &ChannelCounters {
        self.channels[channel].counters()
    }

    pub(crate) fn can_send_message(&self, channel: usize) -> bool {
        self.channels[channel].can_send_message()
    }

    pub(crate) fn has_messages_to_send(&self, channel: usize) -> bool {
        self.channels[channel].has_messages_to_send()
    }

    pub(crate) fn send_message(&mut self, channel_index: usize, message: M) {
        self.channels[channel_index].send_message(message);
    }

    pub(crate) fn receive_message(&mut self, channel_index: usize) -> Option<(u16, M)> {
        self.channels[channel_index].receive_message()
    }
}

struct ConnectionPacket<M> {
    channel_data: Vec<ChannelPacketData<M>>,
}

impl<M: NetworkMessage> ConnectionPacket<M> {
    fn new(channel_data: Vec<ChannelPacketData<M>>) -> ConnectionPacket<M> {
        ConnectionPacket { channel_data }
    }

    fn serialize(&self, config: &ConnectionConfig, dest: &mut [u8]) -> Result<usize, M::Error> {
        assert!(self.channel_data.len() < u16::MAX as usize);

        let mut writer = Cursor::new(dest);
        writer
            .write_u16::<LittleEndian>(self.channel_data.len() as _)
            .unwrap();
        assert!((writer.position() as usize) < CONSERVATIVE_PACKET_HEADER_BITS);

        if self.channel_data.is_empty() {
            return Ok(writer.position() as _);
        }

        for channel_data in &self.channel_data {
            channel_data.serialize(config, &mut writer)?;
        }

        Ok(writer.position() as _)
    }

    unsafe fn deserialize(
        &mut self,
        config: &ConnectionConfig,
        packet_data: *const u8,
        packet_bytes: usize,
    ) -> Result<(), M::Error> {
        /*
           SAFETY: packet_data comes from a netcode_connection_payload_packet_t

           netcode_connection_payload_packet_t is ultimately allocated in three places:
             - read from decrypted buffer
                - in which case all the bytes should be initialized
             - loopback (both server and client send packets)
                - packet_data is initialized if the sent packet is initialized
        */
        assert!(!packet_data.is_null());
        debug_assert!(packet_bytes < isize::MAX as usize);
        let src = slice::from_raw_parts(packet_data, packet_bytes);

        let mut reader = Cursor::new(src);
        let channels = reader.read_u16::<LittleEndian>().unwrap() as usize;

        for _ in 0..channels {
            let data = ChannelPacketData::deserialize(config, &mut reader)?;
            self.channel_data.push(data);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::config::{ChannelType, ClientServerConfig};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestMessage {
        value: u64,
    }

    impl NetworkMessage for TestMessage {
        type Error = std::io::Error;

        fn serialize<W: std::io::Write>(&self, mut writer: W) -> Result<(), Self::Error> {
            writer.write_u64::<LittleEndian>(self.value)?;

            Ok(())
        }

        fn deserialize<R: std::io::Read>(mut reader: R) -> Result<Self, Self::Error> {
            let value = reader.read_u64::<LittleEndian>()?;

            Ok(TestMessage { value })
        }
    }

    #[test]
    fn test_send_receive_unreliable_messages() {
        let mut time = 100.0;
        let delta_time = 0.016;

        let config = ClientServerConfig::new(1);
        let mut config = config.connection;
        let messages_per_packet = 8;
        config.channels[0].max_messages_per_packet = messages_per_packet;
        config.channels[0].kind = ChannelType::UnreliableUnordered;

        let mut sender = Connection::new(config.clone(), time);
        let mut receiver = Connection::new(config.clone(), time);

        let mut sender_sequence = 0;
        let mut receiver_sequence = 0;

        let messages_sent = 1024;
        assert!(messages_sent <= config.channels[0].message_send_queue_size);
        for i in 0..messages_sent {
            let message = TestMessage { value: i as u64 };
            sender.send_message(0, message);
        }

        let expected_iterations = messages_sent / messages_per_packet;
        let mut expect_value = 0;
        for iter in 0..expected_iterations {
            pump_connection_update(
                &config,
                &mut time,
                &mut sender,
                &mut receiver,
                &mut sender_sequence,
                &mut receiver_sequence,
                delta_time,
                0.0,
            );

            loop {
                let Some((_id, message)) = receiver.receive_message(0) else { break };
                assert_eq!(
                    message.value, expect_value,
                    "actual message value {}, expected {}; iter: {}",
                    message.value, expect_value, iter
                );
                expect_value += 1;
            }
        }

        assert_eq!(
            receiver.channel_counters(0).received,
            messages_sent as usize,
            "left==recieved; right==sent; expected iterations: {}",
            expected_iterations
        );
    }

    #[test]
    fn test_send_receive_reliable_messages() {
        let mut time = 100.0;
        let delta_time = 0.016;

        let config = ClientServerConfig::new(1);
        let mut config = config.connection;
        let messages_per_packet = 8;
        config.channels[0].max_messages_per_packet = messages_per_packet;
        config.channels[0].sent_packet_buffer_size = 16; // severely constrain this
        config.channels[0].kind = ChannelType::ReliableOrdered;

        let mut sender = Connection::new(config.clone(), time);
        let mut receiver = Connection::new(config.clone(), time);

        let mut sender_sequence = 0;
        let mut receiver_sequence = 0;

        let messages_sent = 1024;
        assert!(messages_sent <= config.channels[0].message_send_queue_size);
        for i in 0..messages_sent {
            let message = TestMessage { value: i as u64 };
            sender.send_message(0, message);
        }

        let mut expect_value = 0;
        let mut iter = 0;
        let max_iter = 15 * messages_sent / messages_per_packet;
        loop {
            pump_connection_update(
                &config,
                &mut time,
                &mut sender,
                &mut receiver,
                &mut sender_sequence,
                &mut receiver_sequence,
                delta_time,
                0.90,
            );

            loop {
                let Some((_id, message)) = receiver.receive_message(0) else { break };
                assert_eq!(
                    message.value, expect_value,
                    "actual message value {}, expected {}; iter: {}",
                    message.value, expect_value, iter
                );
                expect_value += 1;
            }

            if receiver.channel_counters(0).received >= messages_sent {
                break;
            }

            if iter > max_iter {
                panic!("exceeded maximum iterations allowed: {}", iter);
            }

            iter += 1;
        }

        assert_eq!(
            receiver.channel_counters(0).received,
            messages_sent as usize,
            "left==recieved; right==sent; iterations: {}",
            iter
        );
    }

    #[test]
    fn test_duplex_reliable_messages() {
        let mut time = 100.0;
        let delta_time = 0.016;

        let config = ClientServerConfig::new(1);
        let mut config = config.connection;
        let messages_per_packet = 8;
        config.channels[0].max_messages_per_packet = messages_per_packet;
        config.channels[0].sent_packet_buffer_size = 16; // severely constrain this
        config.channels[0].kind = ChannelType::ReliableOrdered;

        let mut sender = Connection::new(config.clone(), time);
        let mut receiver = Connection::new(config.clone(), time);

        let mut sender_sequence = 0;
        let mut receiver_sequence = 0;

        let messages_sent = 1024;
        assert!(messages_sent <= config.channels[0].message_send_queue_size);
        for i in 0..messages_sent {
            let message = TestMessage { value: i as u64 };
            sender.send_message(0, message);
            receiver.send_message(0, message);
        }

        let mut sender_expect_value = 0;
        let mut receiver_expect_value = 0;
        let mut iter = 0;
        let max_iter = 15 * messages_sent / messages_per_packet;
        loop {
            pump_connection_update(
                &config,
                &mut time,
                &mut sender,
                &mut receiver,
                &mut sender_sequence,
                &mut receiver_sequence,
                delta_time,
                0.90,
            );

            loop {
                let Some((_id, message)) = sender.receive_message(0) else { break };
                assert_eq!(
                    message.value, sender_expect_value,
                    "actual message value {}, expected {}; iter: {}",
                    message.value, sender_expect_value, iter
                );
                sender_expect_value += 1;
            }

            loop {
                let Some((_id, message)) = receiver.receive_message(0) else { break };
                assert_eq!(
                    message.value, receiver_expect_value,
                    "actual message value {}, expected {}; iter: {}",
                    message.value, receiver_expect_value, iter
                );
                receiver_expect_value += 1;
            }

            if receiver.channel_counters(0).received >= messages_sent
                && sender.channel_counters(0).received >= messages_sent
            {
                break;
            }

            if iter > max_iter {
                panic!("exceeded maximum iterations allowed: {}", iter);
            }

            iter += 1;
        }

        assert_eq!(
            receiver.channel_counters(0).received,
            messages_sent as usize,
            "left==recieved; right==sent; iterations: {}",
            iter
        );
        assert_eq!(
            sender.channel_counters(0).received,
            messages_sent as usize,
            "left==recieved; right==sent; iterations: {}",
            iter
        );
    }

    #[test]
    fn test_send_receive_reliable_messages_multiple_channels() {
        let mut time = 100.0;
        let delta_time = 0.016;

        let config = ClientServerConfig::new(2);
        let mut config = config.connection;
        let messages_per_packet = 8;
        for i in 0..2 {
            config.channels[i].max_messages_per_packet = messages_per_packet;
            config.channels[i].sent_packet_buffer_size = 16; // severely constrain this
            config.channels[i].kind = ChannelType::ReliableOrdered;
        }

        let mut sender = Connection::new(config.clone(), time);
        let mut receiver = Connection::new(config.clone(), time);

        let mut sender_sequence = 0;
        let mut receiver_sequence = 0;

        let channel_0_messages = 1024;
        let channel_1_messages = 400;

        for i in 0..channel_0_messages {
            let message = TestMessage { value: i as u64 };
            sender.send_message(0, message);
        }
        for i in 0..channel_1_messages {
            let message = TestMessage {
                value: 3 * i as u64,
            };
            sender.send_message(1, message);
        }

        let mut iter = 0;
        let max_iter = 20 * (channel_0_messages + channel_1_messages) / (2 * messages_per_packet);
        loop {
            pump_connection_update(
                &config,
                &mut time,
                &mut sender,
                &mut receiver,
                &mut sender_sequence,
                &mut receiver_sequence,
                delta_time,
                0.90,
            );

            loop {
                let Some(_) = receiver.receive_message(0) else { break };
            }

            loop {
                let Some(_) = receiver.receive_message(1) else { break };
            }

            if receiver.channel_counters(0).received >= channel_0_messages
                && receiver.channel_counters(1).received >= channel_1_messages
            {
                break;
            }

            if iter > max_iter {
                panic!("exceeded maximum iterations allowed: {}", iter);
            }

            iter += 1;
        }

        assert_eq!(
            receiver.channel_counters(0).received,
            channel_0_messages,
            "left==recieved; right==sent; iterations: {}",
            iter
        );
        assert_eq!(
            receiver.channel_counters(1).received,
            channel_1_messages,
            "left==recieved; right==sent; iterations: {}",
            iter
        );
    }

    fn pump_connection_update(
        config: &ConnectionConfig,
        time: &mut f64,
        sender: &mut Connection<TestMessage>,
        receiver: &mut Connection<TestMessage>,
        sender_sequence: &mut u16,
        receiver_sequence: &mut u16,
        delta_time: f64,
        packet_loss: f32,
    ) {
        let mut packet = vec![0u8; config.max_packet_size];

        let mut bytes_written = sender.generate_packet(*sender_sequence, &mut packet[..]);
        if bytes_written > 0 {
            if rand::random::<f32>() > packet_loss {
                unsafe {
                    receiver.process_packet(*sender_sequence, packet.as_ptr(), bytes_written);
                    sender.process_acks(sender_sequence, 1);
                }
            }
        }

        bytes_written = receiver.generate_packet(*receiver_sequence, &mut packet[..]);
        if bytes_written > 0 {
            if rand::random::<f32>() > packet_loss {
                unsafe {
                    sender.process_packet(*receiver_sequence, packet.as_ptr(), bytes_written);
                    receiver.process_acks(receiver_sequence, 1);
                }
            }
        }

        *time += delta_time;

        sender.advance_time(*time);
        receiver.advance_time(*time);

        *sender_sequence = sender_sequence.wrapping_add(1);
        *receiver_sequence = receiver_sequence.wrapping_add(1);

        assert!(sender.error_level() == ConnectionErrorLevel::None);
        assert!(receiver.error_level() == ConnectionErrorLevel::None);
    }
}
