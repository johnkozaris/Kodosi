namespace Kodosi.Application;

public interface ILiveSessionStreamCache
{
    LiveTerminalFrameStoreOutcome TryStoreEncryptedCheckpoint(
        ulong checkpointRevision,
        ulong nextSequence,
        byte[] encryptedBlob);
    LiveTerminalFrameStoreOutcome TryStoreRawBatch(
        ulong firstSequence,
        ulong nextSequence,
        byte[] encryptedBlob);
    LiveTerminalFrameStoreOutcome TryStoreEncryptedPresentation(
        ulong presentationRevision,
        byte[] encryptedBlob);
    LiveTerminalFrameStoreOutcome TryStorePendingPermissionsSnapshot(
        Guid incarnationId,
        ulong snapshotGeneration,
        byte[] encryptedBlob);

    bool TryAdmitTerminalFrame(
        LiveTerminalFrameKind kind,
        uint keyGen,
        ulong counter);
    bool TryAdmitPendingPermissionsFrame(uint keyGen, ulong counter);
    LiveSessionKeyRotationStoreResult TryStoreKeyRotation(uint newKeyGen, byte[] keyRotationMessage);
    LiveSessionReplayState GetReplayState();
}

public enum LiveTerminalFrameKind
{
    SemanticCheckpoint,
    RawBatch,
    Presentation,
}

public enum LiveTerminalFrameStoreOutcome
{
    Stored,
    Duplicate,
    Rejected,
}
