using System.Reflection;
using System.Runtime.Serialization;
using Kodosi.Domain;

namespace Kodosi.DomainTests;

internal static class DomainFixtureHydrator
{
#pragma warning disable SYSLIB0050
    public static RoomMember RoomMember(
        RoomId roomId,
        UserId userId,
        RoomRole role = RoomRole.Owner)
    {
        var member = (RoomMember)FormatterServices.GetUninitializedObject(typeof(RoomMember));
        Set(member, nameof(Kodosi.Domain.RoomMember.RoomId), roomId);
        Set(member, nameof(Kodosi.Domain.RoomMember.UserId), userId);
        Set(member, nameof(Kodosi.Domain.RoomMember.Role), role);
        Set(member, nameof(Kodosi.Domain.RoomMember.CreatedAt), DateTimeOffset.UtcNow);
        return member;
    }
#pragma warning restore SYSLIB0050

    public static void CancelDeviceLink(
        DeviceLinkRequest request,
        DateTimeOffset cancelledAt) =>
        Set(request, nameof(DeviceLinkRequest.CancelledAt), cancelledAt);

    public static void SetDeviceRevocationTargetEvidence(
        DeviceRevocationAuditEntry entry,
        string? targetsJson,
        Guid[] affectedSessionIds)
    {
        Set(entry, nameof(DeviceRevocationAuditEntry.AffectedSessionTargetsJson), targetsJson);
        Set(entry, nameof(DeviceRevocationAuditEntry.AffectedSessionIds), affectedSessionIds);
    }

    public static void SetIdentityResetTargetEvidence(
        IdentityResetAuditEntry entry,
        string? sessionTargetsJson,
        string? endedSessionTargetsJson,
        Guid[] endedSessionIds,
        Guid[] sessionsWithRevokedKeys)
    {
        Set(entry, nameof(IdentityResetAuditEntry.SessionTargetsJson), sessionTargetsJson);
        Set(entry, nameof(IdentityResetAuditEntry.EndedSessionTargetsJson), endedSessionTargetsJson);
        Set(entry, nameof(IdentityResetAuditEntry.EndedSessionIds), endedSessionIds);
        Set(entry, nameof(IdentityResetAuditEntry.SessionsWithRevokedKeys), sessionsWithRevokedKeys);
    }

    private static void Set<T>(object instance, string propertyName, T value) =>
        instance.GetType()
            .GetProperty(
                propertyName,
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(instance, value);
}
