using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class ConnectionRegistryTests
{
    [Fact]
    public void DeviceRegistrationFence_Cancels_Connections_Registered_During_Identity_Reset()
    {
        var registry = new ConnectionRegistry();
        var userId = UserId.New();
        using var connectionLifetime = new CancellationTokenSource();

        using (registry.FenceDeviceRegistrations(userId, ["reset-device"]))
        {
            registry.RegisterSharedParticipant(
                "late-connection",
                userId,
                "reset-device",
                SessionId.New(),
                connectionLifetime);

            Assert.True(connectionLifetime.IsCancellationRequested);

            using var hostLifetime = new CancellationTokenSource();
            registry.RegisterHost(
                "late-host",
                userId,
                "reset-device",
                SessionId.New(),
                hostLifetime);
            Assert.True(hostLifetime.IsCancellationRequested);
        }
    }

    [Fact]
    public void SessionQueries_Stay_Scoped_To_That_Session_And_User()
    {
        var sessionA = SessionId.New();
        var sessionB = SessionId.New();
        var sharedUserA = UserId.New();
        var sharedUserB = UserId.New();
        var ownerUser = UserId.New();
        var registry = new ConnectionRegistry();

        registry.RegisterSharedParticipant("shared-a-1", sharedUserA, "shared-device-a", sessionA);
        registry.RegisterSharedParticipant("shared-a-2", sharedUserA, "shared-device-a", sessionA);
        registry.RegisterSharedParticipant("shared-b-1", sharedUserA, "shared-device-b", sessionB);
        registry.RegisterSharedParticipant("shared-other-user", sharedUserB, "other-device", sessionA);
        registry.RegisterOwnerParticipant("owner-a", ownerUser, "owner-device", sessionA);
        registry.RegisterHost("host-a", ownerUser, "owner-device", sessionA);

        Assert.Equal(
            ["shared-a-1", "shared-a-2"],
            registry.GetSharedParticipantConnectionIds(sessionA, sharedUserA).OrderBy(connectionId => connectionId));
        Assert.Equal(
            [("owner-a", ownerUser), ("shared-a-1", sharedUserA), ("shared-a-2", sharedUserA), ("shared-other-user", sharedUserB)],
            registry.GetActiveParticipants(sessionA).OrderBy(participant => participant.ConnectionId));
        Assert.Equal(
            [("shared-a-1", sharedUserA), ("shared-a-2", sharedUserA), ("shared-other-user", sharedUserB)],
            registry.GetActiveSharedParticipants(sessionA).OrderBy(participant => participant.ConnectionId));
        Assert.Equal(
            ["host-a", "owner-a"],
            registry.GetConnectionsForDevice(ownerUser, "owner-device")
                .Select(connection => connection.ConnectionId)
                .OrderBy(connectionId => connectionId));
    }

    [Fact]
    public void Remove_And_Reregister_Refresh_Session_Indexes()
    {
        var firstSessionId = SessionId.New();
        var secondSessionId = SessionId.New();
        var firstUserId = UserId.New();
        var secondUserId = UserId.New();
        var registry = new ConnectionRegistry();

        registry.RegisterSharedParticipant("viewer-1", firstUserId, "first-device", firstSessionId);
        registry.Remove("viewer-1");
        registry.RegisterSharedParticipant("viewer-1", secondUserId, "second-device", secondSessionId);

        Assert.Empty(registry.GetSharedParticipantConnectionIds(firstSessionId, firstUserId));
        Assert.Empty(registry.GetActiveParticipants(firstSessionId));
        Assert.Equal(["viewer-1"], registry.GetSharedParticipantConnectionIds(secondSessionId, secondUserId));
        Assert.Equal([("viewer-1", secondUserId)], registry.GetActiveSharedParticipants(secondSessionId));
    }

    [Fact]
    public void CancelConnectionsForDevice_Cancels_The_Snapshotted_Targets()
    {
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var registry = new ConnectionRegistry();
        using var target = new CancellationTokenSource();
        registry.RegisterHost(
            "host",
            userId,
            "device",
            sessionId,
            target);

        var connections = registry.CancelConnectionsForDevice(
            userId,
            "device");

        Assert.True(target.IsCancellationRequested);
        Assert.Equal(["host"], connections.Select(connection => connection.ConnectionId));
    }
}
