use std::io::{self, Cursor};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    config::{ChannelType, ConnectionConfig},
    message::NetworkMessage,
};

#[cfg(feature = "serialize_check")]
use super::SERIALIZE_CHECK_VALUE;

/// Contains a series of messages sent on `channel_index`.
///
/// Defines how the channel index is serialized to packets.
pub(crate) struct ChannelPacketData<M> {
    pub(crate) channel_index: usize,
    /// List of `(message_id, and message)`
    ///
    /// `message_id` for unreliable channels is simply the packet sequence
    /// number the message was sent in. For reliable channels, `message_id`
    /// increments for each message sent on that channel.
    ///
    /// On deserialize, for unreliable channels `message_id` will be set to 0;
    /// `Processor::process_packet_data` is responsible for setting the correct ID.
    ///
    /// On deserialize, for reliable channels `message_id` is decoded from the
    /// stream.
    ///
    /// Bear in mind that the message ID will wrap at the bounds of u16.
    pub(crate) messages: Vec<(u16, M)>,
}

impl<M: NetworkMessage> ChannelPacketData<M> {
    pub(crate) fn serialize(
        &self,
        config: &ConnectionConfig,
        dest: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        dest.write_u16::<LittleEndian>(self.channel_index.try_into().unwrap())
            .unwrap();
        let config = &config.channels[self.channel_index];

        // TODO: block messages

        let has_messages = !self.messages.is_empty();

        dest.write_u8(if has_messages { 1 } else { 0 }).unwrap();

        if !has_messages {
            return Ok(());
        }

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize,);
        assert!(self.messages.len() <= config.max_messages_per_packet);
        dest.write_u8((self.messages.len() - 1).try_into().unwrap())
            .unwrap();

        match config.kind {
            ChannelType::UnreliableUnordered => self.serialize_unordered(dest)?,
            ChannelType::ReliableOrdered => self.serialize_ordered(dest)?,
        }

        Ok(())
    }

    pub(crate) fn deserialize(
        config: &ConnectionConfig,
        src: &mut Cursor<&[u8]>,
    ) -> Result<ChannelPacketData<M>, M::Error> {
        let channel_index = src.read_u16::<LittleEndian>().unwrap() as usize;
        let config = &config.channels[channel_index];

        // TODO: block messages

        let has_messages = src.read_u8().unwrap() == 1;

        if !has_messages {
            return Ok(ChannelPacketData::empty());
        }

        let message_count = 1 + src.read_u8().unwrap() as usize;

        debug_assert!(config.max_messages_per_packet - 1 <= u8::MAX as usize);
        assert!(message_count <= config.max_messages_per_packet);

        let mut messages = Vec::with_capacity(message_count);

        match config.kind {
            ChannelType::UnreliableUnordered => {
                ChannelPacketData::deserialize_unordered(src, message_count, &mut messages)?
            }
            ChannelType::ReliableOrdered => {
                ChannelPacketData::deserialize_ordered(src, message_count, &mut messages)?
            }
        }

        Ok(ChannelPacketData {
            channel_index,
            messages,
        })
    }

    pub(crate) fn serialize_unordered(
        &self,
        mut writer: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        for (_, message) in &self.messages {
            message.serialize(&mut writer)?;

            Self::serialize_check(writer);
        }

        Ok(())
    }

    pub(crate) fn deserialize_unordered(
        mut reader: &mut Cursor<&[u8]>,
        message_count: usize,
        messages: &mut Vec<(u16, M)>,
    ) -> Result<(), M::Error> {
        for _ in 0..message_count {
            // the ID is actually decided in `Processor::process_packet_data` - set 0 for now
            messages.push((0, M::deserialize(&mut reader)?));

            Self::deserialize_check(reader);
        }

        Ok(())
    }

    pub(crate) fn serialize_ordered(
        &self,
        mut writer: &mut Cursor<&mut [u8]>,
    ) -> Result<(), M::Error> {
        /*
           this order (IDs list followed by messages list) is taken from
           yojimbo (which serializes IDs relative to previous ID for
           compression)
        */

        // write the message IDs
        for (id, _) in &self.messages {
            // TODO: serialize sequence relative
            writer.write_u16::<LittleEndian>(*id).unwrap();
        }

        Self::serialize_check(writer);

        // write the message contents
        for (_, message) in &self.messages {
            message.serialize(&mut writer)?;

            Self::serialize_check(writer);
        }

        Ok(())
    }

    pub(crate) fn deserialize_ordered(
        mut reader: &mut Cursor<&[u8]>,
        message_count: usize,
        messages: &mut Vec<(u16, M)>,
    ) -> Result<(), M::Error> {
        // read the message IDs
        let mut message_ids = Vec::with_capacity(message_count);
        for _ in 0..message_count {
            let id = reader.read_u16::<LittleEndian>().unwrap();
            message_ids.push(id);
        }

        Self::deserialize_check(reader);

        // read the messages
        let expect_length = message_ids.len();
        for id in message_ids {
            let message = M::deserialize(&mut reader)?;
            messages.push((id, message));

            Self::deserialize_check(reader);
        }
        assert_eq!(messages.len(), expect_length);

        Ok(())
    }

    #[inline]
    fn deserialize_check(_reader: &mut Cursor<&[u8]>) {
        #[cfg(feature = "serialize_check")]
        {
            let check_value = _reader
                .read_u32::<LittleEndian>()
                .expect("expected check value, found end of stream");
            assert_eq!(
                check_value, SERIALIZE_CHECK_VALUE,
                "expected check value {} but found {}",
                SERIALIZE_CHECK_VALUE, check_value
            );
        }
    }

    #[inline]
    fn serialize_check(_writer: &mut Cursor<&mut [u8]>) {
        #[cfg(feature = "serialize_check")]
        {
            _writer
                .write_u32::<LittleEndian>(SERIALIZE_CHECK_VALUE)
                .expect("failed to write check value");
        }
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
