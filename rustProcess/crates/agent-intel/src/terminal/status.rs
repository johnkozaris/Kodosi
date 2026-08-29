use std::time::Instant;

use crate::domain::AgentStatus;

const IDLE_THRESHOLD_MS: u128 = 2000;

const HITL_STICKY_VISIBLE_BYTES: usize = 80;

const IDLE_TO_RUNNING_BYTES: usize = 120;

const HITL_FALLBACK_LAST_LINES: usize = 5;

pub struct StatusDetector {
    status: AgentStatus,
    last_output: Instant,

    visible_since_hitl: usize,
    bytes_since_idle: usize,
    tail_buffer: Vec<u8>,
}

impl Default for StatusDetector {
    fn default() -> Self {
        Self {
            status: AgentStatus::Idle,
            last_output: Instant::now(),
            visible_since_hitl: 0,
            bytes_since_idle: 0,
            tail_buffer: Vec::with_capacity(512),
        }
    }
}

impl StatusDetector {
    pub fn feed(&mut self, bytes: &[u8]) -> Option<AgentStatus> {
        if bytes.is_empty() {
            return None;
        }

        self.last_output = Instant::now();
        let prev = self.status;

        self.tail_buffer.extend_from_slice(bytes);
        if self.tail_buffer.len() > 500 {
            let start = self.tail_buffer.len() - 500;
            self.tail_buffer.drain(..start);
        }

        let stripped = strip_ansi_lossy(&self.tail_buffer);

        match self.status {
            AgentStatus::WaitingForInput => {
                self.visible_since_hitl += count_visible_bytes(bytes);
                if self.visible_since_hitl > HITL_STICKY_VISIBLE_BYTES {
                    self.transition_to(AgentStatus::Running);
                }
            }
            AgentStatus::Idle => {
                if detect_hitl_prompt(&stripped) {
                    self.transition_to(AgentStatus::WaitingForInput);
                } else {
                    self.bytes_since_idle += bytes.len();
                    if self.bytes_since_idle >= IDLE_TO_RUNNING_BYTES {
                        self.transition_to(AgentStatus::Running);
                    }
                }
            }
            AgentStatus::Running => {
                if detect_hitl_prompt(&stripped) {
                    self.transition_to(AgentStatus::WaitingForInput);
                } else {
                    self.status = AgentStatus::Running;
                }
            }
        }

        if self.status == prev {
            None
        } else {
            Some(self.status)
        }
    }

    pub fn tick(&mut self) -> Option<AgentStatus> {
        let prev = self.status;

        if self.status == AgentStatus::Running
            && self.last_output.elapsed().as_millis() > IDLE_THRESHOLD_MS
        {
            self.transition_to(AgentStatus::Idle);
        }

        if self.status == prev {
            None
        } else {
            Some(self.status)
        }
    }

    fn transition_to(&mut self, next: AgentStatus) {
        if next == self.status {
            return;
        }
        self.status = next;
        self.bytes_since_idle = 0;
        self.visible_since_hitl = 0;
    }
}

fn detect_hitl_prompt(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    if last_non_empty_line_is_ready_prompt(trimmed) {
        return false;
    }

    if matches_hitl_pattern(trimmed) {
        return true;
    }

    let last_lines = collect_last_non_empty_lines(trimmed, HITL_FALLBACK_LAST_LINES);
    matches_hitl_pattern(&last_lines)
}

fn matches_hitl_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();
    HITL_PATTERNS.iter().any(|p| lower.contains(p))
}

