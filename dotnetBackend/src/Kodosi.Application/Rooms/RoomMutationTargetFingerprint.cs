using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Domain;

namespace Kodosi.Application;

public readonly record struct RoomMutationTargetFingerprint
{
    private static readonly byte[] Domain =
        Encoding.UTF8.GetBytes("kodosi:room-mutation-target:v1:");

    private RoomMutationTargetFingerprint(byte[] value)
    {
        Value = value;
    }

    public byte[] Value { get; }

    public static RoomMutationTargetFingerprint AcceptInvitation(
        RoomId roomId,
        Guid invitationId,
        long baseRosterGeneration,
        long proposedRosterGeneration,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId) =>
        Build(writer =>
        {
            writer.WriteString("acceptInvitation");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(invitationId);
            writer.WriteInt64(baseRosterGeneration);
            writer.WriteInt64(proposedRosterGeneration);
            writer.WriteHash(decisionBody);
            writer.WriteHash(decisionSignature);
            writer.WriteString(decisionSignerDeviceId);
        });

    public static RoomMutationTargetFingerprint DeclineInvitation(
        RoomId roomId,
        Guid invitationId,
        long baseRosterGeneration,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId) =>
        Build(writer =>
        {
            writer.WriteString("declineInvitation");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(invitationId);
            writer.WriteInt64(baseRosterGeneration);
            writer.WriteHash(decisionBody);
            writer.WriteHash(decisionSignature);
            writer.WriteString(decisionSignerDeviceId);
        });

    public static RoomMutationTargetFingerprint CancelInvitation(
        RoomId roomId,
        Guid invitationId,
        long baseRosterGeneration) =>
        Build(writer =>
        {
            writer.WriteString("cancelInvitation");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(invitationId);
            writer.WriteInt64(baseRosterGeneration);
        });

    public static RoomMutationTargetFingerprint RemoveMember(
        RoomId roomId,
        UserId removedUserId,
        long baseRosterGeneration,
        long desiredRosterGeneration,
        byte[] rosterBody,
        byte[] rosterSignature,
        string rosterSignerDeviceId) =>
        Build(writer =>
        {
            writer.WriteString("removeMember");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(removedUserId.Value);
            writer.WriteInt64(baseRosterGeneration);
            writer.WriteInt64(desiredRosterGeneration);
            writer.WriteHash(rosterBody);
            writer.WriteHash(rosterSignature);
            writer.WriteString(rosterSignerDeviceId);
        });

    public static RoomMutationTargetFingerprint AssignTask(
        RoomId roomId,
        Guid taskId,
        long expectedTaskRevision,
        Guid? desiredAssigneeSessionId,
        Guid? desiredAssigneeSessionIncarnationId) =>
        Build(writer =>
        {
            writer.WriteString("assignTask");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(taskId);
            writer.WriteInt64(expectedTaskRevision);
            writer.WriteNullableGuid(desiredAssigneeSessionId);
            writer.WriteNullableGuid(desiredAssigneeSessionIncarnationId);
        });

    public static RoomMutationTargetFingerprint TransitionTask(
        RoomId roomId,
        Guid taskId,
        long expectedTaskRevision,
        RoomTaskStatus desiredStatus,
        Guid? actorSessionId,
        Guid? actorSessionIncarnationId,
        string? encryptedResult) =>
        Build(writer =>
        {
            writer.WriteString("transitionTask");
            writer.WriteGuid(roomId.Value);
            writer.WriteGuid(taskId);
            writer.WriteInt64(expectedTaskRevision);
            writer.WriteString(desiredStatus.ToString());
            writer.WriteNullableGuid(actorSessionId);
            writer.WriteNullableGuid(actorSessionIncarnationId);
            writer.WriteNullableHash(encryptedResult);
        });

    private static RoomMutationTargetFingerprint Build(Action<FingerprintWriter> append)
    {
        using var stream = new MemoryStream();
        stream.Write(Domain);
        append(new FingerprintWriter(stream));
        return new RoomMutationTargetFingerprint(SHA256.HashData(stream.GetBuffer().AsSpan(
            0,
            checked((int)stream.Length))));
    }

    private sealed class FingerprintWriter(Stream stream)
    {
        public void WriteGuid(Guid value)
        {
            Span<byte> bytes = stackalloc byte[16];
            value.TryWriteBytes(bytes, bigEndian: true, out _);
            WriteBytes(bytes);
        }

        public void WriteNullableGuid(Guid? value)
        {
            WriteByte(value.HasValue ? (byte)1 : (byte)0);
            if (value is { } present)
            {
                WriteGuid(present);
            }
        }

        public void WriteInt64(long value)
        {
            Span<byte> bytes = stackalloc byte[8];
            BinaryPrimitives.WriteInt64BigEndian(bytes, value);
            WriteBytes(bytes);
        }

        public void WriteString(string value) => WriteBytes(Encoding.UTF8.GetBytes(value));

        public void WriteHash(ReadOnlySpan<byte> value) => WriteBytes(SHA256.HashData(value));

        public void WriteNullableHash(string? value)
        {
            WriteByte(value is null ? (byte)0 : (byte)1);
            if (value is not null)
            {
                WriteHash(Encoding.UTF8.GetBytes(value));
            }
        }

        private void WriteByte(byte value) => stream.WriteByte(value);

        private void WriteBytes(ReadOnlySpan<byte> value)
        {
            Span<byte> length = stackalloc byte[4];
            BinaryPrimitives.WriteInt32BigEndian(length, value.Length);
            stream.Write(length);
            stream.Write(value);
        }
    }
}
