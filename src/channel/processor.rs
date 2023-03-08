use crate::config::ChannelConfig;

use super::channel_packet_data::ChannelPacketData;

pub(crate) trait Processor<M> {
    fn advance_time(&mut self, new_time: f64);
    fn reset(&mut self);
    fn can_send_message(&self) -> bool;
    fn has_messages_to_send(&self) -> bool;
    fn send_message(&mut self, message: M);
    fn receive_message(&mut self) -> Option<M>;
    fn packet_data(
        &mut self,
        config: &ChannelConfig,
        channel_index: usize,
        packet_sequence: u16,
        available_bits: usize,
    ) -> (ChannelPacketData<M>, usize);
    fn process_packet_data(&mut self, packet_data: ChannelPacketData<M>, packet_sequence: u16);

    // process_ack(&mut self, ack: u16);
}
