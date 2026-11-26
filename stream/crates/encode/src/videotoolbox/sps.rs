use crate::nal::escape;

/// Sets `gaps_in_frame_num_value_allowed_flag` in an H.264 SPS NAL unit. Viewers resuming at
/// an LTR anchor see gaps in `frame_num`, which decoders may otherwise treat as lost frames.
pub(super) fn allowing_gaps(nal: &[u8]) -> Option<Vec<u8>> {
    let (&header, payload) = nal.split_first()?;
    let mut bits = Bits {
        rbsp: unescape(payload),
        at: 0,
    };
    let profile = bits.read(8)?;
    bits.read(16)?;
    bits.number()?;
    if matches!(
        profile,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    ) {
        if bits.number()? == 3 {
            bits.read(1)?;
        }
        bits.number()?;
        bits.number()?;
        bits.read(1)?;
        if bits.read(1)? == 1 {
            return None;
        }
    }
    bits.number()?;
    match bits.number()? {
        0 => {
            bits.number()?;
        }
        1 => {
            bits.read(1)?;
            bits.number()?;
            bits.number()?;
            for _ in 0..bits.number()? {
                bits.number()?;
            }
        }
        _ => {}
    }
    bits.number()?;
    let at = bits.at;
    let mut rbsp = bits.rbsp;
    *rbsp.get_mut(at / 8)? |= 0x80 >> (at % 8);
    Some(escape(header, &rbsp))
}

struct Bits {
    rbsp: Vec<u8>,
    at: usize,
}

impl Bits {
    fn read(&mut self, count: u32) -> Option<u32> {
        let mut value = 0;
        for _ in 0..count {
            let byte = self.rbsp.get(self.at / 8)?;
            value = value << 1 | u32::from(byte >> (7 - self.at % 8) & 1);
            self.at += 1;
        }
        Some(value)
    }

    /// Reads an unsigned Exp-Golomb code (`ue(v)`). Also used to skip signed (`se(v)`) fields
    /// since both encodings have the same bit length.
    fn number(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.read(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1 << zeros) - 1 + self.read(zeros)?)
    }
}

fn unescape(payload: &[u8]) -> Vec<u8> {
    let mut rbsp = Vec::with_capacity(payload.len());
    let mut zeros = 0;
    for &byte in payload {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        rbsp.push(byte);
    }
    rbsp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_of_videotoolbox_allows_gaps_and_keeps_the_rest() {
        let set = [
            0x27, 0x64, 0x0c, 0x34, 0xac, 0x13, 0x14, 0x30, 0x02, 0x80, 0x0d, 0x1a, 0x6a, 0x02,
            0x02, 0x02, 0x0f, 0x08, 0x04, 0x23, 0x20,
        ];
        let mut allowed = set;
        allowed[7] = 0x32;
        assert_eq!(allowing_gaps(&set), Some(allowed.to_vec()));
    }

    #[test]
    fn the_bytes_that_would_look_like_a_start_code_keep_their_escape() {
        let payload = [0x42, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x03, 0x03];
        assert_eq!(escape(0x67, &unescape(&payload))[1..], payload);
    }
}
