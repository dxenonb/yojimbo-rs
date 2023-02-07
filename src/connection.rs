use crate::config::{ChannelConfig, ConnectionConfig};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionErrorLevel {
    /// No error. All is well.
    None,
    /// A channel is in an error state.
    Channel,
    /// The allocator is an error state.
    Allocator,
    /// The message factory is in an error state.
    MessageFactory,
    /// Failed to read packet. Received an invalid packet?     
    ReadPacketFailed,
}

/// Sends and receives messages across a set of user defined channels.
pub(crate) struct Connection {
    // message_factory: MessageFactory,
    // connection_config: ConnectionConfig,
    channels: Vec<Channel>,
    error_level: ConnectionErrorLevel,
}

impl Connection {
    pub(crate) fn new(config: &ConnectionConfig) -> Connection {
        assert!(config.num_channels >= 1);

        let mut channels = Vec::with_capacity(config.num_channels);
        for channel_config in &config.channels {
            channels.push(Channel::new(channel_config));
        }

        Connection {
            channels,
            error_level: ConnectionErrorLevel::None,
        }
    }

    pub(crate) fn advance_time(&mut self, new_time: f64) {
        // TODO
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
    /// Returns None if no packet is generated. Else returns Some(bytes). Bytes is i32 because C is dumb (netcode expect an i32).
    pub(crate) unsafe fn generate_packet(
        &mut self,
        _packet_sequence: u16,
        _packet_data: *const u8,
        _max_packet_size: usize,
    ) -> Option<i32> {
        // TODO
        None
    }

    pub(crate) fn reset(&mut self) {
        self.error_level = ConnectionErrorLevel::None;
        for channel in &mut self.channels {
            channel.reset();
        }
    }
}

struct Channel;

impl Channel {
    fn new(config: &ChannelConfig) -> Channel {
        // TODO
        Channel
    }

    fn reset(&mut self) {
        // TODO
    }
}
