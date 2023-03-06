#[derive(Debug, Clone)]
pub struct NetworkInfo {
    /// Round trip time estimate (milliseconds).
    pub rtt: f32,
    /// Packet loss percent.
    pub packet_loss: f32,
    /// Sent bandwidth (kbps).
    pub sent_bandwidth: f32,
    /// Received bandwidth (kbps).
    pub received_bandwidth: f32,
    /// Acked bandwidth (kbps).
    pub acked_bandwidth: f32,
    /// Number of packets sent.
    pub num_packets_sent: u64,
    /// Number of packets received.
    pub num_packets_received: u64,
    /// Number of packets acked.
    pub num_packets_acked: u64,
}
