use std::collections::VecDeque;

use rand::Rng;

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

/// NetworkSimulator to simulate latency and jitter.
///
/// Differences from original yojimbo:
///
///  - percents for packet loss and duplicates are expressed as [0, 1] instead
///    of [0, 100]; both ranges are entirely reasonable choices but we can
///    assert percents are in [0, 1], whereas we don't know what the user means
///    if they enter 0.9; did they mean 0.9% of packets are lost? Hopefully
///    this saves somebody some headache.
pub struct NetworkSimulator {
    latency: f64,
    jitter: f64,
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
    pub fn set_latency(&mut self, milliseconds: f64) {
        self.latency = milliseconds;
        self.update_active();
    }

    /// Set the packet jitter in milliseconds.
    ///
    /// Jitter is applied +/- this amount in milliseconds. To be truly
    /// effective, jitter must be applied together with some latency.
    pub fn set_jitter(&mut self, milliseconds: f64) {
        self.jitter = milliseconds;
        self.update_active();
    }

    /// Set the amount of packet loss to apply on send, as a percent [0, 1].
    ///
    /// 0% = no packet loss, 100% = all packets are dropped.
    pub fn set_packet_loss(&mut self, percent: f32) {
        assert!(percent >= 0.0 && percent <= 1.0);
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
        assert!(percent >= 0.0 && percent <= 1.0);
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
        let previous = self.active;
        self.active = self.latency != 0.0
            || self.jitter != 0.0
            || self.packet_loss != 0.0
            || self.duplicates != 0.0;
        if previous && !self.active {
            self.entries.clear();
        }
    }

    pub(crate) fn advance_time(&mut self, time: f64) {
        self.time = time;

        self.entries.retain(|entry| !entry.consumed);
    }

    /// Queue a packet to send to a given client.
    ///
    /// If you are calling this from the client, pass anything for
    /// `client_index` (well, 0 is a good choice) - it doesn't matter,
    /// and just ignore the client_index on `receive_packets`.
    pub(crate) fn send_packet(&mut self, client_index: usize, packet_data: &[u8]) {
        let mut rng = rand::thread_rng();

        if rng.gen::<f32>() < self.packet_loss {
            return;
        }

        let mut delay = self.latency / 1000.0;
        if self.jitter > 0.0 {
            delay += rng.gen_range(-self.jitter..=self.jitter) / 1000.0;
        }

        let entry = PacketEntry {
            destination_client_index: client_index,
            delievery_time: self.time + delay,
            packet_data: Vec::from(packet_data),
            consumed: false,
        };
        self.push_packet(entry);
        if rng.gen::<f32>() < self.duplicates {
            let mut entry = self.entries.back().unwrap().clone();
            entry.delievery_time = self.time + delay + rng.gen::<f64>();
            self.push_packet(entry);
        }
    }

    /// Helper function to use the VecDeque as a circular buffer.
    fn push_packet(&mut self, entry: PacketEntry) {
        if self.entries.len() == self.entries.capacity() {
            // drop the oldest packet if we're at capacity
            let _ = self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Recieve all the packets currently available.
    ///
    /// Returns an iterator over (client_index, packet_data) for each packet.
    pub(crate) fn receive_packets(&mut self) -> impl Iterator<Item = (usize, &[u8])> {
        assert!(self.active, "check network simulator is active before calling receive packets, this is for your own good");

        let time = self.time;
        self.entries.iter_mut().filter_map(move |entry| {
            assert!(!entry.consumed, "consumed packet found on receive; did you forget to call advance_time on the network simulator?");

            if entry.delievery_time < time {
                entry.consumed = true;
                return Some((entry.destination_client_index, &entry.packet_data[..]));
            } else {
                None
            }
        })
    }

    /// Discard all packets in the network simulator.
    ///
    /// This is useful if the simulator needs to be reset and used for another purpose.
    pub(crate) fn discard_packets(&mut self) {
        self.entries.clear();
    }

    /// Discard packets sent to a particular client index.
    ///
    /// This is called when a client disconnects from the server.
    pub(crate) fn discard_client_packets(&mut self, client_index: usize) {
        self.entries
            .retain(|entry| entry.destination_client_index != client_index);
    }
}

#[derive(Debug, Clone)]
struct PacketEntry {
    destination_client_index: usize,
    delievery_time: f64,
    packet_data: Vec<u8>,
    /// True if this packet has been received.
    consumed: bool,
}

#[cfg(test)]
mod test {
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

    #[test]
    #[should_panic]
    fn duplicates_on_range_0_1() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_duplicates(50.0);
    }

    #[test]
    #[should_panic]
    fn packet_loss_on_range_0_1() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_packet_loss(50.0);
    }

    #[test]
    fn does_not_exceed_capacity() {
        let capacity = 100;
        let mut n = NetworkSimulator::new(capacity, 100.0);
        n.set_latency(16.0);

        for _ in 0..(2 * capacity) {
            n.send_packet(0, &[0; 8]);
        }

        n.advance_time(n.time + 1.0);
        let count = n.receive_packets().count();
        assert_eq!(count, capacity);
        assert_eq!(n.entries.capacity(), capacity);
    }

    #[test]
    fn discards_packets_on_inactive() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_latency(16.0);

        let sent = 50;
        for _ in 0..sent {
            n.send_packet(0, &[0; 8]);
        }

        n.advance_time(n.time + 1.0);
        assert_eq!(n.receive_packets().count(), sent);

        n.set_latency(0.0);
        assert!(!n.active());

        assert_eq!(n.entries.len(), 0);
    }

    #[test]
    fn drops_packets() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_latency(16.0);
        check_send_recieve(&mut n, 1.0, 50, 50);

        n.set_packet_loss(1.0);
        check_send_recieve(&mut n, 1.0, 50, 0);
    }

    #[test]
    fn duplicates_packets() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_latency(16.0);
        check_send_recieve(&mut n, 1.0, 50, 50);

        n.set_duplicates(1.0);
        check_send_recieve(&mut n, 4.0, 50, 100);

        // check that duplicates don't extend the buffer
        check_send_recieve(&mut n, 4.0, 75, 100);
    }

    #[test]
    fn adds_latency_to_packets() {
        let mut n = NetworkSimulator::new(100, 100.0);
        n.set_latency(16.0);
        check_send_recieve(&mut n, 1.0, 50, 50);

        // check that 1500ms latency packets are not recieved within 1s
        n.set_latency(1500.0);
        check_send_recieve(&mut n, 1.0, 50, 0);

        // check that they are all recieved within the next 1s
        check_send_recieve(&mut n, 1.0, 0, 50);
    }

    fn check_send_recieve(n: &mut NetworkSimulator, dt: f64, send: usize, expect_received: usize) {
        for _ in 0..send {
            n.send_packet(0, &[0; 8]);
        }
        n.advance_time(n.time + dt);
        assert_eq!(n.receive_packets().count(), expect_received);
        n.advance_time(n.time); // remove the consumed entries
    }
}
