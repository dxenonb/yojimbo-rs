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

pub struct Connection {
    // message_factory: MessageFactory,
    // connection_config: ConnectionConfig,
    // channel: Vec<Channel>,
    error_level: ConnectionErrorLevel,
}

impl Connection {
    pub(crate) fn new() -> Connection {
        // TODO
        Connection {
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
        packet_sequence: u16,
        packet_data: *const u8,
        packet_bytes: i32,
    ) -> bool {
        if self.error_level() != ConnectionErrorLevel::None {
            log::debug!("failed to read packet because connection is in error state");
            return false;
        }

        // TODO

        true
    }
}
