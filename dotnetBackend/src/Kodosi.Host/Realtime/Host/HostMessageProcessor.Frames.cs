using System.Buffers.Binary;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;




internal sealed partial class HostMessageProcessor
{
    private const int TerminalCheckpointHeaderBytes = 1 + 4 + 8 + 8 + 8;
    private const int TerminalRawBatchHeaderBytes = 1 + 4 + 8 + 8 + 8;
    private const int TerminalPresentationHeaderBytes = 1 + 4 + 8 + 8;
    private const int PendingPermissionsHeaderBytes = 1 + 4 + 8 + 8 + 16 + 16;

    public async ValueTask<HostProcessOutcome> ProcessBinaryAsync(
        byte[] rawBytes,
        HostMessageSource source,
        CancellationToken ct)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(
            source.SessionId,
            ct);
        if (!IsCurrent(source))
        {
            return StaleHost();
        }

        return ProcessBinaryCore(rawBytes, source);
    }

    private HostProcessOutcome ProcessBinaryCore(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        if (rawBytes.Length == 0)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.UnsupportedData);
        }

        return rawBytes[0] switch
        {
            TerminalCheckpointFrameTypeByte => ProcessTerminalCheckpoint(rawBytes, identity),
            TerminalRawBatchFrameTypeByte => ProcessTerminalRawBatch(rawBytes, identity),
            TerminalPresentationFrameTypeByte => ProcessTerminalPresentation(rawBytes, identity),
            PendingPermissionsFrameTypeByte => ProcessPendingPermissions(rawBytes, identity),
            _ => UnsupportedBinaryType(rawBytes[0], identity.SessionId),
        };
    }

    private HostProcessOutcome ProcessTerminalCheckpoint(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        if (!RelayMessageLimits.IsWithinLimit("term.semanticCheckpoint", rawBytes.Length)
            || !TryReadTerminalRoute(
                rawBytes,
                TerminalCheckpointHeaderBytes,
                LiveTerminalFrameKind.SemanticCheckpoint,
                identity,
                out var keyGen))
        {
            return TerminalRouteRejected(identity.SessionId, "checkpoint");
        }

        var checkpointRevision = BinaryPrimitives.ReadUInt64BigEndian(
            rawBytes.AsSpan(13, 8));
        var nextSequence = BinaryPrimitives.ReadUInt64BigEndian(
            rawBytes.AsSpan(21, 8));
        var store = identity.Runtime.Stream.TryStoreEncryptedCheckpoint(
            checkpointRevision,
            nextSequence,
            rawBytes);
        if (store == LiveTerminalFrameStoreOutcome.Rejected)
        {

            _logger.LogWarning(
                "Dropped stale encrypted checkpoint for session {SessionId}: keyGen={KeyGen} checkpointRevision={CheckpointRevision} nextSequence={NextSequence}",
                identity.SessionId,
                keyGen,
                checkpointRevision,
                nextSequence);
            return HostProcessOutcome.Continue;
        }
        if (store == LiveTerminalFrameStoreOutcome.Duplicate)
        {
            return HostProcessOutcome.Continue;
        }

        return BroadcastTerminalFrame(identity, rawBytes);
    }

    private HostProcessOutcome ProcessTerminalRawBatch(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        if (!RelayMessageLimits.IsWithinLimit("term.rawBatch", rawBytes.Length)
            || !TryReadTerminalRoute(
                rawBytes,
                TerminalRawBatchHeaderBytes,
                LiveTerminalFrameKind.RawBatch,
                identity,
                out var keyGen))
        {
            return TerminalRouteRejected(identity.SessionId, "raw batch");
        }

        var firstSequence = BinaryPrimitives.ReadUInt64BigEndian(
            rawBytes.AsSpan(13, 8));
        var nextSequence = BinaryPrimitives.ReadUInt64BigEndian(
            rawBytes.AsSpan(21, 8));
        var store = identity.Runtime.Stream.TryStoreRawBatch(
            firstSequence,
            nextSequence,
            rawBytes);
        if (store == LiveTerminalFrameStoreOutcome.Duplicate)
        {
            return HostProcessOutcome.Continue;
        }
        if (store == LiveTerminalFrameStoreOutcome.Rejected)
        {
            _logger.LogWarning(
                "Invalid encrypted raw batch route for session {SessionId}: keyGen={KeyGen} firstSequence={FirstSequence} nextSequence={NextSequence}",
                identity.SessionId,
                keyGen,
                firstSequence,
                nextSequence);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        return BroadcastTerminalFrame(identity, rawBytes);
    }

    private HostProcessOutcome ProcessTerminalPresentation(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        if (!RelayMessageLimits.IsWithinLimit("term.presentation", rawBytes.Length)
            || !TryReadTerminalRoute(
                rawBytes,
                TerminalPresentationHeaderBytes,
                LiveTerminalFrameKind.Presentation,
                identity,
                out var keyGen))
        {
            return TerminalRouteRejected(identity.SessionId, "presentation");
        }

        var presentationRevision = BinaryPrimitives.ReadUInt64BigEndian(
            rawBytes.AsSpan(13, 8));
        var store = identity.Runtime.Stream.TryStoreEncryptedPresentation(
            presentationRevision,
            rawBytes);
        if (store == LiveTerminalFrameStoreOutcome.Rejected)
        {
            _logger.LogWarning(
                "Dropped stale encrypted presentation for session {SessionId}: keyGen={KeyGen} presentationRevision={PresentationRevision}",
                identity.SessionId,
                keyGen,
                presentationRevision);
            return HostProcessOutcome.Continue;
        }
        if (store == LiveTerminalFrameStoreOutcome.Duplicate)
        {
            return HostProcessOutcome.Continue;
        }

        return BroadcastTerminalFrame(identity, rawBytes);
    }

    private HostProcessOutcome ProcessPendingPermissions(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        if (!RelayMessageLimits.IsWithinLimit("permission.pendingSnapshot", rawBytes.Length)
            || rawBytes.Length <= PendingPermissionsHeaderBytes)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.UnsupportedData);
        }
        var keyGen = BinaryPrimitives.ReadUInt32BigEndian(rawBytes.AsSpan(1, 4));
        var counter = BinaryPrimitives.ReadUInt64BigEndian(rawBytes.AsSpan(5, 8));
        var generation = BinaryPrimitives.ReadUInt64BigEndian(rawBytes.AsSpan(13, 8));
        var sessionId = new Guid(rawBytes.AsSpan(21, 16), bigEndian: true);
        var incarnationId = new Guid(rawBytes.AsSpan(37, 16), bigEndian: true);
        if (generation == 0
            || sessionId != identity.SessionId.Value
            || incarnationId != identity.SessionIncarnationId
            || !identity.Runtime.Stream.TryAdmitPendingPermissionsFrame(keyGen, counter))
        {
            _metrics.RecordFrameAdmissionRejected();
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        var outcome = identity.Runtime.Stream.TryStorePendingPermissionsSnapshot(
            incarnationId,
            generation,
            rawBytes);
        if (outcome is LiveTerminalFrameStoreOutcome.Rejected
            or LiveTerminalFrameStoreOutcome.Duplicate)
        {
            return HostProcessOutcome.Continue;
        }
        if (!IsCurrent(identity)
            || !_broadcaster.BroadcastPendingPermissionsIfSame(
                identity.SessionId,
                identity.Queues,
                WireMessage.EncryptedBinary(rawBytes)))
        {
            return StaleHost();
        }
        return HostProcessOutcome.Continue;
    }

    private bool TryReadTerminalRoute(
        byte[] rawBytes,
        int headerBytes,
        LiveTerminalFrameKind kind,
        HostMessageSource identity,
        out uint keyGen)
    {
        keyGen = 0;
        if (rawBytes.Length < headerBytes)
        {
            _logger.LogWarning(
                "Encrypted terminal frame too short ({Length} bytes) for session {SessionId}",
                rawBytes.Length,
                identity.SessionId);
            return false;
        }

        keyGen = BinaryPrimitives.ReadUInt32BigEndian(rawBytes.AsSpan(1, 4));
        var counter = BinaryPrimitives.ReadUInt64BigEndian(rawBytes.AsSpan(5, 8));
        if (identity.Runtime.Stream.TryAdmitTerminalFrame(kind, keyGen, counter))
        {
            return true;
        }

        _metrics.RecordFrameAdmissionRejected();
        _logger.LogWarning(
            "Dropped encrypted terminal {FrameKind} frame for session {SessionId}: keyGen={KeyGen} counter={Counter} rejected by terminal admission gate",
            kind,
            identity.SessionId,
            keyGen,
            counter);
        return false;
    }

    private HostProcessOutcome BroadcastTerminalFrame(
        HostMessageSource identity,
        byte[] rawBytes)
    {
        if (!IsCurrent(identity)
            || !_broadcaster.BroadcastToDownstreamClientsIfSame(
                identity.SessionId,
                identity.Queues,
                WireMessage.EncryptedBinary(rawBytes)))
        {
            return StaleHost();
        }
        return HostProcessOutcome.Continue;
    }

    private HostProcessOutcome TerminalRouteRejected(
        SessionId sessionId,
        string frameKind)
    {
        _logger.LogWarning(
            "Rejected encrypted terminal {FrameKind} routing header for session {SessionId}",
            frameKind,
            sessionId);
        return HostProcessOutcome.ProtocolViolation(CloseReason.UnsupportedData);
    }

    private HostProcessOutcome UnsupportedBinaryType(byte type, SessionId sessionId)
    {
        _logger.LogWarning(
            "Unknown binary message type 0x{Type:X2} for session {SessionId}",
            type,
            sessionId);
        return HostProcessOutcome.ProtocolViolation(CloseReason.UnsupportedData);
    }
}
