using Kodosi.Application;

namespace Kodosi.Infrastructure.Realtime;

internal sealed class EncryptedStreamCache : ILiveSessionStreamCache
{
    private const uint KeyGenForwardJumpMax = 1_000_000;
    private const int MaxTerminalRawFrames = 256;
    private const int MaxTerminalRawBytes = 8 * 1024 * 1024;

    private readonly Lock _lock = new();
    private LiveTerminalCheckpointReplay? _terminalCheckpoint;
    private readonly Queue<LiveTerminalRawBatchReplay> _terminalRawBatches = new();
    private int _terminalRawBytes;
    private ulong? _terminalRawNextSequence;
    private LiveTerminalPresentationReplay? _terminalPresentation;
    private byte[]? _keyRotationMessage;
    private uint _currentKeyGen;
    private ulong? _terminalCheckpointHighRevision;
    private ulong? _terminalCheckpointHighNextSequence;
    private ulong? _terminalPresentationHighRevision;
    private Guid? _pendingPermissionsHighIncarnation;
    private ulong? _pendingPermissionsHighGeneration;
    private ulong? _terminalCheckpointMaxCounter;
    private ulong? _terminalRawMaxCounter;
    private ulong? _terminalPresentationMaxCounter;
    private ulong? _pendingPermissionsMaxCounter;
    private LivePendingPermissionsReplay? _pendingPermissions;
    private bool _keyGenInitialized;

    public LiveTerminalFrameStoreOutcome TryStoreEncryptedCheckpoint(
        ulong checkpointRevision,
        ulong nextSequence,
        byte[] encryptedBlob)
    {
        ArgumentNullException.ThrowIfNull(encryptedBlob);

        lock (_lock)
        {
            if (_terminalCheckpointHighRevision is { } highRevision)
            {
                if (checkpointRevision == highRevision)
                {
                    return nextSequence == _terminalCheckpointHighNextSequence!.Value
                        && _terminalCheckpoint is not null
                        ? LiveTerminalFrameStoreOutcome.Duplicate
                        : LiveTerminalFrameStoreOutcome.Rejected;
                }
                if (checkpointRevision < highRevision
                    || nextSequence < _terminalCheckpointHighNextSequence!.Value)
                {
                    return LiveTerminalFrameStoreOutcome.Rejected;
                }
            }
            if (_terminalRawBatches.Count == 0
                && _terminalRawNextSequence is { } rawHighWater
                && nextSequence < rawHighWater)
            {
                return LiveTerminalFrameStoreOutcome.Rejected;
            }
            if (_terminalRawBatches.Any(batch =>
                    batch.FirstSequence < nextSequence
                    && batch.NextSequence > nextSequence))
            {
                return LiveTerminalFrameStoreOutcome.Rejected;
            }

            _terminalCheckpoint = new LiveTerminalCheckpointReplay(
                checkpointRevision,
                nextSequence,
                encryptedBlob.ToArray());
            _terminalCheckpointHighRevision = checkpointRevision;
            _terminalCheckpointHighNextSequence = nextSequence;
            TrimRawBeforeCheckpointUnsafe(nextSequence);
            return LiveTerminalFrameStoreOutcome.Stored;
        }
    }

    public LiveTerminalFrameStoreOutcome TryStoreRawBatch(
        ulong firstSequence,
        ulong nextSequence,
        byte[] encryptedBlob)
    {
        ArgumentNullException.ThrowIfNull(encryptedBlob);

        lock (_lock)
        {
            if (firstSequence >= nextSequence)
            {
                return LiveTerminalFrameStoreOutcome.Rejected;
            }
            if (_terminalRawNextSequence is { } expected)
            {
                if (firstSequence < expected)
                {
                    return _terminalRawBatches.Any(batch =>
                            batch.FirstSequence == firstSequence
                            && batch.NextSequence == nextSequence)
                        ? LiveTerminalFrameStoreOutcome.Duplicate
                        : LiveTerminalFrameStoreOutcome.Rejected;
                }
                if (firstSequence != expected)
                {
                    return LiveTerminalFrameStoreOutcome.Rejected;
                }
            }
            else if (_terminalCheckpoint is { } checkpoint
                && firstSequence != checkpoint.NextSequence)
            {
                return LiveTerminalFrameStoreOutcome.Rejected;
            }

            var stored = encryptedBlob.ToArray();
            _terminalRawBatches.Enqueue(new LiveTerminalRawBatchReplay(
                firstSequence,
                nextSequence,
                stored));
            _terminalRawBytes += stored.Length;
            _terminalRawNextSequence = nextSequence;
            TrimTerminalRawUnsafe();
            return LiveTerminalFrameStoreOutcome.Stored;
        }
    }

