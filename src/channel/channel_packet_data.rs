use std::io::{self, Cursor};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    config::{ChannelConfig, ConnectionConfig},
    message::NetworkMessage,
};

/// Contains a series of messages sent on `channel_index`.
///
/// Defines how the channel index is serialized to packets.
pub(crate) struct ChannelPacketData<M> {
    pub(crate) channel_index: usize,
    pub(crate) messages: Vec<M>,
}

impl<M: NetworkMessage> ChannelPacketData<M> {
    pub(crate) fn serialize(
        &self,
        config: &ConnectionConfig,
        dest: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        dest.write_u16::<LittleEndian>(self.channel_index.try_into().unwrap())
            .unwrap();

        // TODO: block messages

        // TODO: serialize reliable messages

        let config = &config.channels[self.channel_index];
        self.serialize_unordered(config, dest)?;
        Ok(())
    }

    pub(crate) fn deserialize(
        config: &ConnectionConfig,
        src: &mut Cursor<&[u8]>,
    ) -> Result<ChannelPacketData<M>, M::Error> {
        let channel_index = src.read_u16::<LittleEndian>().unwrap() as _;

        // TODO: block messages

        // TODO: deserialize reliable messages

        let config = &config.channels[channel_index];
        let messages = ChannelPacketData::deserialize_unordered(config, src).unwrap();

        Ok(ChannelPacketData {
            channel_index,
            messages,
        })
    }

    pub(crate) fn serialize_unordered(
        &self,
        config: &ChannelConfig,
        mut writer: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        let has_messages = self.messages.len() > 0;

        writer.write_u8(if has_messages { 1 } else { 0 }).unwrap();

        if !has_messages {
            return Ok(());
        }

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize,);
        writer
            .write_u8((self.messages.len() - 1).try_into().unwrap())
            .unwrap();

        for message in &self.messages {
            message.serialize(&mut writer)?;

            #[cfg(feature = "serialize_check")]
            {
                writer
                    .write_u32::<LittleEndian>(SERIALIZE_CHECK_VALUE)
                    .expect("failed to write check value");
            }
        }

        Ok(())
    }

    pub(crate) fn deserialize_unordered(
        config: &ChannelConfig,
        mut reader: &mut Cursor<&[u8]>,
    ) -> Result<Vec<M>, M::Error> {
        let has_messages = reader.read_u8().unwrap() == 1;

        if !has_messages {
            return Ok(Vec::new());
        }

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize);
        let message_count = 1 + reader.read_u8().unwrap() as usize;
        let mut messages = Vec::with_capacity(message_count);

        for _ in 0..message_count {
            messages.push(M::deserialize(&mut reader)?);

            #[cfg(feature = "serialize_check")]
            {
                let check_value = reader
                    .read_u32::<LittleEndian>()
                    .expect("expected check value, found end of stream");
                assert_eq!(
                    check_value, SERIALIZE_CHECK_VALUE,
                    "expected check value {} but found {}",
                    SERIALIZE_CHECK_VALUE, check_value
                );
            }
        }

        Ok(messages)
    }

    pub(crate) fn empty() -> ChannelPacketData<M> {
        ChannelPacketData {
            channel_index: usize::MAX,
            messages: Vec::new(),
        }
    }
}

/// A writer just like std::io::Sink but it measures like yojimbo's measure stream.
pub(crate) struct MeasureSink {
    pub(crate) bytes: usize,
}

impl MeasureSink {
    pub(crate) fn new() -> MeasureSink {
        MeasureSink { bytes: 0 }
    }
}

impl io::Write for MeasureSink {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes += buf.len();
        Ok(buf.len())
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let total_len = bufs.iter().map(|b| b.len()).sum();
        self.bytes += total_len;
        Ok(total_len)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
