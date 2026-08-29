#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-None/Err for failure clarity"
    )
)]

pub mod auth;
pub mod device_link;
pub mod domain_tags;
pub mod ids;
pub mod lifecycle;
pub mod permissions;
pub mod provider_conversation;
pub mod session;
pub mod terminal;
pub mod user;
