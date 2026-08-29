use crate::{
    AppError, Result, local_sessions::ops::LocalResizeOutcome, runtime::Runtime,
    session_runtime::commands::SessionInput,
};
use kodosi_domain::{
    ids::SessionId,
    terminal::{TerminalPixelGeometry, TerminalSize},
};

impl Runtime {
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the size-authority claim must remain held until the correlated PTY outcome settles"
    )]
    pub(crate) async fn send_confirmed_local_input(
        &mut self,
        id: SessionId,
        input: SessionInput,
    ) -> Result<()> {
        let authority = self.state.local.owned_session_runtimes.size_authority(id);
        let mut claim = match authority.as_ref() {
            Some(cell) if crate::local_sessions::size_authority::SizeAuthorityCell::input_claims_authority(&input) => {
                Some(cell.begin_claim(crate::local_sessions::size_authority::SizeOrigin::Local).await)
            }
            _ => None,
        };
        let reclaim_size = claim.as_ref().and_then(|claim| match claim.preview() {
            crate::local_sessions::size_authority::ClaimPreview::Reclaim(size) => size,
            crate::local_sessions::size_authority::ClaimPreview::Unavailable
            | crate::local_sessions::size_authority::ClaimPreview::AlreadyHeld => None,
        });
        let runtime = self
            .state
            .local
            .owned_session_runtimes
            .runtime_handle(id)
            .ok_or(AppError::NoActiveSession)?;
        let completion = runtime.begin_confirmed_input(input, reclaim_size).await?;
        let result = self.await_coordinator_mutation(completion).await;
        match result {
            Ok(()) => {
                if let Some(claim) = claim.take() {
                    claim.commit();
                }
                Ok(())
            }
            Err(error @ (AppError::Unsupported { .. } | AppError::DeliveryUnknown { .. })) => {
                Err(error)
            }
            Err(error) => Err(AppError::DeliveryUnknown {
                reason: format!(
                    "confirmed input was admitted but its PTY outcome is unknown: {error}"
                ),
            }),
        }
    }

    pub(crate) async fn claim_size_and_resize(
        &mut self,
        id: SessionId,
        action_id: String,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<LocalResizeOutcome> {
        if self.state.local.sessions.record(id).is_some() {
            self.state.local.last_terminal_size = size;
            let authority = self.state.local.owned_session_runtimes.size_authority(id);
            let mut claim = match authority.as_ref() {
                Some(cell) => Some(
                    cell.begin_claim(crate::local_sessions::size_authority::SizeOrigin::Local)
                        .await,
                ),
                None => None,
            };
            let runtime = self
                .state
                .local
                .owned_session_runtimes
                .runtime_handle(id)
                .ok_or(AppError::NoActiveSession)?;
            let completion = runtime.begin_resize(size, pixel_geometry).await?;
            self.await_coordinator_mutation(completion).await?;
            let _ = authority.as_ref().map(|cell| {
                cell.admit_resize(
                    crate::local_sessions::size_authority::SizeOrigin::Local,
                    size,
                )
            });
            if let Some(claim) = claim.take() {
                claim.commit();
            }
            Ok(LocalResizeOutcome::Applied)
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            let relay_generation = self.state.remote.session_relays.generation(id);
            crate::runtime::remote_sessions::claim_size(self, id, action_id, size, pixel_geometry)?;
            Ok(LocalResizeOutcome::PendingRemote { relay_generation })
        } else if self.state.discovery.session(id).is_some() {
            Ok(LocalResizeOutcome::RejectedRemoteOwnerOnly)
        } else {
            Err(AppError::NoActiveSession)
        }
    }

    pub(crate) async fn resize_session(
        &mut self,
        id: SessionId,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<LocalResizeOutcome> {
        if self.state.local.sessions.record(id).is_some() {
            self.state.local.last_terminal_size = size;
            let authority = self.state.local.owned_session_runtimes.size_authority(id);
            if authority.as_ref().is_some_and(|cell| {
                !cell.admit_resize(
                    crate::local_sessions::size_authority::SizeOrigin::Local,
                    size,
                )
            }) {
                return Ok(LocalResizeOutcome::RejectedByAuthority);
            }
            let runtime = self
                .state
                .local
                .owned_session_runtimes
                .runtime_handle(id)
                .ok_or(AppError::NoActiveSession)?;
            let completion = runtime.begin_resize(size, pixel_geometry).await?;
            self.await_coordinator_mutation(completion).await?;
            Ok(LocalResizeOutcome::Applied)
        } else if self.state.discovery.session(id).is_some() {
            Ok(LocalResizeOutcome::RejectedRemoteOwnerOnly)
        } else {
            Err(AppError::NoActiveSession)
        }
    }
}
