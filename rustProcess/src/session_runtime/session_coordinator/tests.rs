use super::{
    RESIZE_COALESCE_WINDOW, ResizeCoalescer, provider_resume_arguments, wait_resize_deadline,
};
use kodosi_domain::{
    provider_conversation::{ProviderConversationIdentity, ProviderConversationProvider},
    terminal::TerminalSize,
};
use tokio::time::{self, Duration, Instant};

fn size(rows: u16, cols: u16) -> TerminalSize {
    TerminalSize::new(rows, cols).unwrap_or_else(|error| panic!("fixed size: {error}"))
}

#[test]
fn provider_resume_arguments_are_typed_and_shell_free() {
    let id = "01900000-0000-4000-8000-000000000001";
    assert_eq!(
        provider_resume_arguments(&ProviderConversationIdentity {
            provider: ProviderConversationProvider::Claude,
            native_conversation_id: id.to_owned(),
        }),
        ["--resume", id]
    );
    assert_eq!(
        provider_resume_arguments(&ProviderConversationIdentity {
            provider: ProviderConversationProvider::Copilot,
            native_conversation_id: id.to_owned(),
        }),
        [format!("--resume={id}")]
    );
}

#[test]
fn a_burst_keeps_only_the_newest_size() {
    let mut coalescer = ResizeCoalescer::default();
    let now = Instant::now();

    coalescer.record(size(24, 80), now);
    coalescer.record(size(30, 100), now);
    coalescer.record(size(40, 120), now);

    assert_eq!(coalescer.take(), Some(size(40, 120)));
    assert!(!coalescer.is_pending());
    assert!(coalescer.deadline().is_none());
}

#[test]
fn a_continuous_burst_cannot_push_the_deadline_out() {
    let mut coalescer = ResizeCoalescer::default();
    let start = Instant::now();

    coalescer.record(size(24, 80), start);
    let first_deadline = coalescer.deadline();
    coalescer.record(size(30, 100), start + Duration::from_millis(10));
    coalescer.record(size(40, 120), start + Duration::from_millis(20));

    assert_eq!(first_deadline, Some(start + RESIZE_COALESCE_WINDOW));
    assert_eq!(coalescer.deadline(), first_deadline);
}

#[tokio::test(start_paused = true)]
async fn an_idle_sessions_resize_fires_before_the_metadata_tick() {
    let mut coalescer = ResizeCoalescer::default();
    coalescer.record(size(24, 80), Instant::now());
    let started = Instant::now();

    wait_resize_deadline(coalescer.deadline()).await;

    let waited = started.elapsed();
    assert_eq!(waited, RESIZE_COALESCE_WINDOW);
    assert!(waited < super::RUNTIME_METADATA_INTERVAL);
    assert_eq!(coalescer.take(), Some(size(24, 80)));
}

#[tokio::test(start_paused = true)]
async fn an_absent_deadline_parks_instead_of_spinning() {
    let result = time::timeout(Duration::from_mins(1), wait_resize_deadline(None)).await;
    assert!(result.is_err());
}
