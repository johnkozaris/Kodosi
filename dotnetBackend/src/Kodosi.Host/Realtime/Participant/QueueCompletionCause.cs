namespace Kodosi.Host.Realtime;

internal enum QueueCompletionCause
{
    Normal,
    Lagging,
    AccessRevokedCascade,
    AccessRefresh,
    Drain,
    Timeout,
    SessionEnd,
}

internal static class QueueCompletionCauseCloseReason
{
    public static CloseReason ToCloseReason(this QueueCompletionCause cause) =>
        cause switch
        {
            QueueCompletionCause.Normal => CloseReason.ClosingNormal,
            QueueCompletionCause.Lagging => CloseReason.LaggingParticipant,
            QueueCompletionCause.AccessRevokedCascade => CloseReason.AccessRevoked,
            QueueCompletionCause.AccessRefresh => CloseReason.ClosingNormal,
            QueueCompletionCause.Drain => CloseReason.ServerRestarting,
            QueueCompletionCause.Timeout => CloseReason.ParticipantTimeout,
            QueueCompletionCause.SessionEnd => CloseReason.SessionEnded,
            _ => throw new ArgumentOutOfRangeException(nameof(cause), cause, "Unknown queue completion cause."),
        };
}
