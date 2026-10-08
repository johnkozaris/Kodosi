using System.Security.Cryptography;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService(
    KodosiDbContext db,
    SignatureVerifier signatures,
    DeviceCertificateParser certificates,
    SignedDeviceListParser lists,
    ConnectionDirectory connections,
    DeviceSessions sessions,
    TimeProvider clock)
{
    public async Task<Device> RequireDeviceAsync(Guid userId, string deviceId, CancellationToken ct, bool allowExpiredList = false)
    {
        var device = await db.Devices.AsNoTracking().SingleOrDefaultAsync(x => x.Id == deviceId && x.UserId == userId, ct);
        var list = await db.DeviceLists.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId, ct);
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        if (device is null || list is null || device.Revoked || device.ExpiresAtMs <= now || (!allowExpiredList && list.ExpiresAtMs <= now))
            throw ApiException.Forbidden("Approve this device before using shared sessions.");
        if (!lists.Parse(list.Body).Entries.Any(x => x.DeviceId == deviceId))
            throw ApiException.Forbidden("This device was removed.");
        return device;
    }

    public async Task<Device> RequireProofAsync(HttpContext context, Guid userId, CancellationToken ct, bool allowExpiredList = false)
    {
        var deviceId = context.Request.Headers["X-Kodosi-Device-Id"].ToString();
        if (!sessions.Holds(userId, deviceId, context.Request.Headers["X-Kodosi-Device-Session"]))
            throw new ApiException(428, "A device session is required.");
        return await RequireDeviceAsync(userId, deviceId, ct, allowExpiredList);
    }

    public object CreateChallenge(Guid userId)
    {
        var challenge = sessions.NewChallenge(userId);
        return new { challengeId = challenge.Id, challengeBytes = Convert.ToBase64String(challenge.Bytes), challenge.ExpiresAt };
    }

    public async Task<object> OpenSessionAsync(Guid userId, OpenDeviceSession request, CancellationToken ct)
    {
        var deviceId = DeviceIdRules.Require(request.DeviceId);
        var challenge = sessions.ConsumeChallenge(userId, request.ChallengeId);
        var device = await RequireDeviceAsync(userId, deviceId, ct, allowExpiredList: true);
        var signature = Limits.Base64(request.Signature, "Device signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!signatures.Verify(device.SigningPublicKey, Proofs.DeviceSession(userId, deviceId, request.ChallengeId, challenge), signature))
            throw ApiException.Forbidden("Invalid device proof.");
        var (token, expiresAt) = sessions.Open(userId, deviceId);
        return new { session = token, expiresAt };
    }

    public async Task EnrollAsync(Guid userId, RegisterDevice request, CancellationToken ct)
    {
        var existing = await db.Devices.AsNoTracking().SingleOrDefaultAsync(x => x.Id == request.DeviceId, ct);
        var certBytes = Limits.Base64(request.DeviceCertificate, "Device certificate", IdentityWireFormat.MaxDeviceCertificateBodyLength);
        var certSig = Limits.Base64(request.DeviceCertificateSignature, "Certificate signature", IdentityWireFormat.MlDsa65SignatureLength);
        var listBytes = Limits.Base64(request.SignedDeviceList, "Signed device list", IdentityWireFormat.MaxSignedDeviceListBodyLength);
        var listSig = Limits.Base64(request.SignedDeviceListSignature, "List signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (existing is not null)
        {
            if (existing.UserId == userId && !existing.Revoked && existing.Certificate.AsSpan().SequenceEqual(certBytes)
                && existing.CertificateSignature.AsSpan().SequenceEqual(certSig)) return;
            throw ApiException.Conflict("This device identity is already registered.");
        }
        var challenge = sessions.ConsumeChallenge(userId, request.ChallengeId);
        var signingKey = Limits.Base64(request.SigningPublicKey, "Signing key", IdentityWireFormat.MlDsa65PublicKeyLength);
        var pop = Limits.Base64(request.PopSignature, "Possession signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!signatures.Verify(signingKey, Proofs.Tagged(DomainTags.DevicePopV1, challenge), pop))
            throw ApiException.Forbidden("The device possession proof is invalid.");
        if (await db.DeviceLists.AnyAsync(x => x.UserId == userId, ct)
            || await db.Devices.AnyAsync(x => x.UserId == userId && !x.Revoked, ct))
            throw ApiException.Forbidden("An existing trusted device must approve this device.");
        var cert = certificates.Parse(certBytes);
        var list = lists.Parse(listBytes);
        ValidateCertificate(userId, request.DeviceId, cert, signingKey);
        ValidateTime(cert.IssuedAtMs, cert.ExpiresAtMs);
        ValidateTime(list.IssuedAtMs, list.ExpiresAtMs);
        if (!cert.IsSelfSigned || list.UserId != userId.ToString("D") || list.Generation != 1
            || list.Entries.Count != 1 || list.Entries[0].DeviceId != cert.DeviceId
            || list.Entries[0].SignerDeviceId != cert.DeviceId || list.SignerDeviceId != cert.DeviceId)
            throw ApiException.Invalid("The first device must provide one self-signed identity.");
        VerifySignature(signingKey, DomainTags.DeviceCertV3, certBytes, certSig);
        VerifySignature(signingKey, DomainTags.DeviceListV1, listBytes, listSig);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        db.Devices.Add(ToDevice(userId, cert, certBytes, certSig));
        db.DeviceLists.Add(ToList(userId, list, listBytes, listSig));
        var user = await db.Users.SingleAsync(x => x.Id == userId, ct);
        user.IdentityIncarnationId = Guid.CreateVersion7(); user.IdentityRevision = 1;
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        connections.Notify(userId, "devices");
    }

    public async Task ReplaceListAsync(Guid userId, ReplaceDeviceList request, CancellationToken ct)
    {
        var current = await db.DeviceLists.SingleOrDefaultAsync(x => x.UserId == userId, ct)
            ?? throw ApiException.Missing();
        var bytes = Limits.Base64(request.SignedDeviceList, "Signed device list", IdentityWireFormat.MaxSignedDeviceListBodyLength);
        var signature = Limits.Base64(request.SignedDeviceListSignature, "List signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (current.Body.AsSpan().SequenceEqual(bytes) && current.Signature.AsSpan().SequenceEqual(signature)) return;
        var next = lists.Parse(bytes);
        var previous = lists.Parse(current.Body);
        var signer = await RequireDeviceAsync(userId, next.SignerDeviceId, ct, allowExpiredList: true);
        ValidateSuccessor(userId, previous, next);
        var old = previous.Entries.ToDictionary(x => x.DeviceId);
        if (next.Entries.Count == 0 || next.Entries.Any(x => !old.TryGetValue(x.DeviceId, out var entry) || entry.SignerDeviceId != x.SignerDeviceId))
            throw ApiException.Invalid("Device removal cannot add or change existing device identities.");
        VerifySignature(signer.SigningPublicKey, DomainTags.DeviceListV1, bytes, signature);
        var retained = next.Entries.Select(x => x.DeviceId).ToHashSet(StringComparer.Ordinal);
        var removed = await db.Devices.Where(x => x.UserId == userId && !x.Revoked && !retained.Contains(x.Id)).ToListAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        foreach (var device in removed) { device.Revoked = true; connections.RemoveDevice(userId, device.Id); }
        UpdateList(current, next, bytes, signature);
        var affected = removed.Count == 0 ? [] : await InvalidateUserSessionsAsync(userId, removed, ct);
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        foreach (var device in removed) { sessions.Remove(userId, device.Id); connections.RemoveDevice(userId, device.Id); }
        await PublishSessionChangesAsync(userId, affected, removed, ct);
        connections.Notify(userId, "devices");
    }

    public static readonly TimeSpan ReauthenticationWindow = TimeSpan.FromMinutes(15);

    public async Task ResetIdentityAsync(Guid userId, DateTimeOffset? authenticatedAt, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        if (authenticatedAt is null || authenticatedAt > now.AddMinutes(5) || now - authenticatedAt > ReauthenticationWindow)
            throw ApiException.Forbidden("Sign in again to start fresh on this device.");
        var devices = await db.Devices.Where(x => x.UserId == userId && !x.Revoked).ToListAsync(ct);
        var list = await db.DeviceLists.SingleOrDefaultAsync(x => x.UserId == userId, ct);
        if (list is null && devices.Count == 0) return;
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        foreach (var device in devices) device.Revoked = true;
        if (list is not null) db.DeviceLists.Remove(list);
        await db.DeviceLinks.Where(x => x.UserId == userId && x.State == "pending")
            .ExecuteUpdateAsync(set => set.SetProperty(x => x.State, "cancelled"), ct);
        var user = await db.Users.SingleAsync(x => x.Id == userId, ct);
        user.IdentityIncarnationId = null;
        var affected = await InvalidateUserSessionsAsync(userId, devices, ct);
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        sessions.Remove(userId);
        foreach (var device in devices) connections.RemoveDevice(userId, device.Id);
        await PublishSessionChangesAsync(userId, affected, devices, ct);
        connections.Notify(userId, "devices");
    }

    public async Task<(IdentityBundle? Bundle, string Tag)> IdentityAsync(Guid callerId, Guid userId, string? known, CancellationToken ct)
    {
        if (callerId != userId)
        {
            var (first, second) = Friends.FriendService.Pair(callerId, userId);
            var friend = await db.Friendships.AnyAsync(x => x.FirstUserId == first && x.SecondUserId == second && x.Accepted, ct);
            var shared = await db.Sessions.AnyAsync(s => !s.Ended &&
                ((s.OwnerUserId == userId && db.SessionMembers.Any(m => m.SessionId == s.Id && m.UserId == callerId)) ||
                 (s.OwnerUserId == callerId && db.SessionMembers.Any(m => m.SessionId == s.Id && m.UserId == userId))), ct);
            var room = await db.Missions.AnyAsync(r =>
                (r.OwnerUserId == callerId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == callerId) || db.MissionInvitations.Any(i => i.MissionId == r.Id && i.UserId == callerId))
                && (r.OwnerUserId == userId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == userId) || db.MissionInvitations.Any(i => i.MissionId == r.Id && i.UserId == userId)), ct);
            if (!friend && !shared && !room) throw ApiException.Missing();
        }
        var user = await db.Users.AsNoTracking().SingleOrDefaultAsync(x => x.Id == userId, ct)
            ?? throw ApiException.Missing();
        var list = await db.DeviceLists.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId, ct)
            ?? throw ApiException.Missing();
        if (callerId != userId) ValidateTime(list.IssuedAtMs, list.ExpiresAtMs);
        var incarnation = user.IdentityIncarnationId ?? throw ApiException.Conflict("Identity is not enrolled.");
        var tag = $"\"{incarnation:N}-{list.Generation}\"";
        if (known == tag) return (null, tag);
        var devices = await db.Devices.AsNoTracking().Where(x => x.UserId == userId).ToListAsync(ct);
        var ids = lists.Parse(list.Body).Entries.Select(x => x.DeviceId).ToHashSet(StringComparer.Ordinal);
        var active = devices.Where(x => ids.Contains(x.Id) && !x.Revoked).ToArray();
        var all = devices.ToDictionary(x => x.Id, StringComparer.Ordinal);
        var ancestors = new Dictionary<string, Device>(StringComparer.Ordinal);
        foreach (var device in active)
        {
            var signer = device.SignerDeviceId;
            var visited = new HashSet<string>(StringComparer.Ordinal) { device.Id };
            while (signer != device.Id && visited.Add(signer) && all.TryGetValue(signer, out var ancestor))
            {
                if (!ids.Contains(signer)) ancestors.TryAdd(signer, ancestor);
                if (ancestor.SignerDeviceId == signer) break;
                signer = ancestor.SignerDeviceId;
            }
        }
        if (ancestors.Count > IdentityWireFormat.MaxEntries) throw ApiException.Conflict("Device certificate chain is too large.");
        return (new IdentityBundle(user.Id, user.IdentityRevision, incarnation,
            new SignedList(Convert.ToBase64String(list.Body), Convert.ToBase64String(list.Signature)),
            active.Select(CertificateWire).ToArray(), ancestors.Values.Select(CertificateWire).ToArray()), tag);
    }

    private async Task<List<Session>> InvalidateUserSessionsAsync(Guid userId, IReadOnlyCollection<Device> removed, CancellationToken ct)
    {
        var sessions = await db.Sessions.Where(s => !s.Ended && (s.OwnerUserId == userId || db.SessionMembers.Any(m => m.SessionId == s.Id && m.UserId == userId))).ToListAsync(ct);
        foreach (var session in sessions)
        {
            session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
            if (removed.Any(device => device.Id == session.HostDeviceId))
            {
                session.Ended = true;
                session.ExpiresAt = clock.GetUtcNow() + PublicationCleanup.GracePeriod;
            }
        }
        return sessions;
    }

    private static Func<Guid, string, bool> Keep(Guid userId, IReadOnlyCollection<Device> removed)
        => (user, device) => user != userId || !removed.Any(x => x.Id == device);

    private async Task PublishSessionChangesAsync(Guid userId, List<Session> affected, IReadOnlyCollection<Device> removed, CancellationToken ct)
    {
        foreach (var session in affected)
            if (session.Ended) connections.RemoveSession(session.Id); else connections.Revoke(session, Keep(userId, removed));
        var ended = affected.Where(session => session.Ended).Select(session => session.Id).ToArray();
        if (ended.Length == 0) return;
        connections.Notify(userId, "sessions");
        foreach (var viewer in await db.SessionMembers.Where(m => ended.Contains(m.SessionId)).Select(m => m.UserId).Distinct().ToListAsync(ct))
            connections.Notify(viewer, "sessions");
    }

    private static Certificate CertificateWire(Device device) => new(Convert.ToBase64String(device.Certificate), Convert.ToBase64String(device.CertificateSignature));
    private void ValidateTime(long issuedAt, long? expiresAt)
    {
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        if (issuedAt > now + 300_000 || expiresAt <= now)
            throw ApiException.Invalid("The device proof is expired or dated in the future.");
    }
    private static void ValidateCertificate(Guid userId, string deviceId, DeviceCertificateParser.ParsedDeviceCertificate cert, byte[] signing)
    {
        if (cert.UserId != userId.ToString("D") || cert.DeviceId != deviceId || !cert.SigPublicKey.AsSpan().SequenceEqual(signing))
            throw ApiException.Invalid("Certificate identity and keys do not match the submitted device.");
    }
    private void ValidateSuccessor(Guid userId, SignedDeviceListParser.ParsedSignedDeviceList previous, SignedDeviceListParser.ParsedSignedDeviceList next)
    {
        ValidateTime(next.IssuedAtMs, next.ExpiresAtMs);
        if (next.UserId != userId.ToString("D") || next.Generation != checked(previous.Generation + 1)
            || next.IssuedAtMs <= previous.IssuedAtMs)
            throw ApiException.Conflict("The signed device list does not extend the current generation.");
    }
    private void VerifySignature(byte[] key, ReadOnlySpan<byte> domain, byte[] body, byte[] signature)
    {
        if (!signatures.Verify(key, Proofs.Tagged(domain, body), signature)) throw ApiException.Forbidden("Invalid identity signature.");
    }
    private static Device ToDevice(Guid userId, DeviceCertificateParser.ParsedDeviceCertificate cert, byte[] bytes, byte[] signature) => new()
    {
        Id = cert.DeviceId,
        UserId = userId,
        Label = cert.DeviceLabel,
        SignerDeviceId = cert.SignerDeviceId,
        Certificate = bytes,
        CertificateSignature = signature,
        SigningPublicKey = cert.SigPublicKey,
        IssuedAtMs = cert.IssuedAtMs,
        ExpiresAtMs = cert.ExpiresAtMs,
    };
    private static DeviceList ToList(Guid userId, SignedDeviceListParser.ParsedSignedDeviceList list, byte[] bytes, byte[] signature)
    {
        var stored = new DeviceList { UserId = userId }; UpdateList(stored, list, bytes, signature); return stored;
    }
    private static void UpdateList(DeviceList stored, SignedDeviceListParser.ParsedSignedDeviceList list, byte[] bytes, byte[] signature)
    {
        stored.Generation = list.Generation; stored.SignerDeviceId = list.SignerDeviceId;
        stored.Body = bytes; stored.Signature = signature; stored.IssuedAtMs = list.IssuedAtMs; stored.ExpiresAtMs = list.ExpiresAtMs;
    }

    public sealed record RegisterDevice(string DeviceId, string SigningPublicKey, Guid ChallengeId,
        string PopSignature, string DeviceCertificate, string DeviceCertificateSignature, string SignedDeviceList, string SignedDeviceListSignature);
    public sealed record ReplaceDeviceList(string SignedDeviceList, string SignedDeviceListSignature);
    public sealed record OpenDeviceSession(string DeviceId, Guid ChallengeId, string Signature);
    public sealed record Certificate(
        [property: System.Text.Json.Serialization.JsonPropertyName("certificate")] string Body,
        string CertificateSignature);
    public sealed record SignedList(string Body, string Signature);
    public sealed record IdentityBundle(Guid UserId, long IdentityRevision, Guid IdentityIncarnationId,
        SignedList DeviceList, IReadOnlyList<Certificate> Devices, IReadOnlyList<Certificate> CertificateChain);
}
