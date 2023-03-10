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

    /// Insert an entry in the sequence buffer.
    ///
    /// IMPORTANT: If another entry exists at the sequence modulo buffer size, it is overwritten.
    ///
    /// Returns None if the insert was successful, or `Some(entry)` if the entry could not be added.
    /// This happens when the sequence number is too old.
    #[must_use]
    pub(crate) fn insert(&mut self, sequence: u16, entry: T) -> Option<T> {
        if sequence_greater_than(sequence + 1, self.sequence) {
            self.remove_entries(self.sequence, sequence);
            self.sequence = sequence + 1;
        } else if sequence_less_than(sequence, self.sequence.wrapping_sub(self.capacity() as u16)) {
            return Some(entry);
        }
        let index = self.sequence_index(sequence);
        self.entry_sequence[index] = Some(sequence);
        self.entries[index] = Some(entry);
        None
    }

    /// Insert an entry into the sequence buffer.
    ///
    /// IMPORTANT: If another entry exists at `sequence` % buffer size,
    /// it is overwritten.
    ///
    /// Returns true if the insert was successful, or false if the entry could
    /// not be added. This happens when the sequence number is too old.
    pub(crate) fn insert_with<F: FnOnce() -> T>(&mut self, sequence: u16, f: F) -> bool {
        if sequence_greater_than(sequence + 1, self.sequence) {
            self.remove_entries(self.sequence, sequence);
            self.sequence = sequence + 1;
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

    /// Remove an entry from the sequence buffer.
    pub(crate) fn remove(&mut self, sequence: u16) {
        let index = self.sequence_index(sequence);
        self.entry_sequence[index] = None;
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
fn sequence_less_than(s1: u16, s2: u16) -> bool {
    sequence_greater_than(s2, s1)
}
