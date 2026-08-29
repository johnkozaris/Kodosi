namespace Kodosi.Application;

public sealed record LiveSessionReplayState(
    LiveTerminalCheckpointReplay? TerminalCheckpoint,
    IReadOnlyList<LiveTerminalRawBatchReplay> TerminalRawBatches,
    LiveTerminalPresentationReplay? TerminalPresentation,
    byte[]? KeyRotation,
    uint CurrentKeyGeneration,
    LivePendingPermissionsReplay? PendingPermissions);

public sealed record LiveTerminalCheckpointReplay(
    ulong CheckpointRevision,
    ulong NextSequence,
    byte[] EncryptedBlob);

public sealed record LiveTerminalRawBatchReplay(
    ulong FirstSequence,
    ulong NextSequence,
    byte[] EncryptedBlob);

public sealed record LiveTerminalPresentationReplay(
    ulong PresentationRevision,
    byte[] EncryptedBlob);

public sealed record LivePendingPermissionsReplay(
    Guid IncarnationId,
    ulong SnapshotGeneration,
    byte[] EncryptedBlob);
