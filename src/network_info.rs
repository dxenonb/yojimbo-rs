pub struct NetworkInfo {
    /// Round trip time estimate (milliseconds).
    rtt: f32,
    /// Packet loss percent.
    packet_loss: f32,
    /// Sent bandwidth (kbps).
    sent_bandwidth: f32,
    /// Received bandwidth (kbps).
    received_bandwidth: f32,
    /// Acked bandwidth (kbps).
    acked_bandwidth: f32,
    /// Number of packets sent.
    num_packets_sent: u64,
    /// Number of packets received.
    num_packets_received: u64,
    /// Number of packets acked.
    num_packets_acked: u64,
}
