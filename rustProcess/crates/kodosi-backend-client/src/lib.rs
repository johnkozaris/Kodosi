#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-None/Err for failure clarity"
    )
)]

pub mod api;
pub mod artifact_endorsement;
pub mod auth;
mod backoff;
mod close_reasons;
pub mod config;
pub mod control;
pub mod crypto;
mod dto;
mod endpoint;
mod error;
pub mod host_ws;
pub mod http_client;
pub mod labels;
pub mod relay;
mod relay_wire;
mod session_key_access;
pub mod session_key_service;
pub mod session_relay;
mod session_relay_authority;
mod terminal_wire;
pub mod user_events;
pub mod user_events_ws;

pub use endpoint::BackendOrigin;
pub use error::{BackendClientError, RelayReservationKind, Result};
