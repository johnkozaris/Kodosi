use tokio_tungstenite::tungstenite::protocol::CloseFrame;

pub(crate) const AUTH_REVOKED_REASON: &str = "auth_revoked";
pub(crate) const TERMINAL_REPLAY_GAP_REASON: &str = "terminal_replay_gap";
pub(crate) const RETRYABLE_REASONS: &[&str] = &[
    "capacity_reached",
    "closing",
    "lagging_participant",
    "server_error",
    "server_restarting",
    "session_runtime_recovering",
    "timeout",
];
pub(crate) const TERMINAL_REASONS: &[&str] = &[
    "access_revoked",
    "already_hosted",
    "client_messages_not_supported",
    "ended",
    "expected_host_hello",
    "expected_participant_join",
    "host_missing",
    "host_stopped",
    "invalid_message",
    "invalid_session_id",
    "invalid_session_or_secret",
    "invalid_session_state",
    "owner_identity_reset",
    "session_not_found",
    "session_not_live",
    "session_timeout",
    "unsupported_data",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloseBehavior {
    AuthRefresh,

    TerminalReplayGap,
    Terminal { reason: String },
    Retryable,
}

pub(crate) fn interpret(frame: Option<&CloseFrame>) -> CloseBehavior {
    let Some(frame) = frame else {
        return CloseBehavior::Retryable;
    };
    let reason = frame.reason.as_ref();
    if reason == AUTH_REVOKED_REASON {
        return CloseBehavior::AuthRefresh;
    }
    if reason == TERMINAL_REPLAY_GAP_REASON {
        return CloseBehavior::TerminalReplayGap;
    }
    if RETRYABLE_REASONS.contains(&reason) {
        return CloseBehavior::Retryable;
    }
    if TERMINAL_REASONS.contains(&reason) {
        return CloseBehavior::Terminal {
            reason: reason.to_owned(),
        };
    }

    if reason.is_empty() {
        return CloseBehavior::Retryable;
    }

    tracing::error!(%reason, "unknown relay close reason; treating as terminal protocol drift");
    CloseBehavior::Terminal {
        reason: format!("unknown:{reason}"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        AUTH_REVOKED_REASON, CloseBehavior, RETRYABLE_REASONS, TERMINAL_REASONS,
        TERMINAL_REPLAY_GAP_REASON, interpret,
    };
    use crate::session_relay_authority::load_session_relay_authority;
    use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};

    fn frame(reason: &str) -> CloseFrame {
        CloseFrame {
            code: CloseCode::Policy,
            reason: reason.into(),
        }
    }

    #[test]
    fn auth_revoked_requests_refresh() {
        assert_eq!(
            interpret(Some(&frame(AUTH_REVOKED_REASON))),
            CloseBehavior::AuthRefresh
        );
    }

    #[test]
    fn empty_reason_and_missing_frame_stay_retryable() {
        assert_eq!(interpret(Some(&frame(""))), CloseBehavior::Retryable);
        assert_eq!(interpret(None), CloseBehavior::Retryable);
    }

    #[test]
    fn unknown_reason_is_terminal() {
        assert_eq!(
            interpret(Some(&frame("totally_made_up"))),
            CloseBehavior::Terminal {
                reason: "unknown:totally_made_up".to_owned()
            }
        );
    }

    #[test]
    fn unknown_reason_drift_is_terminal_and_namespaced() {
        assert_eq!(
            interpret(Some(&frame(" "))),
            CloseBehavior::Terminal {
                reason: "unknown: ".to_owned()
            }
        );
        for declared in TERMINAL_REASONS {
            assert!(
                !declared.starts_with("unknown:"),
                "declared terminal reason {declared} collides with the drift namespace"
            );
        }
    }

    #[test]
    fn terminal_reasons_stop_reconnect() {
        assert_eq!(
            interpret(Some(&frame("host_stopped"))),
            CloseBehavior::Terminal {
                reason: "host_stopped".to_owned()
            }
        );
    }

    #[test]
    fn lagging_and_capacity_reasons_are_retryable() {
        for reason in ["lagging_participant", "capacity_reached"] {
            assert_eq!(interpret(Some(&frame(reason))), CloseBehavior::Retryable);
        }
    }

    #[test]
    fn terminal_replay_gap_has_dedicated_behavior() {
        assert_eq!(
            interpret(Some(&frame(TERMINAL_REPLAY_GAP_REASON))),
            CloseBehavior::TerminalReplayGap
        );
    }

    #[test]
    fn participant_timeout_reason_stays_retryable() {
        assert_eq!(interpret(Some(&frame("timeout"))), CloseBehavior::Retryable);
    }

    #[test]
    fn runtime_recovery_retries_without_ending_remote_session() {
        assert_eq!(
            interpret(Some(&frame("session_runtime_recovering"))),
            CloseBehavior::Retryable
        );
        assert_eq!(
            interpret(Some(&frame("session_not_live"))),
            CloseBehavior::Terminal {
                reason: "session_not_live".to_owned()
            }
        );
        assert_eq!(
            interpret(Some(&frame("ended"))),
            CloseBehavior::Terminal {
                reason: "ended".to_owned()
            }
        );
    }

    #[test]
    fn declared_retryable_reasons_stay_retryable() {
        for reason in RETRYABLE_REASONS {
            assert_eq!(interpret(Some(&frame(reason))), CloseBehavior::Retryable);
        }
    }

    #[test]
    fn close_behavior_reasons_match_session_relay_authority() {
        let authority = load_session_relay_authority();
        let manifest_reasons = authority
            .close_reasons
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut behavior_reasons = BTreeSet::new();
        assert!(behavior_reasons.insert(AUTH_REVOKED_REASON));
        assert!(behavior_reasons.insert(TERMINAL_REPLAY_GAP_REASON));

        for &reason in RETRYABLE_REASONS {
            assert!(
                behavior_reasons.insert(reason),
                "duplicate Rust close-behavior reason: {reason}"
            );
            assert_ne!(
                reason, AUTH_REVOKED_REASON,
                "auth refresh and retryable behavior must not share a close reason"
            );
        }

        for &reason in TERMINAL_REASONS {
            assert!(
                behavior_reasons.insert(reason),
                "duplicate Rust close-behavior reason: {reason}"
            );
            assert_ne!(
                reason, AUTH_REVOKED_REASON,
                "auth refresh and terminal behavior must not share a close reason"
            );
        }

        for &reason in &behavior_reasons {
            assert!(
                manifest_reasons.contains(reason),
                "Rust close behavior handles \"{reason}\" but session-relay authority does not list it in closeReasons"
            );
        }

        for reason in manifest_reasons {
            assert!(
                behavior_reasons.contains(reason),
                "session-relay authority lists \"{reason}\" in closeReasons but Rust does not classify it"
            );
        }
    }
}
