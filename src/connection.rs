use byteorder::{LittleEndian, WriteBytesExt};

use crate::{
    channel::{
        Channel, ChannelErrorLevel, ChannelPacketData, CONSERVATIVE_CHANNEL_HEADER_BITS,
        CONSERVATIVE_PACKET_HEADER_BITS,
    },
    config::ConnectionConfig,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionErrorLevel {
    /// No error. All is well.
    None,
    /// A channel is in an error state.
    Channel,
    // /// The allocator is an error state.
    // Allocator,
    // /// The message factory is in an error state.
    // MessageFactory,
    /// Failed to read packet. Received an invalid packet?     
    ReadPacketFailed,
}

/// Sends and receives messages across a set of user defined channels.
pub(crate) struct Connection<M> {
    // message_factory: MessageFactory,
    // connection_config: ConnectionConfig,
    channels: Vec<Channel<M>>,
    error_level: ConnectionErrorLevel,
}

impl<M> Connection<M> {
    pub(crate) fn new(config: &ConnectionConfig, time: f64) -> Connection<M> {
        assert!(config.num_channels >= 1);

        let mut channels = Vec::with_capacity(config.num_channels);
        for (channel_index, channel_config) in config.channels.iter().enumerate() {
            channels.push(Channel::new(channel_config.clone(), channel_index, time));
        }

        Connection {
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
        // TODO
    }

    pub(crate) unsafe fn process_packet(
        &mut self,
        _packet_sequence: u16,
        _packet_data: *const u8,
        _packet_bytes: i32,
    ) -> bool {
        if self.error_level() != ConnectionErrorLevel::None {
            log::debug!("failed to read packet because connection is in error state");
            return false;
        }

        // TODO

        true
    }

    /// Generate a packet.
    ///
    /// Advances `packet_data` as it writes.
    pub(crate) fn generate_packet(&mut self, packet_sequence: u16, packet_data: &mut &mut [u8]) {
        if self.channels.len() == 0 {
            return;
        }

        // TODO: cache
        let mut channel_data = Vec::new();

        let max_packet_size = packet_data.len();
        let mut available_bits = max_packet_size * 8 - CONSERVATIVE_PACKET_HEADER_BITS;

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
            packet.serialize(packet_data);
            // TODO: serialize check
        }
    }

    pub(crate) fn reset(&mut self) {
        self.error_level = ConnectionErrorLevel::None;
        for channel in &mut self.channels {
            channel.reset();
        }
    }
}

struct ConnectionPacket<M> {
    channel_data: Vec<ChannelPacketData<M>>,
}

impl<M> ConnectionPacket<M> {
    fn new(channel_data: Vec<ChannelPacketData<M>>) -> ConnectionPacket<M> {
        ConnectionPacket { channel_data }
    }

    fn serialize(&self, dest: &mut &mut [u8]) {
        assert!(self.channel_data.len() < u16::MAX as usize);

        let max_packet_size = dest.len();
        dest.write_u16::<LittleEndian>(self.channel_data.len() as _)
            .unwrap();
        assert!(max_packet_size - dest.len() < CONSERVATIVE_PACKET_HEADER_BITS);

        if self.channel_data.is_empty() {
            return;
        }

        for channel_data in &self.channel_data {
            channel_data.serialize(dest);
        }
    }
}
