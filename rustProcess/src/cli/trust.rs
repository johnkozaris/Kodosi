use std::future::Future;
use std::time::Duration;

use serde::Serialize;

use crate::{
    AppError, Result, headless_host, identity_core::device_list_pin_store::DeviceListPinStore,
};
use kodosi_domain::ids::UserId;

use super::{
    args::{CliTrustAction, TrustResetArgs},
    output::OutputMode,
};

pub(in crate::cli) async fn run_trust_command(
    command: CliTrustAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliTrustAction::List => run_trust_list(output),
        CliTrustAction::Reset(args) => run_trust_reset(&args, output).await,
    }
}

#[derive(Debug, Serialize)]
struct TrustPinOutput {
    user_id: String,
    generation: u64,
    signer_device_id: String,
    device_count: usize,
    pinned_at_ms: i64,
    pinned_at: String,
}

#[derive(Debug)]
enum TrustResetLane<H, L> {
    Headless(H),
    DirectActor(L),
}

fn run_trust_list(output: OutputMode) -> Result<()> {
    let store = DeviceListPinStore::load_default()?;

    let pins: Vec<TrustPinOutput> = store
        .iter_pins()
        .map(|pin| {
            let summary = pin.proof_summary()?;
            Ok(TrustPinOutput {
                user_id: pin.user_id.clone(),
                generation: summary.generation,
                signer_device_id: summary.signer_device_id,
                device_count: summary.active_device_ids.len(),
                pinned_at_ms: pin.pinned_at_ms,
                pinned_at: format_pinned_at_ms(pin.pinned_at_ms),
            })
        })
        .collect::<Result<_>>()?;

    if output.json {
        return output.write_json(&pins);
    }

    if pins.is_empty() {
        output.write_line("No pinned device lists.")?;
        output.write_line(
            "(Pins update while the runtime is running and connected; `kodosi host start` keeps them fresh.)",
        )?;
        return Ok(());
    }

    for pin in &pins {
        output.write_line(format!(
            "{}  gen {}  ({} devices, signed by {}, pinned {})",
            pin.user_id, pin.generation, pin.device_count, pin.signer_device_id, pin.pinned_at
        ))?;
    }
    Ok(())
}

fn format_pinned_at_ms(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let nanos = u32::try_from(ms.rem_euclid(1000)).unwrap_or(0) * 1_000_000;
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .and_then(|t| t.replace_nanosecond(nanos).ok())
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| format!("ms={ms}"))
}

async fn run_trust_reset(args: &TrustResetArgs, output: OutputMode) -> Result<()> {
    let user_id = UserId::try_from(args.user.as_str())
        .map_err(|error| AppError::InvalidBackendData {
            field: "userId".to_owned(),
            reason: error.to_string(),
        })?
        .to_string();
    let cleared = match resolve_trust_reset_lane().await? {
        TrustResetLane::Headless(mut host) => host.reset_trust(&user_id).await?,
        TrustResetLane::DirectActor(_runtime_lock) => Err(AppError::Unsupported {
            reason:
                "trust reset requires an authenticated runtime so the account bucket is explicit"
                    .to_owned(),
        })?,
    };
    write_reset_result(output, &user_id, cleared)
}

async fn resolve_trust_reset_lane()
-> Result<TrustResetLane<headless_host::HeadlessHostClient, std::fs::File>> {
    resolve_trust_reset_lane_with(
        || match headless_host::acquire_runtime_lock() {
            Ok(lock) => Ok(Some(lock)),
            Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(None)
            }
            Err(error) => Err(error),
        },
        headless_host::connect_existing_or_replace_incompatible_host,
        || tokio::time::sleep(Duration::from_millis(50)),
        Duration::from_secs(30),
    )
    .await
}

