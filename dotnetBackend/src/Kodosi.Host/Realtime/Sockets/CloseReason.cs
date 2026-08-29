using System.Net.WebSockets;

namespace Kodosi.Host.Realtime;

public enum CloseReason
{
    AccessRevoked,
    OwnerIdentityReset,
    InvalidSessionId,
    SessionNotLive,
    SessionRuntimeRecovering,
    SessionNotFound,
    ExpectedParticipantJoin,
    ExpectedHostHello,
    InvalidSessionOrSecret,
    CapacityReached,
    AlreadyHosted,
    InvalidSessionState,
    UnsupportedData,
    InvalidMessage,
    ClientMessagesNotSupported,
    AuthRevoked,
    LaggingParticipant,
    TerminalReplayGap,
    ClosingNormal,
    ServerRestarting,
    ServerError,
    HostStopped,
    HostMissing,
    ParticipantTimeout,
    SessionTimeout,
    SessionEnded,
}

internal static class CloseReasonWire
{
    public static string ToWire(this CloseReason reason) =>
        reason switch
        {
            CloseReason.AccessRevoked => "access_revoked",
            CloseReason.OwnerIdentityReset => "owner_identity_reset",
            CloseReason.InvalidSessionId => "invalid_session_id",
            CloseReason.SessionNotLive => "session_not_live",
            CloseReason.SessionRuntimeRecovering => "session_runtime_recovering",
            CloseReason.SessionNotFound => "session_not_found",
            CloseReason.ExpectedParticipantJoin => "expected_participant_join",
            CloseReason.ExpectedHostHello => "expected_host_hello",
            CloseReason.InvalidSessionOrSecret => "invalid_session_or_secret",
            CloseReason.CapacityReached => "capacity_reached",
            CloseReason.AlreadyHosted => "already_hosted",
            CloseReason.InvalidSessionState => "invalid_session_state",
            CloseReason.UnsupportedData => "unsupported_data",
            CloseReason.InvalidMessage => "invalid_message",
            CloseReason.ClientMessagesNotSupported => "client_messages_not_supported",
            CloseReason.AuthRevoked => "auth_revoked",
            CloseReason.LaggingParticipant => "lagging_participant",
            CloseReason.TerminalReplayGap => "terminal_replay_gap",
            CloseReason.ClosingNormal => "closing",
            CloseReason.ServerRestarting => "server_restarting",
            CloseReason.ServerError => "server_error",
            CloseReason.HostStopped => "host_stopped",
            CloseReason.HostMissing => "host_missing",
            CloseReason.ParticipantTimeout => "timeout",
            CloseReason.SessionTimeout => "session_timeout",
            CloseReason.SessionEnded => "ended",
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, "Unknown close reason."),
        };

    public static CloseReason? TryFromHostEndWire(string? value) =>
        value switch
        {
            "host_stopped" => CloseReason.HostStopped,
            _ => null,
        };
}

internal static class CloseReasonWebSocketStatus
{
    public static WebSocketCloseStatus ToWebSocketCloseStatus(this CloseReason reason) =>
        reason switch
        {
            CloseReason.InvalidSessionId
                or CloseReason.SessionNotLive
                or CloseReason.SessionNotFound
                or CloseReason.ExpectedParticipantJoin
                or CloseReason.ExpectedHostHello
                or CloseReason.InvalidSessionOrSecret
                or CloseReason.CapacityReached
                or CloseReason.AlreadyHosted
                or CloseReason.InvalidSessionState
                or CloseReason.AuthRevoked
                or CloseReason.AccessRevoked
                or CloseReason.OwnerIdentityReset
                or CloseReason.LaggingParticipant
                or CloseReason.TerminalReplayGap => WebSocketCloseStatus.PolicyViolation,
            CloseReason.UnsupportedData
                or CloseReason.InvalidMessage
                or CloseReason.ClientMessagesNotSupported => WebSocketCloseStatus.InvalidPayloadData,
            CloseReason.ServerRestarting
                or CloseReason.SessionRuntimeRecovering
                or CloseReason.ServerError => WebSocketCloseStatus.EndpointUnavailable,
            CloseReason.ClosingNormal
                or CloseReason.HostStopped
                or CloseReason.HostMissing
                or CloseReason.ParticipantTimeout
                or CloseReason.SessionTimeout
                or CloseReason.SessionEnded => WebSocketCloseStatus.NormalClosure,
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, "Unknown close reason."),
        };
}