    public LiveTerminalFrameStoreOutcome TryStoreEncryptedPresentation(
        ulong presentationRevision,
        byte[] encryptedBlob)
    {
        ArgumentNullException.ThrowIfNull(encryptedBlob);

        lock (_lock)
        {
            if (_terminalPresentationHighRevision is { } highRevision)
            {
                if (presentationRevision == highRevision)
                {
                    return _terminalPresentation is not null
                        ? LiveTerminalFrameStoreOutcome.Duplicate
                        : LiveTerminalFrameStoreOutcome.Rejected;
                }

                if (presentationRevision < highRevision)
                {
                    return LiveTerminalFrameStoreOutcome.Rejected;
                }
            }

            _terminalPresentation = new LiveTerminalPresentationReplay(
                presentationRevision,
                encryptedBlob.ToArray());
            _terminalPresentationHighRevision = presentationRevision;
            return LiveTerminalFrameStoreOutcome.Stored;
        }
    }

    public LiveTerminalFrameStoreOutcome TryStorePendingPermissionsSnapshot(
        Guid incarnationId,
        ulong snapshotGeneration,
        byte[] encryptedBlob)
    {
        ArgumentNullException.ThrowIfNull(encryptedBlob);
        lock (_lock)
        {
            if (_pendingPermissionsHighGeneration is { } highGeneration)
            {
                if (incarnationId != _pendingPermissionsHighIncarnation)
                {
                    return LiveTerminalFrameStoreOutcome.Rejected;
                }
                if (snapshotGeneration == highGeneration)
                {
                    return _pendingPermissions is { } current
                        && encryptedBlob.AsSpan().SequenceEqual(current.EncryptedBlob)
                        ? LiveTerminalFrameStoreOutcome.Duplicate
                        : LiveTerminalFrameStoreOutcome.Rejected;
                }
                if (snapshotGeneration < highGeneration)
                {
                    return LiveTerminalFrameStoreOutcome.Rejected;
                }
            }
            _pendingPermissions = new LivePendingPermissionsReplay(
                incarnationId,
                snapshotGeneration,
                encryptedBlob.ToArray());
            _pendingPermissionsHighIncarnation = incarnationId;
            _pendingPermissionsHighGeneration = snapshotGeneration;
            return LiveTerminalFrameStoreOutcome.Stored;
        }
    }

    public bool TryAdmitTerminalFrame(
        LiveTerminalFrameKind kind,
        uint keyGen,
        ulong counter)
    {
        lock (_lock)
        {
            if (!_keyGenInitialized || keyGen != _currentKeyGen)
            {
                return false;
            }

            ref var seenMax = ref TerminalCounterUnsafe(kind);
            if (seenMax is { } max && counter <= max)
            {
                return false;
            }
            seenMax = counter;
            return true;
        }
    }

    public bool TryAdmitPendingPermissionsFrame(uint keyGen, ulong counter)
    {
        lock (_lock)
        {
            if (!_keyGenInitialized || keyGen != _currentKeyGen)
            {
                return false;
            }

            if (_pendingPermissionsMaxCounter is { } seenMax && counter <= seenMax)
            {
                return false;
            }
            _pendingPermissionsMaxCounter = counter;
            return true;
        }
    }