async fn resolve_trust_reset_lane_with<H, L, Connect, ConnectFuture, TryLock, Wait, WaitFuture>(
    mut try_lock: TryLock,
    mut connect: Connect,
    mut wait: Wait,
    timeout: Duration,
) -> Result<TrustResetLane<H, L>>
where
    Connect: FnMut() -> ConnectFuture,
    ConnectFuture: Future<Output = Result<Option<H>>>,
    TryLock: FnMut() -> Result<Option<L>>,
    Wait: FnMut() -> WaitFuture,
    WaitFuture: Future<Output = ()>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_connect_error = None;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(trust_reset_lane_timeout(last_connect_error));
        }
        if let Some(lock) = try_lock()? {
            return Ok(TrustResetLane::DirectActor(lock));
        }
        let mut connect_attempt = std::pin::pin!(connect());
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => {
                    return Err(trust_reset_lane_timeout(last_connect_error));
                }
                result = &mut connect_attempt => {
                    match result {
                        Ok(Some(host)) => return Ok(TrustResetLane::Headless(host)),
                        Ok(None) => {}
                        Err(error) => last_connect_error = Some(error),
                    }
                    break;
                }
                () = wait() => {
                    if let Some(lock) = try_lock()? {
                        return Ok(TrustResetLane::DirectActor(lock));
                    }
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => {
                return Err(trust_reset_lane_timeout(last_connect_error));
            }
            () = wait() => {}
        }
    }
}

fn trust_reset_lane_timeout(last_connect_error: Option<AppError>) -> AppError {
    AppError::Unsupported {
        reason: last_connect_error.map_or_else(
            || "timed out waiting for the runtime lock or a compatible headless host".to_owned(),
            |error| {
                format!(
                    "timed out waiting for the runtime lock or a compatible headless host; \
                     last connection error: {error}"
                )
            },
        ),
    }
}

fn write_reset_result(output: OutputMode, user_id: &str, cleared: bool) -> Result<()> {
    if output.json {
        return output.write_json(&reset_result_payload(user_id, cleared));
    }
    output.write_line(reset_result_line(user_id, cleared))
}

fn reset_result_payload(user_id: &str, cleared: bool) -> serde_json::Value {
    serde_json::json!({
        "user": user_id,
        "cleared": cleared,
    })
}

fn reset_result_line(user_id: &str, cleared: bool) -> String {
    if cleared {
        format!("Cleared pinned device list for {user_id}.")
    } else {
        format!("No pin was stored for {user_id}.")
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, future, time::Duration};

    use super::{
        TrustResetLane, reset_result_line, reset_result_payload, resolve_trust_reset_lane_with,
    };
    use crate::AppError;

    #[tokio::test]
    async fn trust_reset_prefers_headless_lane_while_runtime_lock_is_held() {
        let lane = resolve_trust_reset_lane_with(
            || Ok::<_, AppError>(None::<&str>),
            || future::ready(Ok(Some("host"))),
            || future::ready(()),
            Duration::from_secs(1),
        )
        .await
        .expect("headless lane");
        std::assert_matches!(lane, TrustResetLane::Headless("host"));
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_host_race_falls_back_to_direct_actor_while_connect_is_stalled() {
        let locks = Cell::new(0);
        let lane = resolve_trust_reset_lane_with(
            || {
                let probe = locks.get();
                locks.set(probe + 1);
                Ok::<_, AppError>((probe >= 1).then_some("runtime-lock"))
            },
            future::pending::<crate::Result<Option<&str>>>,
            || tokio::time::sleep(Duration::from_millis(10)),
            Duration::from_secs(1),
        )
        .await
        .expect("direct actor after stop");

        std::assert_matches!(lane, TrustResetLane::DirectActor("runtime-lock"));
        assert_eq!(locks.get(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_host_resolution_honors_one_wall_clock_deadline() {
        let started = tokio::time::Instant::now();
        let error = resolve_trust_reset_lane_with(
            || Ok::<_, AppError>(None::<&str>),
            future::pending::<crate::Result<Option<&str>>>,
            || tokio::time::sleep(Duration::from_millis(10)),
            Duration::from_millis(50),
        )
        .await
        .expect_err("stalled host must time out");

        assert_eq!(
            tokio::time::Instant::now().duration_since(started),
            Duration::from_millis(50)
        );
        std::assert_matches!(
            error,
            AppError::Unsupported { reason } if reason.contains("timed out")
        );
    }

    #[test]
    fn trust_reset_json_and_human_output_share_the_same_cleared_truth() {
        for (cleared, expected_line) in [
            (true, "Cleared pinned device list for user-1."),
            (false, "No pin was stored for user-1."),
        ] {
            assert_eq!(
                reset_result_payload("user-1", cleared)["cleared"].as_bool(),
                Some(cleared)
            );
            assert_eq!(reset_result_line("user-1", cleared), expected_line);
        }
    }
}
