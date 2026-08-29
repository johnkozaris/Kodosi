use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use kodosi_domain::terminal::TerminalSize;

use crate::session_runtime::commands::SessionInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimPreview {
    Unavailable,
    AlreadyHeld,
    Reclaim(Option<TerminalSize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizeOrigin {
    Local,
    Remote,
}

const CLAIM_IDLE_TTL: Duration = Duration::from_secs(45);

#[derive(Debug, Default)]
struct Inner {
    authority: Option<SizeOrigin>,
    claimed_at: Option<Instant>,
    last_local: Option<TerminalSize>,
    last_remote: Option<TerminalSize>,
}

impl Inner {
    fn expire_stale_claim(&mut self, now: Instant) {
        if let Some(at) = self.claimed_at
            && now.duration_since(at) >= CLAIM_IDLE_TTL
        {
            self.authority = None;
            self.claimed_at = None;
        }
    }
}

impl Inner {
    fn last_mut(&mut self, origin: SizeOrigin) -> &mut Option<TerminalSize> {
        match origin {
            SizeOrigin::Local => &mut self.last_local,
            SizeOrigin::Remote => &mut self.last_remote,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SizeAuthorityCell {
    inner: Mutex<Inner>,
    claim_gate: tokio::sync::Mutex<()>,
}

pub(crate) struct SizeClaim<'a> {
    cell: &'a SizeAuthorityCell,
    _guard: tokio::sync::MutexGuard<'a, ()>,
    origin: SizeOrigin,
    preview: ClaimPreview,
}

impl SizeClaim<'_> {
    pub(crate) const fn preview(&self) -> ClaimPreview {
        self.preview
    }

    pub(crate) fn commit(self) {
        self.cell.commit_claim(self.origin);
    }
}

impl SizeAuthorityCell {
    pub(crate) async fn begin_claim(&self, origin: SizeOrigin) -> SizeClaim<'_> {
        let guard = self.claim_gate.lock().await;
        let preview = self.preview_claim(origin);
        SizeClaim {
            cell: self,
            _guard: guard,
            origin,
            preview,
        }
    }

    pub(crate) fn admit_resize(&self, origin: SizeOrigin, size: TerminalSize) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return true;
        };
        inner.expire_stale_claim(Instant::now());
        *inner.last_mut(origin) = Some(size);
        inner.authority.is_none_or(|holder| holder == origin)
    }

    pub(crate) fn release_remote(&self) {
        if let Ok(mut inner) = self.inner.lock()
            && inner.authority == Some(SizeOrigin::Remote)
        {
            inner.authority = None;
            inner.claimed_at = None;
        }
    }

    fn preview_claim(&self, origin: SizeOrigin) -> ClaimPreview {
        let Ok(mut inner) = self.inner.lock() else {
            return ClaimPreview::Unavailable;
        };
        inner.expire_stale_claim(Instant::now());
        if inner.authority == Some(origin) {
            return ClaimPreview::AlreadyHeld;
        }
        ClaimPreview::Reclaim(*inner.last_mut(origin))
    }

    fn commit_claim(&self, origin: SizeOrigin) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let now = Instant::now();
        inner.expire_stale_claim(now);
        inner.authority = Some(origin);
        inner.claimed_at = Some(now);
    }

    pub(crate) fn input_claims_authority(input: &SessionInput) -> bool {
        !input.as_bytes().is_empty()
    }

    #[cfg(test)]
    fn backdate_claim(&self) {
        if let Ok(mut inner) = self.inner.lock()
            && inner.claimed_at.is_some()
        {
            inner.claimed_at = Some(
                Instant::now()
                    .checked_sub(CLAIM_IDLE_TTL)
                    .unwrap_or_else(Instant::now),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(rows: u16, cols: u16) -> TerminalSize {
        TerminalSize::new(rows, cols).unwrap_or_else(|error| panic!("size: {error}"))
    }

    async fn claim(cell: &SizeAuthorityCell, origin: SizeOrigin) -> ClaimPreview {
        let claim = cell.begin_claim(origin).await;
        let preview = claim.preview();
        claim.commit();
        preview
    }

    #[test]
    fn unclaimed_applies_all_resizes() {
        let cell = SizeAuthorityCell::default();
        assert!(cell.admit_resize(SizeOrigin::Local, size(24, 80)));
        assert!(cell.admit_resize(SizeOrigin::Remote, size(40, 120)));
    }

    #[tokio::test]
    async fn claim_gates_the_other_origin_and_reclaim_returns_last_size() {
        let cell = SizeAuthorityCell::default();
        assert_eq!(
            claim(&cell, SizeOrigin::Local).await,
            ClaimPreview::Reclaim(None),
            "no size seen yet"
        );

        assert!(cell.admit_resize(SizeOrigin::Local, size(30, 100)));
        assert!(!cell.admit_resize(SizeOrigin::Remote, size(40, 120)));

        assert_eq!(
            claim(&cell, SizeOrigin::Remote).await,
            ClaimPreview::Reclaim(Some(size(40, 120)))
        );
        assert!(cell.admit_resize(SizeOrigin::Remote, size(41, 121)));
        assert!(!cell.admit_resize(SizeOrigin::Local, size(30, 100)));

        assert_eq!(
            claim(&cell, SizeOrigin::Remote).await,
            ClaimPreview::AlreadyHeld
        );

        assert_eq!(
            claim(&cell, SizeOrigin::Local).await,
            ClaimPreview::Reclaim(Some(size(30, 100)))
        );
    }

    #[tokio::test]
    async fn unshare_releases_remote_claim_but_not_local() {
        let cell = SizeAuthorityCell::default();
        assert!(cell.admit_resize(SizeOrigin::Remote, size(40, 120)));
        assert_eq!(
            claim(&cell, SizeOrigin::Remote).await,
            ClaimPreview::Reclaim(Some(size(40, 120)))
        );
        assert!(!cell.admit_resize(SizeOrigin::Local, size(24, 80)));

        cell.release_remote();
        assert!(cell.admit_resize(SizeOrigin::Local, size(24, 80)));

        assert_eq!(
            claim(&cell, SizeOrigin::Local).await,
            ClaimPreview::Reclaim(Some(size(24, 80)))
        );
        cell.release_remote();
        assert!(!cell.admit_resize(SizeOrigin::Remote, size(40, 120)));
    }

    #[tokio::test]
    async fn idle_claim_decays_so_an_idle_holder_never_blocks() {
        let cell = SizeAuthorityCell::default();
        assert!(cell.admit_resize(SizeOrigin::Local, size(30, 100)));
        assert_eq!(
            claim(&cell, SizeOrigin::Local).await,
            ClaimPreview::Reclaim(Some(size(30, 100)))
        );
        assert!(!cell.admit_resize(SizeOrigin::Remote, size(40, 120)));

        cell.backdate_claim();
        assert!(cell.admit_resize(SizeOrigin::Remote, size(40, 120)));

        let _ = claim(&cell, SizeOrigin::Remote).await;
        assert!(!cell.admit_resize(SizeOrigin::Local, size(30, 100)));
    }

    #[tokio::test]
    async fn dropped_claim_does_not_change_authority() {
        let cell = SizeAuthorityCell::default();
        claim(&cell, SizeOrigin::Local).await;

        let remote = cell.begin_claim(SizeOrigin::Remote).await;
        assert_eq!(remote.preview(), ClaimPreview::Reclaim(None));
        drop(remote);

        assert!(cell.admit_resize(SizeOrigin::Local, size(30, 100)));
        assert!(!cell.admit_resize(SizeOrigin::Remote, size(40, 120)));
    }

    #[test]
    fn every_nonempty_delivered_input_claims_size_authority() {
        for bytes in [
            b"\x1b[I".as_slice(),
            b"\x1b[?1004;1$y".as_slice(),
            b"\x1bP1$r0m\x1b\\".as_slice(),
            &[0, 0xff],
        ] {
            assert!(SizeAuthorityCell::input_claims_authority(
                &SessionInput::new(bytes.to_vec())
            ));
        }
        assert!(!SizeAuthorityCell::input_claims_authority(
            &SessionInput::new(Vec::new())
        ));
    }
}
