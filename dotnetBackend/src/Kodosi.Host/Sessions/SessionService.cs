using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Realtime;
using Kodosi.Rooms;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Sessions;

public sealed class SessionService(KodosiDbContext db, RelayDirectory relay,
    DeviceService devices, RoomService rooms, SignatureVerifier signatures, TimeProvider clock)
{
    public async Task<Session> AuthorizedAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var session = await db.Sessions.SingleOrDefaultAsync(x => x.Id == id && !x.Ended, ct) ?? throw ApiException.Missing();
        if (session.OwnerUserId != userId && !await db.SessionMembers.AnyAsync(x => x.SessionId == id && x.UserId == userId, ct))
            throw ApiException.Missing();
        return session;
    }
    public async Task<Session> OwnerAsync(Guid id, Guid userId, string deviceId, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct);
        if (session.OwnerUserId != userId) throw ApiException.Forbidden("Only the owner may change this publication.");
        await devices.RequireDeviceAsync(userId, deviceId, ct);
        return session;
    }
    public async Task<Session> HostAsync(Guid id, Guid userId, string deviceId, CancellationToken ct)
    {
        var session = await OwnerAsync(id, userId, deviceId, ct);
        if (session.HostDeviceId != deviceId) throw ApiException.Forbidden("Only the hosting device may change this publication.");
        return session;
    }
    public async Task<SessionDto> DescribeAsync(Session session, CancellationToken ct)
    {
        var owner = await db.Users.AsNoTracking().SingleAsync(x => x.Id == session.OwnerUserId, ct);
        var shared = await db.SessionMembers.AsNoTracking().Where(x => x.SessionId == session.Id).Select(x => x.UserId).ToArrayAsync(ct);
        var room = session.RoomId is { } roomId ? await db.Rooms.AsNoTracking().SingleOrDefaultAsync(x => x.Id == roomId, ct) : null;
        return new SessionDto(session.Id, session.IncarnationId, session.Name, session.OwnerUserId, owner.DisplayName,
            session.HostDeviceId, session.HostName, session.RoomId, room?.Name, shared, session.AuthorizationRevision,
            session.KeyGeneration, session.Ready, relay.HostOnline(session.Id));
    }
    public async Task<IReadOnlyList<SessionDto>> ListAsync(Guid userId, CancellationToken ct)
    {
        var sessions = await (from session in db.Sessions.AsNoTracking()
            where !session.Ended && (session.OwnerUserId == userId || db.SessionMembers.Any(m => m.SessionId == session.Id && m.UserId == userId))
            join owner in db.Users on session.OwnerUserId equals owner.Id
            join room in db.Rooms on session.RoomId equals room.Id into roomsById
            from room in roomsById.DefaultIfEmpty()
            orderby session.CreatedAt, session.Id
            select new { Session = session, OwnerName = owner.DisplayName, RoomName = room == null ? null : room.Name })
            .Take(4097).ToListAsync(ct);
        if (sessions.Count > 4096) throw ApiException.Conflict("The session catalog exceeds its display limit.");
        var ids = sessions.Select(x => x.Session.Id).ToArray();
        var members = await db.SessionMembers.AsNoTracking().Where(x => ids.Contains(x.SessionId))
            .Select(x => new { x.SessionId, x.UserId }).ToListAsync(ct);
        var shared = members.ToLookup(x => x.SessionId, x => x.UserId);
        return sessions.Select(x => new SessionDto(x.Session.Id, x.Session.IncarnationId, x.Session.Name,
            x.Session.OwnerUserId, x.OwnerName, x.Session.HostDeviceId, x.Session.HostName,
            x.Session.RoomId, x.RoomName, shared[x.Session.Id].ToArray(), x.Session.AuthorizationRevision,
            x.Session.KeyGeneration, x.Session.Ready, relay.HostOnline(x.Session.Id))).ToArray();
    }
    public async Task<SessionDto> CreateAsync(Guid userId, Device device, CreateSession body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Session ID"); Limits.Id(body.IncarnationId, "Incarnation ID");
        if (body.HostDeviceId != device.Id) throw ApiException.Forbidden("The hosting device proof must match.");
        await devices.RequireDeviceAsync(userId, device.Id, ct);
        var existing = await db.Sessions.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.Ended || existing.OwnerUserId != userId || existing.HostDeviceId != device.Id || existing.IncarnationId != body.IncarnationId)
                throw ApiException.Conflict("Session publication identity was already used.");
            return await DescribeAsync(existing, ct);
        }
        if (await db.Sessions.CountAsync(x => x.OwnerUserId == userId && !x.Ended, ct) >= Limits.MaxSessionsPerUser)
            throw ApiException.Conflict("Too many published sessions.");
        if (body.RoomId is { } roomId) await rooms.RequireMemberAsync(roomId, userId, ct);
        var session = new Session
        {
            Id = body.Id,
            IncarnationId = body.IncarnationId,
            OwnerUserId = userId,
            HostDeviceId = device.Id,
            HostName = Limits.Text(body.HostName, "Host name", 128),
            Name = Limits.Text(body.Name, "Session name", 128),
            RoomId = body.RoomId,
            ExpiresAt = clock.GetUtcNow() + PublicationCleanup.GracePeriod,
            CreatedAt = clock.GetUtcNow()
        };
        db.Sessions.Add(session); await db.SaveChangesAsync(ct); relay.Notify(userId, "sessions");
        return await DescribeAsync(session, ct);
    }
    public async Task<SessionDto> RenameAsync(Guid id, Guid userId, string deviceId, RenameSession body, CancellationToken ct)
    {
        var session = await OwnerAsync(id, userId, deviceId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        session.Name = Limits.Text(body.Name, "Session name", 128); await db.SaveChangesAsync(ct); await NotifyAsync(session, ct);
        return await DescribeAsync(session, ct);
    }
    public async Task<SessionDto> AttachAsync(Guid id, Guid userId, string deviceId, AttachSession body, CancellationToken ct)
    {
        var session = await OwnerAsync(id, userId, deviceId, ct); Incarnation(session, body.IncarnationId);
        if (body.RoomId is { } roomId) await rooms.RequireMemberAsync(roomId, userId, ct);
        session.RoomId = body.RoomId; await db.SaveChangesAsync(ct); await NotifyAsync(session, ct); relay.Notify(userId, "rooms");
        return await DescribeAsync(session, ct);
    }
    public async Task<SessionDto> ShareAsync(Guid id, Guid userId, string deviceId, ShareSession body, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        if (body.UserIds is null || body.UserIds.Length > Limits.MaxSessionMembers || body.UserIds.Any(x => x == Guid.Empty || x == userId))
            throw ApiException.Invalid("Choose up to 64 friends, excluding yourself.");
        var selected = body.UserIds.Distinct().ToHashSet();
        var accepted = await db.Friendships.AsNoTracking()
            .Where(x => x.Accepted && ((x.FirstUserId == userId && selected.Contains(x.SecondUserId))
                || (x.SecondUserId == userId && selected.Contains(x.FirstUserId))))
            .Select(x => x.FirstUserId == userId ? x.SecondUserId : x.FirstUserId).ToListAsync(ct);
        if (!selected.SetEquals(accepted)) throw ApiException.Forbidden("Session recipients must be accepted friends.");
        var existing = await db.SessionMembers.Where(x => x.SessionId == id).ToListAsync(ct);
        if (selected.SetEquals(existing.Select(x => x.UserId))) return await DescribeAsync(session, ct);
        var affected = existing.Select(x => x.UserId).Union(selected).Append(userId).ToArray();
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        db.SessionMembers.RemoveRange(existing.Where(x => !selected.Contains(x.UserId)));
        foreach (var recipient in selected.Except(existing.Select(x => x.UserId))) db.SessionMembers.Add(new SessionMember { SessionId = id, UserId = recipient });
        await InvalidateAsync(session, ct); await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        relay.Invalidate(session); foreach (var recipient in affected) relay.Notify(recipient, "sessions");
        return await DescribeAsync(session, ct);
    }
    public async Task LeaveAsync(Guid id, Guid userId, LeaveSession body, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        if (session.OwnerUserId == userId) throw ApiException.Invalid("The owner cannot leave their own session.");
        var member = await db.SessionMembers.SingleOrDefaultAsync(x => x.SessionId == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        db.SessionMembers.Remove(member); await InvalidateAsync(session, ct);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct); relay.Invalidate(session);
        await NotifyAsync(session, ct); relay.Notify(userId, "sessions");
    }
    public async Task EndAsync(Guid id, Guid userId, string deviceId, Guid incarnationId, CancellationToken ct)
    {
        var session = await db.Sessions.SingleOrDefaultAsync(x => x.Id == id, ct);
        if (session is null) return;
        if (session.OwnerUserId != userId || session.HostDeviceId != deviceId) throw ApiException.Forbidden();
        Incarnation(session, incarnationId);
        if (session.Ended) return;
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        session.Ended = true; session.Ready = false;
        session.ExpiresAt = clock.GetUtcNow() + PublicationCleanup.GracePeriod;
        relay.Invalidate(session, notifyHost: false);
        await db.SessionKeys.Where(x => x.SessionId == id).ExecuteDeleteAsync(ct);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct); relay.RemoveSession(id);
        await NotifyAsync(session, ct);
    }
    public async Task<SessionDto> RotateAsync(Guid id, Guid userId, string deviceId, RotateKeys body, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        if (session.KeyGeneration != body.ExpectedGeneration) throw ApiException.Conflict("Session key generation changed.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        session.KeyGeneration = checked(session.KeyGeneration + 1); session.Ready = false;
        relay.Invalidate(session, notifyHost: false);
        await db.SessionKeys.Where(x => x.SessionId == id).ExecuteDeleteAsync(ct);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct); relay.Invalidate(session, notifyHost: false);
        return await DescribeAsync(session, ct);
    }
    public async Task<object> AuthorizedDevicesAsync(Guid id, Guid userId, string deviceId, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct);
        var recipients = await RecipientsAsync(session, ct);
        return new { session.AuthorizationRevision, session.KeyGeneration, devices = recipients.Select(x => new { x.UserId, deviceId = x.Id }) };
    }
    public async Task PublishKeysAsync(Guid id, Guid userId, string deviceId, PublishKeys body, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct); Match(session, body.IncarnationId, body.AuthorizationRevision);
        if (body.KeyGeneration <= 0 || session.KeyGeneration != body.KeyGeneration) throw ApiException.Conflict("Session key generation changed.");
        var recipients = await RecipientsAsync(session, ct);
        var host = await devices.RequireDeviceAsync(userId, deviceId, ct);
        if (body.Blobs is null || body.Blobs.Length > recipients.Count || body.Blobs.Any(x => x is null)
            || body.Blobs.Select(x => x.RecipientDeviceId).Distinct().Count() != body.Blobs.Length
            || recipients.Where(x => x.UserId == userId).Any(x => !body.Blobs.Any(blob => blob.RecipientDeviceId == x.Id)))
            throw ApiException.Invalid("Publish one key envelope for each owner device, and only trusted authorized friend devices.");
        var expected = recipients.ToDictionary(x => x.Id, StringComparer.Ordinal);
        var validated = new List<SessionKeyEnvelope>();
        foreach (var blob in body.Blobs)
        {
            if (!expected.TryGetValue(blob.RecipientDeviceId, out var recipient) || recipient.UserId != blob.RecipientUserId
                || blob.SenderDeviceId != deviceId || blob.SignatureVersion != 2
                || blob.IssuedAtMs < 0 || blob.IssuedAtMs > clock.GetUtcNow().ToUnixTimeMilliseconds() + 300_000)
                throw ApiException.Invalid("Key envelope does not match the authorized recipient and host.");
            var bytes = Limits.Base64(blob.EncryptedSessionKey, "Encrypted session key", 8192);
            var signature = Limits.Base64(blob.Signature, "Session key signature", IdentityWireFormat.MlDsa65SignatureLength);
            if (!signatures.Verify(host.SigningPublicKey, Proofs.SessionKey(id, session.IncarnationId, recipient.Id, bytes, (uint)session.KeyGeneration, (ulong)blob.IssuedAtMs), signature))
                throw ApiException.Forbidden("Invalid session key signature.");
            validated.Add(new SessionKeyEnvelope
            {
                SessionId = id,
                RecipientDeviceId = recipient.Id,
                RecipientUserId = recipient.UserId,
                SenderDeviceId = deviceId,
                KeyGeneration = session.KeyGeneration,
                IssuedAtMs = blob.IssuedAtMs,
                EncryptedKey = bytes,
                Signature = signature
            });
        }
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.SessionKeys.Where(x => x.SessionId == id).ExecuteDeleteAsync(ct); db.SessionKeys.AddRange(validated);
        session.Ready = true; await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        relay.MarkReady(session); await NotifyAsync(session, ct);
    }
    public async Task<object> MyKeyAsync(Guid id, Guid userId, string deviceId, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct); await devices.RequireDeviceAsync(userId, deviceId, ct);
        var blob = await db.SessionKeys.AsNoTracking().SingleOrDefaultAsync(x => x.SessionId == id && x.RecipientDeviceId == deviceId && x.RecipientUserId == userId && x.KeyGeneration == session.KeyGeneration, ct);
        if (!session.Ready || blob is null)
        {
            relay.RequestKeys(session); return new { state = "pendingDistribution", session.AuthorizationRevision };
        }
        var host = await devices.RequireDeviceAsync(session.OwnerUserId, session.HostDeviceId, ct);
        return new
        {
            state = "ready",
            session.AuthorizationRevision,
            keyBlob = new
            {
                session.IncarnationId,
                incarnationProtocolVersion = 11,
                encryptedSessionKey = Convert.ToBase64String(blob.EncryptedKey),
                blob.SenderDeviceId,
                senderKemPublicKey = Convert.ToBase64String(host.KemPublicKey),
                signature = Convert.ToBase64String(blob.Signature),
                signatureVersion = 2,
                blob.KeyGeneration,
                blob.IssuedAtMs
            }
        };
    }
    internal async Task<List<Device>> RecipientsAsync(Session session, CancellationToken ct)
    {
        var ids = await db.SessionMembers.Where(x => x.SessionId == session.Id).Select(x => x.UserId).ToListAsync(ct); ids.Add(session.OwnerUserId);
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        var recipients = await db.Devices.AsNoTracking().Where(d => ids.Contains(d.UserId) && !d.Revoked &&
            (d.ExpiresAtMs == null || d.ExpiresAtMs > now) && db.DeviceLists.Any(l => l.UserId == d.UserId && (l.ExpiresAtMs == null || l.ExpiresAtMs > now))).ToListAsync(ct);
        if (recipients.Count > 1024) throw ApiException.Conflict("Too many recipient devices.");
        return recipients;
    }
    private async Task InvalidateAsync(Session session, CancellationToken ct)
    {
        session.AuthorizationRevision = checked(session.AuthorizationRevision + 1); session.KeyGeneration = checked(session.KeyGeneration + 1); session.Ready = false;
        relay.Invalidate(session, notifyHost: false);
        await db.SessionKeys.Where(x => x.SessionId == session.Id).ExecuteDeleteAsync(ct);
    }
    private async Task NotifyAsync(Session session, CancellationToken ct)
    {
        relay.Notify(session.OwnerUserId, "sessions");
        foreach (var user in await db.SessionMembers.Where(x => x.SessionId == session.Id).Select(x => x.UserId).ToListAsync(ct)) relay.Notify(user, "sessions");
    }
    internal static void Incarnation(Session session, Guid expected)
    { if (expected == Guid.Empty || session.IncarnationId != expected) throw ApiException.Conflict("This session publication was replaced."); }
    internal static void Match(Session session, Guid incarnation, long revision)
    { Incarnation(session, incarnation); if (session.AuthorizationRevision != revision) throw ApiException.Conflict("Session sharing changed; refresh before retrying."); }

    public sealed record SessionDto(Guid Id, Guid IncarnationId, string Name, Guid OwnerUserId, string OwnerName,
        string HostDeviceId, string HostName, Guid? RoomId, string? RoomName, Guid[] SharedWith, long AuthorizationRevision, int KeyGeneration, bool Ready, bool HostOnline);
    public sealed record CreateSession(Guid Id, Guid IncarnationId, string Name, string HostDeviceId, string HostName, Guid? RoomId);
    public sealed record RenameSession(Guid IncarnationId, long ExpectedRevision, string Name);
    public sealed record AttachSession(Guid IncarnationId, Guid? RoomId);
    public sealed record ShareSession(Guid IncarnationId, long ExpectedRevision, Guid[] UserIds);
    public sealed record LeaveSession(Guid IncarnationId, long ExpectedRevision);
    public sealed record RotateKeys(Guid IncarnationId, long ExpectedRevision, int ExpectedGeneration);
    public sealed record KeyBlob(Guid RecipientUserId, string RecipientDeviceId, string EncryptedSessionKey, string SenderDeviceId, string Signature, int SignatureVersion, long IssuedAtMs);
    public sealed record PublishKeys(Guid IncarnationId, long AuthorizationRevision, int KeyGeneration, KeyBlob[] Blobs);
}
