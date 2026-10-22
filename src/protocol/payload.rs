//! An owned receive buffer whose consumed headers stay outside the visible slice.
use std::ops::{Deref, DerefMut, Range};

#[derive(Debug, Default)]
pub(crate) struct Payload {
    bytes: Vec<u8>,
    start: usize,
}

impl From<Vec<u8>> for Payload {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes, start: 0 }
    }
}

impl Payload {
    pub(crate) fn restrict(&mut self, range: Range<usize>) {
        assert!(range.start <= range.end && range.end <= self.len());
        self.bytes.truncate(self.start + range.end);
        self.start += range.start;
    }

    /// Compact only when a consumer requires a standalone Vec.
    pub(crate) fn into_vec(mut self) -> Vec<u8> {
        if self.start != 0 {
            let len = self.len();
            self.bytes.copy_within(self.start.., 0);
            self.bytes.truncate(len);
        }
        self.bytes
    }
}

impl Deref for Payload {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes[self.start..]
    }
}

impl DerefMut for Payload {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[self.start..]
    }
}

impl AsRef<[u8]> for Payload {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

#[cfg(test)]
impl<T: AsRef<[u8]>> PartialEq<T> for Payload {
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other.as_ref()
    }
}
