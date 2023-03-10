/// Data structure that stores data indexed by sequence number.
///
/// Entries may or may not exist. If they don't exist, the sequence value for
/// the entry at that index is set to None.
///
/// This is incredibly useful and is used as the foundation of the packet level
/// ack system and the reliable message send and receive queues.
///
/// Ported from Yojimbo.
pub(crate) struct SequenceBuffer<T> {
    /// The most requence sequence number added to the buffer.
    sequence: u16,
    /// The sequence number corresponding to each entry in `entries` at the same index. None if there is no entry.
    entry_sequence: Vec<Option<u16>>,
    /// The sequence buffer entries. Seperate from `entry_sequence` for fast lookup, when size_of::<T>() is relatively large.
    entries: Vec<Option<T>>,
}

impl<T> SequenceBuffer<T> {
    pub(crate) fn new(size: usize) -> SequenceBuffer<T> {
        let mut entries = Vec::with_capacity(size);
        for _ in 0..size {
            entries.push(None);
        }

        assert!(size <= u16::MAX as usize);

        SequenceBuffer {
            sequence: 0,
            entry_sequence: vec![None; size],
            entries,
        }
    }

    /// Reset the sequence buffer.
    ///
    /// Removes all entries from the sequence buffer and restores it to initial state.
    pub(crate) fn reset(&mut self) {
        self.sequence = 0;
        for entry_sequence in &mut self.entry_sequence {
            *entry_sequence = None;
        }
        // no need to reset the actua entries
    }

    /// The "current" sequence number, which advances when entries are added.
    ///
    /// This sequence number can wrap around, so if you are at 65535 and add
    /// an entry sequence 65535, then 0 becomes the new "current" sequence number.
    ///
    /// See `sequence_greater_than` and `sequence_less_than`.
    pub(crate) fn sequence_pointer(&self) -> u16 {
        self.sequence
    }

    /// Insert an entry into the sequence buffer.
    ///
    /// IMPORTANT: If another entry exists at `sequence` % buffer size,
    /// it is overwritten.
    ///
    /// Returns true if the insert was successful, or false if the entry could
    /// not be added. This happens when the sequence number is too old.
    pub(crate) fn insert_with<F: FnOnce() -> T>(&mut self, sequence: u16, f: F) -> bool {
        let next_sequence = sequence.wrapping_add(1);
        if sequence_greater_than(next_sequence, self.sequence) {
            self.remove_entries(self.sequence, sequence);
            self.sequence = next_sequence;
        } else if sequence_less_than(sequence, self.sequence.wrapping_sub(self.capacity() as u16)) {
            return false;
        }
        let index = self.sequence_index(sequence);
        self.entry_sequence[index] = Some(sequence);
        self.entries[index] = Some(f());
        true
    }

    /// Take an entry from the buffer with matching `sequence`.
    ///
    /// Returns None if the sequence entry does not exist, else returns
    /// `Some(entry)`, and removes it from the buffer.
    pub(crate) fn take(&mut self, sequence: u16) -> Option<T> {
        if self.exists(sequence) {
            let index = self.sequence_index(sequence);
            self.entry_sequence[index] = None;
            self.entries[index].take()
        } else {
            None
        }
    }

    pub(crate) fn get(&self, sequence: u16) -> Option<&T> {
        if self.exists(sequence) {
            self.entries[self.sequence_index(sequence)].as_ref()
        } else {
            None
        }
    }

    pub(crate) fn get_mut(&mut self, sequence: u16) -> Option<&mut T> {
        if self.exists(sequence) {
            let index = self.sequence_index(sequence);
            self.entries[index].as_mut()
        } else {
            None
        }
    }

    /// Returns true if the sequence buffer entry is available, false if it is occupied.
    pub(crate) fn available(&self, sequence: u16) -> bool {
        self.entry_sequence[self.sequence_index(sequence)].is_none()
    }

    pub(crate) fn exists(&self, sequence: u16) -> bool {
        self.entry_sequence[self.sequence_index(sequence)] == Some(sequence)
    }

    pub(crate) fn sequence_index(&self, sequence: u16) -> usize {
        sequence as usize % self.capacity()
    }

    pub(crate) fn capacity(&self) -> usize {
        debug_assert_eq!(self.entries.len(), self.entry_sequence.len());
        debug_assert_eq!(self.entries.len(), self.entries.capacity());
        self.entries.len()
    }

    /// Remove entries between start_sequence and end_sequence
    ///
    /// Note from yojimbo:
    ///
    /// > This is used to remove old entries as we advance the sequence buffer forward.
    /// > Otherwise, if when entries are added with holes (eg. receive buffer for packets or messages, where not all sequence numbers are added to the buffer because we have high packet loss),
    /// > and we are extremely unlucky, we can have old sequence buffer entries from the previous sequence # wrap around still in the buffer, which corrupts our internal connection state.
    /// > This actually happened in the soak test at high packet loss levels (>90%). It took me days to track it down :)
    fn remove_entries(&mut self, start_sequence: u16, end_sequence: u16) {
        let start_sequence = start_sequence as usize;
        let mut end_sequence = end_sequence as usize;
        if end_sequence < start_sequence {
            end_sequence += u16::MAX as usize;
        }
        debug_assert!(end_sequence >= start_sequence);
        if end_sequence - start_sequence < self.capacity() {
            for sequence in start_sequence..=end_sequence {
                let index = sequence % self.capacity();
                self.entry_sequence[index] = None;
            }
        } else {
            for entry in &mut self.entry_sequence {
                *entry = None;
            }
        }
    }
}

