using Kodosi.Application;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class EncryptedStreamCacheTests
{
    [Fact]
    public void Live_Frame_Kinds_Have_Separate_Counter_Domains()
    {
        var stream = PrimedStream();

        foreach (var kind in Enum.GetValues<LiveTerminalFrameKind>())
        {
            Assert.True(stream.TryAdmitTerminalFrame(kind, 1, 10));
            Assert.False(stream.TryAdmitTerminalFrame(kind, 1, 10));
            Assert.False(stream.TryAdmitTerminalFrame(kind, 1, 9));
            Assert.True(stream.TryAdmitTerminalFrame(kind, 1, 11));
        }
        Assert.True(stream.TryAdmitPendingPermissionsFrame(1, 10));
        Assert.False(stream.TryAdmitPendingPermissionsFrame(1, 10));
    }

    [Fact]
    public void Terminal_Admission_Requires_Current_Key_Generation()
    {
        var stream = new EncryptedStreamCache();
        Assert.False(stream.TryAdmitTerminalFrame(
            LiveTerminalFrameKind.SemanticCheckpoint,
            1,
            1));
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            stream.TryStoreKeyRotation(1, KeyRotation(1)));
        Assert.False(stream.TryAdmitTerminalFrame(
            LiveTerminalFrameKind.SemanticCheckpoint,
            2,
            1));
        Assert.True(stream.TryAdmitTerminalFrame(
            LiveTerminalFrameKind.SemanticCheckpoint,
            1,
            1));
    }

    [Fact]
    public void Checkpoint_Replaces_Only_With_Monotonic_Revision_And_Boundary()
    {
        var stream = PrimedStream();

        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(7, 20, [0x03, 0x01]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Duplicate,
            stream.TryStoreEncryptedCheckpoint(7, 20, [0x03, 0x02]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedCheckpoint(8, 19, [0x03, 0x03]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(8, 20, [0x03, 0x04]));

        var replay = stream.GetReplayState();
        Assert.Equal(8UL, replay.TerminalCheckpoint?.CheckpointRevision);
        Assert.Equal(new byte[] { 0x03, 0x04 }, replay.TerminalCheckpoint?.EncryptedBlob);
    }

    [Fact]
    public void Raw_Batches_Must_Be_Contiguous()
    {
        var stream = PrimedStream();
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 5, [0x03]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(5, 7, [0x04, 0x01]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreRawBatch(8, 9, [0x04, 0x02]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreRawBatch(7, 7, [0x04, 0x03]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(7, 9, [0x04, 0x04]));
    }

    [Fact]
    public void Raw_Ring_Evicts_Oldest_At_256_Frames()
    {
        var stream = PrimedStream();
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 0, [0x03]));
        for (ulong sequence = 0; sequence < 257; sequence++)
        {
            Assert.Equal(
                LiveTerminalFrameStoreOutcome.Stored,
                stream.TryStoreRawBatch(
                    sequence,
                    sequence + 1,
                    [0x04, (byte)sequence]));
        }

        var replay = stream.GetReplayState();
        var batches = replay.TerminalRawBatches;
        Assert.Equal(256, batches.Count);
        Assert.Equal(1UL, batches[0].FirstSequence);
        Assert.Equal(257UL, batches[^1].NextSequence);
        Assert.Null(replay.TerminalCheckpoint);
    }

    [Fact]
    public void Raw_Ring_Evicts_Oldest_At_Eight_Mib()
    {
        var stream = PrimedStream();
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 0, [0x03]));
        for (ulong sequence = 0; sequence < 5; sequence++)
        {
            Assert.Equal(
                LiveTerminalFrameStoreOutcome.Stored,
                stream.TryStoreRawBatch(
                    sequence,
                    sequence + 1,
                    new byte[2 * 1024 * 1024]));
        }

        var batches = stream.GetReplayState().TerminalRawBatches;
        Assert.Equal(4, batches.Count);
        Assert.Equal(1UL, batches[0].FirstSequence);
    }

    [Fact]
    public void New_Checkpoint_Trims_Covered_Raw_Batches()
    {
        var stream = PrimedStream();
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 0, [0x03]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(0, 2, [0x04, 0]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(2, 4, [0x04, 1]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(2, 2, [0x03, 2]));

        var batches = stream.GetReplayState().TerminalRawBatches;
        Assert.Single(batches);
        Assert.Equal(2UL, batches[0].FirstSequence);
    }

    [Fact]
    public void Presentation_Keeps_Latest_Revision()
    {
        var stream = PrimedStream();
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(3, [0x05, 3]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Duplicate,
            stream.TryStoreEncryptedPresentation(3, [0x05, 4]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedPresentation(2, [0x05, 2]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(4, [0x05, 4]));
        Assert.Equal(4UL,
            stream.GetReplayState().TerminalPresentation?.PresentationRevision);
    }

    [Fact]
    public void Key_Rotation_Clears_Terminal_Ciphertext_And_Resets_Per_Key_Counters()
    {
        var stream = PrimedStream();
        foreach (var kind in Enum.GetValues<LiveTerminalFrameKind>())
        {
            Assert.True(stream.TryAdmitTerminalFrame(kind, 1, 50));
        }
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 0, [0x03]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(0, 1, [0x04]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(1, [0x05]));

        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            stream.TryStoreKeyRotation(2, KeyRotation(2)));

        var replay = stream.GetReplayState();
        Assert.Null(replay.TerminalCheckpoint);
        Assert.Empty(replay.TerminalRawBatches);
        Assert.Null(replay.TerminalPresentation);
        foreach (var kind in Enum.GetValues<LiveTerminalFrameKind>())
        {
            Assert.True(stream.TryAdmitTerminalFrame(kind, 2, 0));
            Assert.False(stream.TryAdmitTerminalFrame(kind, 1, 51));
        }
    }

    [Fact]
    public void Key_Rotation_Preserves_Logical_High_Water_Across_All_Encrypted_Lanes()
    {
        var stream = PrimedStream();
        var incarnation = Guid.NewGuid();
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(7, 10, [0x03, 7]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(10, 12, [0x04, 12]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(5, [0x05, 5]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 9, [0x06, 9]));

        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            stream.TryStoreKeyRotation(2, KeyRotation(2)));
        var cleared = stream.GetReplayState();
        Assert.Null(cleared.TerminalCheckpoint);
        Assert.Empty(cleared.TerminalRawBatches);
        Assert.Null(cleared.TerminalPresentation);
        Assert.Null(cleared.PendingPermissions);

        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedCheckpoint(7, 12, [0x03, 7]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedCheckpoint(6, 12, [0x03, 6]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedCheckpoint(8, 11, [0x03, 8]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(8, 12, [0x03, 8]));

        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreRawBatch(11, 12, [0x04, 11]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreRawBatch(13, 14, [0x04, 14]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(12, 13, [0x04, 13]));

        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedPresentation(5, [0x05, 5]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStoreEncryptedPresentation(4, [0x05, 4]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(6, [0x05, 6]));

        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 9, [0x06, 9]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 8, [0x06, 8]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStorePendingPermissionsSnapshot(Guid.NewGuid(), 10, [0x06, 10]));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 10, [0x06, 10]));
    }

    [Fact]
    public void Replay_State_Does_Not_Expose_Stored_Array_Aliases()
    {
        var stream = PrimedStream();
        var checkpoint = new byte[] { 0x03, 0x01 };
        var raw = new byte[] { 0x04, 0x02 };
        var presentation = new byte[] { 0x05, 0x03 };
        var pending = new byte[] { 0x06, 0x04 };
        var incarnation = Guid.NewGuid();
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 0, checkpoint));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(0, 1, raw));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedPresentation(1, presentation));
        Assert.Equal(LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 1, pending));

        checkpoint[0] = raw[0] = presentation[0] = pending[0] = 0xFF;
        var replay = stream.GetReplayState();
        replay.TerminalCheckpoint!.EncryptedBlob[0] = 0xEE;
        replay.TerminalRawBatches[0].EncryptedBlob[0] = 0xEE;
        replay.TerminalPresentation!.EncryptedBlob[0] = 0xEE;
        replay.KeyRotation![0] = 0xEE;
        replay.PendingPermissions!.EncryptedBlob[0] = 0xEE;

        var reread = stream.GetReplayState();
        Assert.Equal(0x03, reread.TerminalCheckpoint!.EncryptedBlob[0]);
        Assert.Equal(0x04, reread.TerminalRawBatches[0].EncryptedBlob[0]);
        Assert.Equal(0x05, reread.TerminalPresentation!.EncryptedBlob[0]);
        Assert.Equal(0x01, reread.KeyRotation![0]);
        Assert.Equal(0x06, reread.PendingPermissions!.EncryptedBlob[0]);
    }

    [Fact]
    public void Pending_Permissions_Ciphertext_Is_Cleared_By_Key_Rotation()
    {
        var stream = PrimedStream();
        var incarnation = Guid.NewGuid();
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 1, [0x06, 1]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 0, [0x06, 0]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Rejected,
            stream.TryStorePendingPermissionsSnapshot(Guid.NewGuid(), 2, [0x06, 2]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 2, [0x06, 2]));
        Assert.Equal(2UL, stream.GetReplayState().PendingPermissions?.SnapshotGeneration);

        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            stream.TryStoreKeyRotation(2, KeyRotation(2)));
        Assert.Null(stream.GetReplayState().PendingPermissions);
        Assert.True(stream.TryAdmitPendingPermissionsFrame(2, 1));
    }

    [Fact]
    public void Pending_Permissions_Accepts_Next_Generation_Empty_Snapshot_After_Task_Restart()
    {
        var stream = PrimedStream();
        var incarnation = Guid.NewGuid();
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 7, [0x06, 7]));

        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStorePendingPermissionsSnapshot(incarnation, 8, [0x06, 8]));
        Assert.Equal(8UL, stream.GetReplayState().PendingPermissions?.SnapshotGeneration);
    }

    private static EncryptedStreamCache PrimedStream()
    {
        var stream = new EncryptedStreamCache();
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            stream.TryStoreKeyRotation(1, KeyRotation(1)));
        return stream;
    }

    private static byte[] KeyRotation(uint keyGen) => [(byte)keyGen];
}
