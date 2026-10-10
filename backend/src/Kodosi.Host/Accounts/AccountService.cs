using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Accounts;

public sealed class AccountService(KodosiDbContext db, ConnectionDirectory connections, DeviceSessions deviceSessions, AccountGate gate, TimeProvider clock)
{
    internal static readonly TimeSpan DeletionMemory = TimeSpan.FromDays(30);

    public async Task DeleteSignedInAsync(User user, DateTimeOffset? authenticatedAt, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        if (authenticatedAt is null || authenticatedAt > now.AddMinutes(5) || now - authenticatedAt > DeviceService.ReauthenticationWindow)
            throw ApiException.Forbidden("Sign in again to delete your account.");
        await DeleteAsync(user, now, ct);
    }

    public async Task ForgetAsync(string issuer, string subject, DateTimeOffset deletedAt, CancellationToken ct)
    {
        var user = await db.Users.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
        if (user is null)
        {
            await RememberAsync(issuer, subject, deletedAt, ct);
            await db.SaveChangesAsync(ct);
            return;
        }
        using var held = await gate.EnterAsync(user.Id, ct);
        await DeleteAsync(user, deletedAt, ct);
    }

    private async Task DeleteAsync(User user, DateTimeOffset deletedAt, CancellationToken ct)
    {
        var userId = user.Id;
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var friendships = await db.Friendships.Where(x => x.FirstUserId == userId || x.SecondUserId == userId).ToListAsync(ct);
        var owned = await db.Sessions.Where(x => x.OwnerUserId == userId).ToListAsync(ct);
        var ownedIds = owned.Select(x => x.Id).ToArray();
        var viewers = await db.SessionMembers.Where(x => ownedIds.Contains(x.SessionId)).Select(x => x.UserId).Distinct().ToListAsync(ct);
        var memberships = await db.SessionMembers.Where(x => x.UserId == userId).ToListAsync(ct);
        var joinedIds = memberships.Select(x => x.SessionId).ToArray();
        var joined = await db.Sessions.Where(x => joinedIds.Contains(x.Id)).ToListAsync(ct);
        var missions = await db.Missions.Where(x => x.OwnerUserId == userId).ToListAsync(ct);
        var missionIds = missions.Select(x => x.Id).ToArray();
        var missionMemberships = await db.MissionMembers.Where(x => x.UserId == userId).ToListAsync(ct);
        var invitations = await db.MissionInvitations.Where(x => x.UserId == userId || x.InviterUserId == userId).ToListAsync(ct);
        var touchedMissions = missionIds.Concat(missionMemberships.Select(x => x.MissionId)).Concat(invitations.Select(x => x.MissionId)).Distinct().ToArray();
        var missionPeople = await db.MissionMembers.Where(x => touchedMissions.Contains(x.MissionId)).Select(x => x.UserId)
            .Concat(db.MissionInvitations.Where(x => touchedMissions.Contains(x.MissionId)).Select(x => x.UserId)).Distinct().ToListAsync(ct);
        var devices = await db.Devices.Where(x => x.UserId == userId).ToListAsync(ct);
        foreach (var session in joined) session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
        db.SessionMembers.RemoveRange(memberships);
        db.Sessions.RemoveRange(owned);
        db.MissionInvitations.RemoveRange(invitations);
        db.MissionMembers.RemoveRange(missionMemberships);
        db.Missions.RemoveRange(missions);
        db.Friendships.RemoveRange(friendships);
        await db.SaveChangesAsync(ct);
        db.DeviceLists.RemoveRange(await db.DeviceLists.Where(x => x.UserId == userId).ToListAsync(ct));
        db.DeviceLinks.RemoveRange(await db.DeviceLinks.Where(x => x.UserId == userId).ToListAsync(ct));
        db.FriendLists.RemoveRange(await db.FriendLists.Where(x => x.UserId == userId).ToListAsync(ct));
        db.Devices.RemoveRange(devices);
        db.Users.Remove(user);
        await RememberAsync(user.Issuer, user.Subject, deletedAt, ct);
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        deviceSessions.Remove(userId);
        foreach (var session in owned) connections.RemoveSession(session.Id);
        foreach (var session in joined) connections.Revoke(session, (member, _) => member != userId);
        foreach (var device in devices) connections.RemoveDevice(userId, device.Id);
        foreach (var friend in friendships.Select(x => x.FirstUserId == userId ? x.SecondUserId : x.FirstUserId)) connections.Notify(friend, "friends");
        foreach (var viewer in viewers) connections.Notify(viewer, "sessions");
        foreach (var person in missionPeople.Where(x => x != userId)) { connections.Notify(person, "missions"); connections.Notify(person, "sessions"); }
    }

    private async Task RememberAsync(string issuer, string subject, DateTimeOffset deletedAt, CancellationToken ct)
    {
        var known = await db.DeletedAccounts.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
        if (known is null) db.DeletedAccounts.Add(new DeletedAccount { Issuer = issuer, Subject = subject, DeletedAt = deletedAt });
        else if (known.DeletedAt < deletedAt) known.DeletedAt = deletedAt;
    }
}