fn collect_last_non_empty_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn last_non_empty_line_is_ready_prompt(text: &str) -> bool {
    let Some(last) = text.lines().rev().find(|l| !l.trim().is_empty()) else {
        return false;
    };
    let trimmed = last.trim();
    if trimmed == ">" {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix('>') {
        return rest.chars().all(char::is_whitespace);
    }
    false
}

const HITL_PATTERNS: &[&str] = &[
    "[y/n]",
    "(y/n)",
    "allow / deny",
    "allow/deny",
    "do you want",
    "proceed?",
    "confirm?",
    "approve?",
    "enter to select",
    "esc to cancel",
    "tab/arrow keys",
];

fn count_visible_bytes(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    let mut count = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek() {
                Some(&'[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(&']') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == '\x07' || c == '\\' {
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else {
            count += ch.len_utf8();
        }
    }

    count
}

fn strip_ansi_lossy(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                chars.next();
                for c in chars.by_ref() {
                    if c == '\x07' || c == '\\' {
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_running_on_output() {
        let mut det = StatusDetector::default();
        let payload = vec![b'x'; IDLE_TO_RUNNING_BYTES + 1];
        let status = det.feed(&payload);
        assert_eq!(status, Some(AgentStatus::Running));
    }

    #[test]
    fn detects_hitl_on_permission_prompt() {
        let mut det = StatusDetector::default();
        det.feed(b"some output\n");
        let status = det.feed(b"Allow this tool call? [Y/n] ");
        assert_eq!(status, Some(AgentStatus::WaitingForInput));
    }

    #[test]
    fn hitl_sticky_until_enough_visible_output() {
        let mut det = StatusDetector::default();
        det.feed(b"Allow / Deny ");
        assert_eq!(det.status, AgentStatus::WaitingForInput);

        det.feed(b"small");
        assert_eq!(det.status, AgentStatus::WaitingForInput);

        det.feed(&[b'x'; 100]);
        assert_eq!(det.status, AgentStatus::Running);
    }

    #[test]
    fn hitl_does_not_exit_on_ansi_redraw() {
        let mut det = StatusDetector::default();
        det.feed(b"Allow this? [Y/n] ");
        assert_eq!(det.status, AgentStatus::WaitingForInput);

        let mut redraw = Vec::new();
        for _ in 0..20 {
            redraw.extend_from_slice(b"\x1b[1;31m");
            redraw.extend_from_slice(b"\x1b[2;5H");
        }
        redraw.extend_from_slice(b"redraw");

        assert!(
            redraw.len() > HITL_STICKY_VISIBLE_BYTES,
            "test fixture must exceed raw-byte threshold to be meaningful"
        );

        det.feed(&redraw);
        assert_eq!(
            det.status,
            AgentStatus::WaitingForInput,
            "ANSI-heavy redraw must not exit HITL state"
        );
    }

    #[test]
    fn detects_hitl_when_prompt_pushed_down_by_status_line() {
        let mut det = StatusDetector::default();
        let buffer = b"\
            ----- syncing -----\n\
            Allow this tool call? [Y/n] \n\
            Saved 3 files.\n\
            ";
        det.feed(buffer);
        assert_eq!(det.status, AgentStatus::WaitingForInput);
    }

    #[test]
    fn bare_ready_prompt_is_not_hitl() {
        let mut det = StatusDetector::default();
        det.feed(b"some agent output\n>");
        assert_ne!(det.status, AgentStatus::WaitingForInput);

        det.feed(b"more output\n> ");
        assert_ne!(det.status, AgentStatus::WaitingForInput);

        det.feed(b"more output\n>   ");
        assert_ne!(det.status, AgentStatus::WaitingForInput);
    }

    #[test]
    fn tick_transitions_running_to_idle_after_threshold() {
        let mut det = StatusDetector::default();
        det.feed(&[b'x'; IDLE_TO_RUNNING_BYTES + 1]);
        assert_eq!(det.status, AgentStatus::Running);

        assert!(det.tick().is_none());
        assert_eq!(det.status, AgentStatus::Running);

        let rewind = std::time::Duration::from_millis(
            u64::try_from(IDLE_THRESHOLD_MS).unwrap_or(u64::MAX) + 50,
        );
        det.last_output = Instant::now()
            .checked_sub(rewind)
            .expect("instant arithmetic should not underflow within test runtime");
        assert_eq!(det.tick(), Some(AgentStatus::Idle));
        assert_eq!(det.status, AgentStatus::Idle);
    }

    #[test]
    fn count_visible_bytes_strips_csi_and_osc() {
        assert_eq!(count_visible_bytes(b"hello"), 5);
        assert_eq!(count_visible_bytes(b"\x1b[31mhello\x1b[0m"), 5);
        assert_eq!(count_visible_bytes(b"\x1b]2;title\x07hello"), 5);
        assert_eq!(count_visible_bytes(b""), 0);
    }

    #[test]
    fn strip_ansi_removes_csi() {
        let input = b"\x1b[32mHello\x1b[0m World";
        let result = strip_ansi_lossy(input);
        assert_eq!(result, "Hello World");
    }
}
