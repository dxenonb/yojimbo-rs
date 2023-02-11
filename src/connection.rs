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
        assert!(config.channels.len() >= 1);

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
    pub(crate) fn generate_packet(
        &mut self,
        packet_sequence: u16,
        packet_data: &mut [u8],
    ) -> usize {
        if self.channels.len() == 0 {
            return 0;
        }

        // REFACTOR: consider caching
        let mut channel_data = Vec::new();

        assert!(packet_data.len() > 0);
        let mut available_bits = packet_data.len() * 8 - CONSERVATIVE_PACKET_HEADER_BITS;

        for channel in &mut self.channels {
            let (packet_data, packet_data_bits) =
                channel.packet_data(packet_sequence, available_bits);
            if packet_data_bits > 0 {
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

    pub(crate) fn receive_message(&mut self, channel_index: usize) -> Option<M> {
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
