use std::borrow::Cow;

use kodosi_domain::session::SessionMode;

use crate::{runtime::MAX_SESSION_TITLE_LEN, session_runtime::commands::SessionInput};

pub(crate) fn normalized_session_title(requested_title: &str) -> crate::Result<String> {
    let trimmed = requested_title.trim();
    if trimmed.is_empty() {
        return Err(crate::AppError::Unsupported {
            reason: "session titles cannot be blank".to_owned(),
        });
    }
    Ok(if trimmed.len() > MAX_SESSION_TITLE_LEN {
        trimmed[..trimmed.floor_char_boundary(MAX_SESSION_TITLE_LEN)].to_owned()
    } else {
        trimmed.to_owned()
    })
}

pub(crate) fn shift_tab_count_for_agent(
    detected_agent: Option<&str>,
    current_mode: SessionMode,
    target_mode: SessionMode,
    session_id: kodosi_domain::ids::SessionId,
) -> crate::Result<usize> {
    use kodosi_session::AgentKind;

    let kind = detected_agent.and_then(AgentKind::from_banner);
    match (kind, detected_agent) {
        (Some(AgentKind::Copilot), _) => Ok(match (current_mode, target_mode) {
            (SessionMode::Normal, SessionMode::Plan)
            | (SessionMode::Plan, SessionMode::Autopilot)
            | (SessionMode::Autopilot, SessionMode::Normal) => 1,
            (SessionMode::Normal, SessionMode::Autopilot)
            | (SessionMode::Plan, SessionMode::Normal)
            | (SessionMode::Autopilot, SessionMode::Plan) => 2,
            _ => 0,
        }),
        (Some(AgentKind::Claude), _) => Ok(match (current_mode, target_mode) {
            (SessionMode::Normal, SessionMode::Autopilot)
            | (SessionMode::Autopilot, SessionMode::Plan)
            | (SessionMode::Plan, SessionMode::Normal) => 1,
            (SessionMode::Normal, SessionMode::Plan)
            | (SessionMode::Plan, SessionMode::Autopilot)
            | (SessionMode::Autopilot, SessionMode::Normal) => 2,
            _ => 0,
        }),
        (_, Some(agent)) => Err(crate::AppError::Unsupported {
            reason: format!(
                "session {} is running {agent}; desktop mode control is currently only wired for Claude and GitHub Copilot",
                session_id.short()
            ),
        }),
        (_, None) => Err(crate::AppError::Unsupported {
            reason: format!(
                "session {} is not running a detected agent yet, so desktop mode control is unavailable",
                session_id.short()
            ),
        }),
    }
}

pub(crate) fn shift_tab_bytes(count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count * 3);
    for _ in 0..count {
        bytes.extend_from_slice(b"\x1b[Z");
    }
    bytes
}

pub(crate) fn session_input_bytes(input: &SessionInput) -> Cow<'_, [u8]> {
    Cow::Borrowed(input.as_bytes())
}

pub(crate) const fn session_input_command_name(_input: &SessionInput) -> &'static str {
    "session.inputBytes"
}

pub(crate) fn session_mode_label(mode: SessionMode) -> &'static str {
    match mode {
        SessionMode::Normal => "normal",
        SessionMode::Plan => "plan",
        SessionMode::Autopilot => "autopilot",
    }
}

#[cfg(test)]
mod tests {
    use super::{session_input_bytes, shift_tab_count_for_agent};
    use crate::session_runtime::commands::SessionInput;
    use kodosi_domain::{ids::SessionId, session::SessionMode};

    fn sid() -> SessionId {
        SessionId::parse_field("019d1bd1-c0ae-72a0-88cb-c8519739adaa", "session_id").unwrap()
    }

    #[test]
    fn terminal_input_bytes_preserve_every_raw_byte() {
        let all_bytes = (0_u8..=u8::MAX).collect::<Vec<_>>();
        assert_eq!(
            session_input_bytes(&SessionInput::new(all_bytes.clone())),
            all_bytes
        );
    }

    #[test]
    fn copilot_three_mode_cycle() {
        let agent = Some("GitHub Copilot");
        let cases = [
            (SessionMode::Normal, SessionMode::Plan, 1),
            (SessionMode::Plan, SessionMode::Autopilot, 1),
            (SessionMode::Autopilot, SessionMode::Normal, 1),
            (SessionMode::Normal, SessionMode::Autopilot, 2),
            (SessionMode::Plan, SessionMode::Normal, 2),
            (SessionMode::Autopilot, SessionMode::Plan, 2),
            (SessionMode::Normal, SessionMode::Normal, 0),
            (SessionMode::Plan, SessionMode::Plan, 0),
            (SessionMode::Autopilot, SessionMode::Autopilot, 0),
        ];
        for (from, to, expected) in cases {
            let got = shift_tab_count_for_agent(agent, from, to, sid()).unwrap();
            assert_eq!(got, expected, "copilot {from:?} -> {to:?}");
        }
    }

    #[test]
    fn claude_three_mode_cycle() {
        let agent = Some("Claude");
        let cases = [
            (SessionMode::Normal, SessionMode::Autopilot, 1),
            (SessionMode::Autopilot, SessionMode::Plan, 1),
            (SessionMode::Plan, SessionMode::Normal, 1),
            (SessionMode::Normal, SessionMode::Plan, 2),
            (SessionMode::Plan, SessionMode::Autopilot, 2),
            (SessionMode::Autopilot, SessionMode::Normal, 2),
            (SessionMode::Normal, SessionMode::Normal, 0),
            (SessionMode::Plan, SessionMode::Plan, 0),
            (SessionMode::Autopilot, SessionMode::Autopilot, 0),
        ];
        for (from, to, expected) in cases {
            let got = shift_tab_count_for_agent(agent, from, to, sid()).unwrap();
            assert_eq!(got, expected, "claude {from:?} -> {to:?}");
        }
    }

    #[test]
    fn unknown_agent_is_rejected() {
        let err = shift_tab_count_for_agent(
            Some("hackerman"),
            SessionMode::Normal,
            SessionMode::Plan,
            sid(),
        )
        .expect_err("unknown agent must be rejected");
        let message = format!("{err:?}");
        assert!(
            message.contains("currently only wired for Claude and GitHub Copilot"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn no_agent_is_rejected() {
        let err = shift_tab_count_for_agent(None, SessionMode::Normal, SessionMode::Plan, sid())
            .expect_err("missing agent must be rejected");
        let message = format!("{err:?}");
        assert!(
            message.contains("not running a detected agent yet"),
            "unexpected error: {message}"
        );
    }
}