    public LiveSessionKeyRotationStoreResult TryStoreKeyRotation(uint newKeyGen, byte[] keyRotationMessage)
    {
        ArgumentNullException.ThrowIfNull(keyRotationMessage);

        lock (_lock)
        {
            if (_keyGenInitialized)
            {
                if (newKeyGen <= _currentKeyGen)
                {
                    if (newKeyGen == _currentKeyGen)
                    {
                        _keyRotationMessage ??= keyRotationMessage.ToArray();
                        return LiveSessionKeyRotationStoreResult.DuplicateSameGeneration;
                    }
                    return LiveSessionKeyRotationStoreResult.Rejected;
                }
                if (newKeyGen - _currentKeyGen > KeyGenForwardJumpMax)
                {
                    return LiveSessionKeyRotationStoreResult.Rejected;
                }
            }
            else if (newKeyGen > KeyGenForwardJumpMax)
            {
                return LiveSessionKeyRotationStoreResult.Rejected;
            }
            _currentKeyGen = newKeyGen;
            _terminalCheckpointMaxCounter = null;
            _terminalRawMaxCounter = null;
            _terminalPresentationMaxCounter = null;
            _pendingPermissionsMaxCounter = null;
            ClearTerminalCiphertextUnsafe();
            _pendingPermissions = null;
            _keyGenInitialized = true;
            _keyRotationMessage = keyRotationMessage.ToArray();
            return LiveSessionKeyRotationStoreResult.StoredNewGeneration;
        }
    }

    public LiveSessionReplayState GetReplayState()
    {
        lock (_lock)
        {
            return new LiveSessionReplayState(
                Copy(_terminalCheckpoint),
                _terminalRawBatches.Select(Copy).ToArray(),
                Copy(_terminalPresentation),
                _keyRotationMessage?.ToArray(),
                _keyGenInitialized ? _currentKeyGen : 0,
                Copy(_pendingPermissions));
        }
    }

    private static LiveTerminalCheckpointReplay? Copy(
        LiveTerminalCheckpointReplay? value) =>
        value is null
            ? null
            : value with { EncryptedBlob = value.EncryptedBlob.ToArray() };

    private static LiveTerminalRawBatchReplay Copy(
        LiveTerminalRawBatchReplay value) =>
        value with { EncryptedBlob = value.EncryptedBlob.ToArray() };

    private static LiveTerminalPresentationReplay? Copy(
        LiveTerminalPresentationReplay? value) =>
        value is null
            ? null
            : value with { EncryptedBlob = value.EncryptedBlob.ToArray() };

    private static LivePendingPermissionsReplay? Copy(LivePendingPermissionsReplay? value) =>
        value is null
            ? null
            : value with { EncryptedBlob = value.EncryptedBlob.ToArray() };

    private ref ulong? TerminalCounterUnsafe(LiveTerminalFrameKind kind)
    {
        switch (kind)
        {
            case LiveTerminalFrameKind.SemanticCheckpoint:
                return ref _terminalCheckpointMaxCounter;
            case LiveTerminalFrameKind.RawBatch:
                return ref _terminalRawMaxCounter;
            case LiveTerminalFrameKind.Presentation:
                return ref _terminalPresentationMaxCounter;
            default:
                throw new ArgumentOutOfRangeException(nameof(kind), kind, null);
        }
    }

    private void TrimRawBeforeCheckpointUnsafe(ulong nextSequence)
    {
        while (_terminalRawBatches.TryPeek(out var frame)
            && frame.NextSequence <= nextSequence)
        {
            _terminalRawBatches.Dequeue();
            _terminalRawBytes -= frame.EncryptedBlob.Length;
        }

        _terminalRawNextSequence = _terminalRawBatches.TryPeek(out _)
            ? _terminalRawBatches.Last().NextSequence
            : nextSequence;
    }

    private void TrimTerminalRawUnsafe()
    {
        while (_terminalRawBatches.Count > 0
            && (_terminalRawBatches.Count > MaxTerminalRawFrames
                || _terminalRawBytes > MaxTerminalRawBytes))
        {
            var removed = _terminalRawBatches.Dequeue();
            _terminalRawBytes -= removed.EncryptedBlob.Length;
        }

        if (_terminalCheckpoint is { } checkpoint
            && (!_terminalRawBatches.TryPeek(out var firstRetained)
                || firstRetained.FirstSequence > checkpoint.NextSequence))
        {
            _terminalCheckpoint = null;
        }
    }

    private void ClearTerminalCiphertextUnsafe()
    {
        _terminalCheckpoint = null;
        _terminalRawBatches.Clear();
        _terminalRawBytes = 0;
        _terminalPresentation = null;
    }

}
