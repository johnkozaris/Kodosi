use bytes::{Bytes, BytesMut};

use super::TerminalDataFrame;

pub struct CaptureBuffer {
    max_bytes: usize,
    buf: BytesMut,
    total_bytes_seen: usize,
    truncated: bool,
}

impl CaptureBuffer {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            buf: BytesMut::new(),
            total_bytes_seen: 0,
            truncated: false,
        }
    }

    pub fn accept(&mut self, frame: &TerminalDataFrame) {
        let incoming = &frame.bytes;
        self.total_bytes_seen = self.total_bytes_seen.saturating_add(incoming.len());

        let remaining = self.max_bytes.saturating_sub(self.buf.len());
        if remaining == 0 {
            if !incoming.is_empty() {
                self.truncated = true;
            }
            return;
        }

        if incoming.len() <= remaining {
            self.buf.extend_from_slice(incoming);
        } else {
            self.buf.extend_from_slice(&incoming[..remaining]);
            self.truncated = true;
        }
    }

    #[must_use]
    pub fn total_bytes_seen(&self) -> usize {
        self.total_bytes_seen
    }

    #[must_use]
    pub fn captured_bytes(&self) -> &[u8] {
        &self.buf
    }

    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    #[must_use]
    pub fn into_bytes(self) -> Bytes {
        self.buf.freeze()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sequence: u64, bytes: &'static [u8]) -> TerminalDataFrame {
        TerminalDataFrame::new(sequence, Bytes::from_static(bytes))
    }

    #[test]
    fn capture_buffer_collects_up_to_limit() {
        let mut capture = CaptureBuffer::new(5);
        capture.accept(&frame(0, b"abc"));
        capture.accept(&frame(1, b"de"));

        assert_eq!(capture.total_bytes_seen(), 5);
        assert_eq!(capture.captured_bytes(), b"abcde");
        assert!(!capture.truncated());
    }

    #[test]
    fn capture_buffer_truncates_at_limit() {
        let mut capture = CaptureBuffer::new(4);
        capture.accept(&frame(0, b"abcdef"));

        assert_eq!(capture.total_bytes_seen(), 6);
        assert_eq!(capture.captured_bytes(), b"abcd");
        assert!(capture.truncated());
    }

    #[test]
    fn capture_buffer_tracks_bytes_after_limit() {
        let mut capture = CaptureBuffer::new(3);
        capture.accept(&frame(0, b"abc"));
        capture.accept(&frame(1, b"def"));

        assert_eq!(capture.total_bytes_seen(), 6);
        assert_eq!(capture.captured_bytes(), b"abc");
        assert!(capture.truncated());
    }

    #[test]
    fn zero_limit_capture_is_truncated_by_nonempty_input() {
        let mut capture = CaptureBuffer::new(0);
        capture.accept(&frame(0, b"x"));

        assert_eq!(capture.total_bytes_seen(), 1);
        assert!(capture.captured_bytes().is_empty());
        assert!(capture.truncated());
    }

    #[test]
    fn empty_input_does_not_mark_capture_truncated() {
        let mut capture = CaptureBuffer::new(10);
        capture.accept(&frame(0, b""));

        assert_eq!(capture.total_bytes_seen(), 0);
        assert!(!capture.truncated());
    }

    #[test]
    fn capture_buffer_returns_owned_bytes() {
        let mut capture = CaptureBuffer::new(7);
        capture.accept(&frame(0, b"capture"));

        assert_eq!(capture.into_bytes(), Bytes::from_static(b"capture"));
    }
}
