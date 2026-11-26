//! Writes the bits of the headers that the encoder writes itself, H.264's and HEVC's alike.

use crate::nal::escape;

/// A payload's bits, the most significant first.
#[derive(Default)]
pub(super) struct Bits {
    bytes: Vec<u8>,
    count: usize,
}

impl Bits {
    pub(super) fn put(&mut self, value: u32, width: u32) {
        for shift in (0..width).rev() {
            if self.count.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (value >> shift) & 1 == 1
                && let Some(last) = self.bytes.last_mut()
            {
                *last |= 0x80 >> (self.count % 8);
            }
            self.count += 1;
        }
    }

    pub(super) fn flag(&mut self, on: bool) {
        self.put(u32::from(on), 1);
    }

    /// An unsigned Exp-Golomb code, `ue(v)`. Zero is also the code of a signed zero, `se(v)`.
    pub(super) fn number(&mut self, value: usize) {
        let coded = u32::try_from(value + 1).unwrap_or(u32::MAX);
        let width = coded.bit_width();
        self.put(0, width - 1);
        self.put(coded, width);
    }

    /// A signed Exp-Golomb code, `se(v)`.
    pub(super) fn signed(&mut self, value: i8) {
        let magnitude = usize::from(value.unsigned_abs());
        self.number(if value > 0 {
            2 * magnitude - 1
        } else {
            2 * magnitude
        });
    }

    /// A one bit, then zeros up to the next byte.
    pub(super) fn align(&mut self) {
        self.put(1, 1);
        self.count = self.bytes.len() * 8;
    }

    /// A unit after a start code, of `header` and the payload so far, and its length in bits.
    pub(super) fn unit(&self, header: &[u8]) -> (Vec<u8>, usize) {
        let mut unit = vec![0, 0, 0, 1];
        unit.extend(escape(header, &self.bytes));
        let unused = (8 - self.count % 8) % 8;
        let length = unit.len() * 8 - unused;
        (unit, length)
    }

    /// Ends the payload with its stop bit and makes it a unit.
    pub(super) fn ended(mut self, header: &[u8]) -> Vec<u8> {
        self.align();
        self.unit(header).0
    }
}
