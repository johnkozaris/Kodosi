using Kodosi.Application;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal enum SessionReplayQueueOutcome
{
    Complete,
    QueueFailure,
    TerminalGap,
}

internal readonly record struct TerminalReplayPlan(
    bool ReplayCheckpoint,
    int RawBatchStartIndex);

internal sealed class SessionReplaySender(OperationalMetrics metrics)
{
    private readonly OperationalMetrics _metrics = metrics;

    public SessionReplayQueueOutcome QueueReplay(
        LiveSessionReplayState replayState,
        RelayClientSendQueue sendQueue,
        ulong checkpointRevision,
        ulong presentationRevision,
        ulong nextSequence,
        uint lastSeenKeyGeneration)
    {
        if (!QueueKeyRotationAndTerminalReplay(
                replayState,
                sendQueue,
                checkpointRevision,
                presentationRevision,
                nextSequence,
                lastSeenKeyGeneration))
        {
            return SessionReplayQueueOutcome.TerminalGap;
        }
        if (sendQueue.CanReceivePendingPermissions
            && replayState.PendingPermissions is { } pending
            && sendQueue.TryEnqueuePendingPermissionsSnapshot(
                WireMessage.EncryptedBinary(pending.EncryptedBlob))
                != RelayClientSendQueueWriteOutcome.Enqueued)
        {
            return SessionReplayQueueOutcome.QueueFailure;
        }
        return SessionReplayQueueOutcome.Complete;
    }

    private bool QueueKeyRotationAndTerminalReplay(
        LiveSessionReplayState replayState,
        RelayClientSendQueue sendQueue,
        ulong checkpointRevision,
        ulong presentationRevision,
        ulong nextSequence,
        uint lastSeenKeyGeneration)
    {
        var terminalMessages = new List<WireMessage>();
        if (replayState.KeyRotation is { } keyRotation
            && replayState.CurrentKeyGeneration > lastSeenKeyGeneration)
        {
            terminalMessages.Add(WireMessage.Json(keyRotation));
        }

        var plan = PlanTerminalReplay(replayState, checkpointRevision, nextSequence);
        if (plan is null)
        {
            AddPresentation(replayState, terminalMessages, presentationRevision);
            _ = sendQueue.TryQueueTerminalReplay(terminalMessages);
            return false;
        }

        if (plan.Value.ReplayCheckpoint)
        {
            if (checkpointRevision > 0 || nextSequence > 0)
            {
                _metrics.RecordResync();
            }
            terminalMessages.Add(WireMessage.EncryptedBinary(
                replayState.TerminalCheckpoint!.EncryptedBlob));
        }

        for (var index = plan.Value.RawBatchStartIndex;
             index < replayState.TerminalRawBatches.Count;
             index++)
        {
            terminalMessages.Add(WireMessage.EncryptedBinary(
                replayState.TerminalRawBatches[index].EncryptedBlob));
        }

        AddPresentation(replayState, terminalMessages, presentationRevision);

        return sendQueue.TryQueueTerminalReplay(terminalMessages)
            == RelayClientSendQueueWriteOutcome.Enqueued;
    }

    private static void AddPresentation(
        LiveSessionReplayState replayState,
        ICollection<WireMessage> messages,
        ulong presentationRevision)
    {
        if (replayState.TerminalPresentation is not { } presentation
            || presentation.PresentationRevision <= presentationRevision)
        {
            return;
        }

        messages.Add(WireMessage.EncryptedBinary(presentation.EncryptedBlob));
    }

    private static TerminalReplayPlan? PlanTerminalReplay(
        LiveSessionReplayState replayState,
        ulong checkpointRevision,
        ulong nextSequence)
    {
        var checkpoint = replayState.TerminalCheckpoint;
        var rawBatches = replayState.TerminalRawBatches;
        var retainedHighWater = rawBatches.Count == 0
            ? checkpoint?.NextSequence ?? nextSequence
            : rawBatches[^1].NextSequence;
        if (nextSequence > retainedHighWater
            && (checkpoint is null
                || checkpoint.CheckpointRevision <= checkpointRevision))
        {
            return null;
        }
        var replayCheckpoint = checkpoint is not null
            && (checkpoint.CheckpointRevision > checkpointRevision
                || nextSequence < checkpoint.NextSequence);
        var requestedSequence = replayCheckpoint
            ? checkpoint!.NextSequence
            : nextSequence;

        var directStart = FindRawBatchStart(rawBatches, requestedSequence);
        if (directStart is not null
            && (requestedSequence >= retainedHighWater
                || IsContiguousThroughHighWater(
                    rawBatches,
                    directStart.Value,
                    requestedSequence,
                    retainedHighWater)))
        {
            return new TerminalReplayPlan(replayCheckpoint, directStart.Value);
        }

        if (!replayCheckpoint
            && checkpoint is not null
            && checkpoint.NextSequence <= retainedHighWater)
        {
            var checkpointStart = FindRawBatchStart(rawBatches, checkpoint.NextSequence);
            if (checkpointStart is not null
                && IsContiguousThroughHighWater(
                    rawBatches,
                    checkpointStart.Value,
                    checkpoint.NextSequence,
                    retainedHighWater))
            {
                return new TerminalReplayPlan(true, checkpointStart.Value);
            }
        }

        return null;
    }

    private static int? FindRawBatchStart(
        IReadOnlyList<LiveTerminalRawBatchReplay> rawBatches,
        ulong requestedSequence)
    {
        for (var index = 0; index < rawBatches.Count; index++)
        {
            var batch = rawBatches[index];
            if (batch.FirstSequence == requestedSequence)
            {
                return index;
            }
            if (batch.FirstSequence > requestedSequence
                || batch.NextSequence > requestedSequence)
            {
                return null;
            }
        }
        return rawBatches.Count;
    }

    private static bool IsContiguousThroughHighWater(
        IReadOnlyList<LiveTerminalRawBatchReplay> rawBatches,
        int startIndex,
        ulong requestedSequence,
        ulong retainedHighWater)
    {
        var expected = requestedSequence;
        for (var index = startIndex; index < rawBatches.Count; index++)
        {
            var batch = rawBatches[index];
            if (batch.FirstSequence != expected)
            {
                return false;
            }
            expected = batch.NextSequence;
        }
        return expected == retainedHighWater;
    }
}
