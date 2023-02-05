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
    // error_level: ConnectionErrorLevel,
}

impl Connection {
    pub(crate) fn new() -> Connection {
        Connection {}
    }
}