/// Compares two 16 bit sequence numbers and returns true if the first one is greater than the second (considering wrapping).
/// IMPORTANT: This is not the same as s1 > s2!
/// Greater than is defined specially to handle wrapping sequence numbers.
/// If the two sequence numbers are close together, it is as normal, but they are far apart, it is assumed that they have wrapped around.
/// Thus, sequence_greater_than( 1, 0 ) returns true, and so does sequence_greater_than( 0, 65535 )!
#[inline(always)]
pub(crate) fn sequence_greater_than(s1: u16, s2: u16) -> bool {
    ((s1 > s2) && (s1 - s2 <= 32768)) || ((s1 < s2) && (s2 - s1 > 32768))
}

/// Compares two 16 bit sequence numbers and returns true if the first one is less than the second (considering wrapping).
/// IMPORTANT: This is not the same as s1 < s2!
/// Greater than is defined specially to handle wrapping sequence numbers.
/// If the two sequence numbers are close together, it is as normal, but they are far apart, it is assumed that they have wrapped around.
/// Thus, sequence_less_than( 0, 1 ) returns true, and so does sequence_less_than( 65535, 0 )!
#[inline(always)]
pub(crate) fn sequence_less_than(s1: u16, s2: u16) -> bool {
    sequence_greater_than(s2, s1)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sequence_greater_than() {
        assert!(!sequence_greater_than(0, 1));
        assert!(sequence_greater_than(1, 0));
        assert!(sequence_greater_than(0, 65535));

        // floor(65535 / 2) + 1 should be the largest value with respect to 0
        assert!(!sequence_greater_than(0, 65535 / 2 + 1));
        assert!(sequence_greater_than(0, 65535 / 2 + 2));
    }

    #[test]
    fn test_sequence_less_than() {
        assert!(!sequence_less_than(1, 0));
        assert!(sequence_less_than(0, 1));
        assert!(!sequence_less_than(0, 65535));

        // floor(65535 / 2) + 1 should be the largest value with respect to 0
        assert!(sequence_less_than(0, 65535 / 2 + 1));
        assert!(!sequence_less_than(0, 65535 / 2 + 2));
    }

    #[test]
    fn test_sequence_comparisons() {
        for x in 0u16..=65535 {
            assert!(!sequence_greater_than(x, x.wrapping_add(25)));
            assert!(sequence_greater_than(x, x.wrapping_sub(25)));

            assert!(sequence_less_than(x, x.wrapping_add(25)));
            assert!(!sequence_less_than(x, x.wrapping_sub(25)));

            // test transitivity
            let left = x.wrapping_sub(15000);
            let right = x.wrapping_add(15000);

            assert!(sequence_less_than(left, x));
            assert!(sequence_less_than(x, right));
            assert!(sequence_less_than(left, right));

            assert!(sequence_greater_than(right, x));
            assert!(sequence_greater_than(x, left));
            assert!(sequence_greater_than(right, left));
        }
    }

    #[test]
    fn test_sequence_buffer() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct SeqData {
            seq: u16,
            value: usize,
        }

        let size = 256;

        let mut buffer = SequenceBuffer::new(size);

        for i in 0..(5 * size) as u16 {
            // assert that the buffer is empty
            assert!(buffer.available(i));
            assert!(buffer.get(i).is_none());
        }

        // check that we can continuously enter values
        let total_entries = 100_000;
        let mut seq = 0;
        for value in 0..total_entries {
            let entry = SeqData { seq, value };

            assert!(buffer.insert_with(seq, || entry));

            // verify we cannot insert something too old
            assert!(!buffer.insert_with(seq.wrapping_sub(size as u16), || entry));

            if value == 0 {
                // the previous entry will not exist for the first value
                assert!(buffer.available(seq.wrapping_sub(1)));
            } else {
                // otherwise the previous entry will always fail to insert
                assert!(buffer.exists(seq.wrapping_sub(1)));
            }

            assert_eq!(buffer.sequence_pointer(), seq.wrapping_add(1));
            assert_eq!(buffer.get(seq).cloned(), Some(entry));

            seq = seq.wrapping_add(1);
        }

        // check that the buffer remembers everything within range of `size`
        assert!(total_entries > size);
        for i in 1..size {
            assert!(buffer.exists(seq.wrapping_sub(i as u16)), "seq: {}", i);
        }
        // ...and that it forgot about `seq - (size + 1)`
        let forgotten_seq = seq.wrapping_sub((size + 1) as u16);
        assert!(!buffer.exists(forgotten_seq));
        assert_eq!(buffer.get(forgotten_seq), None);

        assert_eq!(buffer.capacity(), size);

        buffer.reset();
        assert_eq!(buffer.sequence_pointer(), 0);
        for i in 0..(5 * size) as u16 {
            // assert that the buffer is empty
            assert!(buffer.available(i));
            assert!(buffer.get(i).is_none());
        }
    }

    #[test]
    fn test_sequence_buffer_take() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct SeqData {
            seq: u16,
            value: usize,
        }

        let size = 256;

        let mut buffer = SequenceBuffer::new(size);

        let total_entries = 100_000;
        let mut seq = 0;
        for value in 0..total_entries {
            let entry = SeqData { seq, value };
            assert!(buffer.insert_with(seq, || entry));
            seq = seq.wrapping_add(1);
        }

        for i in 1..=size {
            let expect_seq = seq - (i as u16);
            let expect_value = total_entries - i;
            assert!(buffer.exists(expect_seq));
            assert!(!buffer.available(expect_seq));
            assert_eq!(
                buffer.take(expect_seq),
                Some(SeqData {
                    seq: expect_seq,
                    value: expect_value
                })
            );
            assert!(buffer.available(expect_seq));
        }
    }
}
