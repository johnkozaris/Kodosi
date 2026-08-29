using System.Text.Json;
using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class DeviceRevocationAuditEntryTests
{
    public static TheoryData<string, Guid[]> InvalidTargetEvidence()
    {
        var sessionId = Guid.NewGuid();
        var incarnationId = Guid.NewGuid();
        var target = new DeviceRevocationSessionTarget(
            SessionId.From(sessionId),
            incarnationId);
        return new TheoryData<string, Guid[]>
        {
            { "null", [sessionId] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([new DeviceRevocationSessionTarget(
                SessionId.From(Guid.Empty),
                incarnationId)]), [Guid.Empty] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([new DeviceRevocationSessionTarget(
                SessionId.From(sessionId),
                Guid.Empty)]), [sessionId] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([target, target]), [sessionId] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([
                target,
                target with { IncarnationId = Guid.NewGuid() }]), [sessionId] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([target]), [Guid.NewGuid()] },
            { JsonSerializer.Serialize<DeviceRevocationSessionTarget[]>([target]), [sessionId, Guid.NewGuid()] },
        };
    }

    [Theory]
    [MemberData(nameof(InvalidTargetEvidence))]
    public void Target_Evidence_Rejects_Malformed_Or_Inconsistent_Tuples(
        string targetsJson,
        Guid[] affectedSessionIds)
    {
        var entry = CreateEntry();
        DomainFixtureHydrator.SetDeviceRevocationTargetEvidence(
            entry,
            targetsJson,
            affectedSessionIds);

        var exception = Assert.Throws<DomainException>(() =>
            entry.GetAffectedSessionTargets());

        Assert.Equal("Affected session target evidence is invalid.", exception.Message);
    }

    [Fact]
    public void Creation_Rejects_Duplicate_Session_Targets()
    {
        var target = new DeviceRevocationSessionTarget(
            SessionId.New(),
            Guid.NewGuid());

        Assert.Throws<DomainException>(() => CreateEntry([
            target,
            target with { IncarnationId = Guid.NewGuid() },
        ]));
    }

    [Fact]
    public void Target_Evidence_Normalizes_Valid_Tuples()
    {
        var first = new DeviceRevocationSessionTarget(
            SessionId.From(Guid.Parse("10000000-0000-0000-0000-000000000001")),
            Guid.Parse("30000000-0000-0000-0000-000000000003"));
        var second = new DeviceRevocationSessionTarget(
            SessionId.From(Guid.Parse("20000000-0000-0000-0000-000000000002")),
            Guid.Parse("40000000-0000-0000-0000-000000000004"));
        var entry = CreateEntry([second, first]);

        Assert.Equal([first, second], entry.GetAffectedSessionTargets());
    }

    private static DeviceRevocationAuditEntry CreateEntry(
        IReadOnlyCollection<DeviceRevocationSessionTarget>? targets = null) =>
        DeviceRevocationAuditEntry.Create(
            Guid.NewGuid(),
            UserId.New(),
            "revoked-device",
            "signer-device",
            2,
            1,
            0,
            targets ?? [new DeviceRevocationSessionTarget(
                SessionId.New(),
                Guid.NewGuid())],
            null,
            null);
}
