use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct NetworkSimulatorConfig {
    /// Maximum number of packets that can be stored in the network simulator.
    /// Additional packets are dropped.
    pub max_simulator_packets: usize,
}

impl Default for NetworkSimulatorConfig {
    fn default() -> Self {
        NetworkSimulatorConfig {
            max_simulator_packets: 4 * 1024,
        }
    }
}

pub struct NetworkSimulator {
    latency: f32,
    jitter: f32,
    packet_loss: f32,
    duplicates: f32,
    active: bool,
    time: f64,
    entries: VecDeque<PacketEntry>,
}

impl NetworkSimulator {
    /// Create an inactive NetworkSimulator, which can store up to `max_packets`.
    ///
    /// If `max_packets` is 0, this does not allocate.
    pub(crate) fn new(max_packets: usize, time: f64) -> NetworkSimulator {
        NetworkSimulator {
            entries: VecDeque::with_capacity(max_packets),
            time,
            latency: 0.0,
            jitter: 0.0,
            packet_loss: 0.0,
            duplicates: 0.0,
            active: false,
        }
    }

    /// Set the latency in milliseconds.
    ///
    /// This latency is added on packet send. To simulate a round trip time of
    /// 100ms, add 50ms of latency to both sides of the connection.
    pub fn set_latency(&mut self, milliseconds: f32) {
        self.latency = milliseconds;
        self.update_active();
    }

    /// Set the packet jitter in milliseconds.
    ///
    /// Jitter is applied +/- this amount in milliseconds. To be truly
    /// effective, jitter must be applied together with some latency.
    pub fn set_jitter(&mut self, milliseconds: f32) {
        self.jitter = milliseconds;
        self.update_active();
    }

    /// Set the amount of packet loss to apply on send, as a percent.
    ///
    /// 0% = no packet loss, 100% = all packets are dropped.
    pub fn set_packet_loss(&mut self, percent: f32) {
        self.packet_loss = percent;
        self.update_active();
    }

    /// Set percentage chance of packet duplicates.
    ///
    /// If the duplicate chance succeeds, a duplicate packet is added to the
    /// queue with a random delay of up to 1 second.
    ///
    /// 0% = no duplicate packets, 100% = all packets have a duplicate sent.
    pub fn set_duplicates(&mut self, percent: f32) {
        self.duplicates = percent;
        self.update_active();
    }

    /// Returns true if the network simulator is active, false otherwise.
    pub fn active(&self) -> bool {
        self.active
    }

    /// Call this after the `set_{property}` method to update the `active` field.
    ///
    /// Minor optimization so that we're not checking each field each
    /// send/recieve in the client and server.
    fn update_active(&mut self) {
        self.active = self.latency != 0.0
            || self.jitter != 0.0
            || self.packet_loss != 0.0
            || self.duplicates != 0.0;
    }

    pub(crate) fn advance_time(&mut self, time: f64) {
        self.time = time;
    }

    /// Queue a packet to send to a given client.
    pub(crate) fn send_packet(&mut self, client_index: usize, packet_data: &[u8]) {
        panic!();
    }

    /// TODO
    pub(crate) fn receive_packets(&mut self) {
        panic!();
    }

    /// Discard all packets in the network simulator.
    ///
    /// This is useful if the simulator needs to be reset and used for another purpose.
    pub(crate) fn discard_packets(&mut self) {
        panic!()
    }

    /// Discard packets sent to a particular client index.
    ///
    /// This is called when a client disconnects from the server.
    pub(crate) fn discard_client_packets(&mut self, client_index: usize) {
        panic!()
    }
}

struct PacketEntry {
    destination_client_index: usize,
    delievery_time: f64,
    packet_data: Vec<u8>,
}

#[cfg(test)]
mod test {
    use rand::Fill;

    use super::NetworkSimulator;

    #[test]
    fn sets_active() {
        let mut n;
        n = NetworkSimulator::new(100, 100.0);
        assert!(!n.active());

        n.set_latency(0.0);
        n.set_jitter(0.0);
        n.set_packet_loss(0.0);
        n.set_duplicates(0.0);

        assert!(!n.active());

        n = NetworkSimulator::new(100, 100.0);
        assert!(!n.active());
        n.set_latency(32.0);
        assert!(n.active());

        n = NetworkSimulator::new(100, 100.0);
        assert!(!n.active());
        n.set_jitter(7.0);
        assert!(n.active());

        n = NetworkSimulator::new(100, 100.0);
        assert!(!n.active());
        n.set_packet_loss(0.5);
        assert!(n.active());

        n = NetworkSimulator::new(100, 100.0);
        assert!(!n.active());
        n.set_duplicates(0.5);
        assert!(n.active());
    }

    // #[test]
    // fn discards_packets_on_inactive() {
    //     let mut n = NetworkSimulator::new(100, 100.0);
    //     let mut buffer = vec![0u8; 1024];
    //     buffer.try_fill(&mut rand::thread_rng()).unwrap();

    //     n.send_packet(0, &buffer);

    //     assert!(n.receive_packets().is_none());
    // }
}
