//! Encodes the pictures of the watched tab as H.264 in Annex B form, for the host's viewers.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(target_os = "macos"))]
mod software;

#[cfg(target_os = "macos")]
pub(crate) use mac::Encoder;
#[cfg(not(target_os = "macos"))]
pub(crate) use software::Encoder;

/// The most pictures that wait in the encoder. A newer picture replaces a waiting one, so the
/// viewer sees the page as it is now.
const MOST_WAITING: usize = 2;
const FRAME_RATE: u32 = 60;
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// Rewrites length-prefixed NAL units as Annex B units with start codes.
fn annex_b_units(data: &[u8], length_size: usize, out: &mut Vec<u8>) -> Option<()> {
    let mut rest = data;
    while !rest.is_empty() {
        let (length, after) = rest.split_at_checked(length_size)?;
        let length = length
            .iter()
            .fold(0_usize, |sum, byte| (sum << 8) | usize::from(*byte));
        let (unit, after) = after.split_at_checked(length)?;
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unit);
        rest = after;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_units_get_start_codes() {
        let mut out = Vec::new();
        annex_b_units(&[0, 0, 0, 2, 0x65, 0xAA, 0, 0, 0, 1, 0x41], 4, &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1, 0x41]);
    }

    #[test]
    fn a_cut_unit_is_refused() {
        assert!(annex_b_units(&[0, 0, 0, 9, 0x65], 4, &mut Vec::new()).is_none());
    }
}
