//! Network abstraction layer units, which escape any byte pattern in their payload that would
//! look like a start code.

/// A unit of `header` and an escaped `rbsp` payload.
pub(crate) fn escape(header: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut nal = vec![header];
    let mut zeros = 0;
    for &byte in rbsp {
        if zeros >= 2 && byte <= 3 {
            nal.push(3);
            zeros = 0;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        nal.push(byte);
    }
    nal
}
