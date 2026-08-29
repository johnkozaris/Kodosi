use std::time::Instant;

use crate::domain::{AgentError, AgentErrorKind};

const ERROR_TTL_SECS: u64 = 30;

pub struct ErrorDetector {
    current_error: Option<AgentError>,
    last_detected: Option<Instant>,
    line_buffer: String,
}

impl Default for ErrorDetector {
    fn default() -> Self {
        Self {
            current_error: None,
            last_detected: None,
            line_buffer: String::with_capacity(512),
        }
    }
}

impl ErrorDetector {
    pub fn feed(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        self.line_buffer.push_str(&text);

        if self.line_buffer.len() > 2048 {
            let start = self
                .line_buffer
                .ceil_char_boundary(self.line_buffer.len() - 2048);
            self.line_buffer.drain(..start);
        }

        let lower = self.line_buffer.to_lowercase();

        if let Some((kind, message)) = detect_error_pattern(&lower) {
            let now = time::OffsetDateTime::now_utc();
            self.current_error = Some(AgentError {
                kind,
                message,
                timestamp: now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            });
            self.last_detected = Some(Instant::now());

            self.line_buffer.clear();
        }
    }

    pub fn tick(&mut self) {
        if let Some(detected_at) = self.last_detected
            && detected_at.elapsed().as_secs() > ERROR_TTL_SECS
        {
            self.current_error = None;
            self.last_detected = None;
        }
    }

    pub const fn take_error(&mut self) -> Option<AgentError> {
        self.current_error.take()
    }
}

fn detect_error_pattern(text: &str) -> Option<(AgentErrorKind, String)> {
    if text.contains("rate limit") || text.contains("too many requests") {
        return Some((
            AgentErrorKind::RateLimit,
            "Rate limited. Claude Code is waiting before retrying.".to_owned(),
        ));
    }
    if text.contains("invalid api key") || text.contains("invalid x-api-key") {
        return Some((
            AgentErrorKind::AuthFailure,
            "Authentication error. Check your API key.".to_owned(),
        ));
    }
    if text.contains("quota exceeded") || text.contains("insufficient credits") {
        return Some((
            AgentErrorKind::QuotaExceeded,
            "Usage quota exceeded.".to_owned(),
        ));
    }
    if text.contains("context window full") || text.contains("context length exceeded") {
        return Some((
            AgentErrorKind::ContextOverflow,
            "Context window exceeded. Try /compact.".to_owned(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_trim_lands_on_a_char_boundary() {
        let mut det = ErrorDetector::default();
        det.feed("你".repeat(2000).as_bytes());
        assert!(det.line_buffer.len() <= 2048);
        assert_eq!(det.line_buffer.len() % 3, 0);
        assert!(det.line_buffer.chars().all(|c| c == '你'));

        det.feed("好".repeat(2000).as_bytes());
        assert!(det.line_buffer.len() <= 2048);
        assert!(det.line_buffer.chars().all(|c| c == '你' || c == '好'));
    }

    #[test]
    fn detects_rate_limit() {
        let (kind, _) = detect_error_pattern("error: rate limit exceeded, retrying in 60s")
            .expect("should detect");
        assert_eq!(kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn detects_auth_failure() {
        let (kind, _) =
            detect_error_pattern("unauthorized: invalid api key").expect("should detect");
        assert_eq!(kind, AgentErrorKind::AuthFailure);
    }

    #[test]
    fn no_false_positive() {
        assert!(detect_error_pattern("everything is working fine").is_none());
    }

    #[test]
    fn feed_keeps_utf8_boundaries_when_trimming() {
        let mut detector = ErrorDetector::default();
        let input = format!("{} rate limit exceeded", "🙂".repeat(700));
        detector.feed(input.as_bytes());

        let error = detector.take_error().expect("should detect rate limit");
        assert_eq!(error.kind, AgentErrorKind::RateLimit);
    }
}
