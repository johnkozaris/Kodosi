using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService(KodosiDbContext db, ConnectionDirectory connections, DeviceSessions sessions, TimeProvider clock,
    PlanCatalog plans)
{
    public async Task<ApprovedDevice> RequireDeviceAsync(Guid userId, string deviceId, CancellationToken ct, bool allowExpiredList = false)
    {
        var found = await (from device in db.Devices
                           join list in db.DeviceLists on device.UserId equals list.UserId
                           where device.Id == deviceId && device.UserId == userId
                           select new { device.SigningPublicKey, device.IssuedAtMs, device.ExpiresAtMs, device.Revoked, ListBody = list.Body, ListExpiresAtMs = list.ExpiresAtMs })
            .SingleOrDefaultAsync(ct);
        if (found is null || !DeviceApproval.IsCurrent(found.Revoked, found.ExpiresAtMs, found.ListExpiresAtMs, clock.GetUtcNow().ToUnixTimeMilliseconds(), allowExpiredList))
            throw ApiException.Forbidden("Approve this device before using shared sessions.");
        if (!DeviceApproval.Listed(found.ListBody).Contains(deviceId))
            throw ApiException.Forbidden("This device was removed.");
        return new ApprovedDevice(deviceId, userId, found.SigningPublicKey, found.IssuedAtMs, found.ExpiresAtMs);
    }

    public async Task<HashSet<(Guid UserId, string DeviceId)>> ApprovedAsync(IReadOnlyCollection<(Guid UserId, string DeviceId)> candidates, CancellationToken ct)
    {
        var ids = candidates.Select(x => x.DeviceId).Distinct().ToArray();
        var users = candidates.Select(x => x.UserId).Distinct().ToArray();
        var devices = await db.Devices.Where(x => ids.Contains(x.Id)).Select(x => new { x.Id, x.UserId, x.Revoked, x.ExpiresAtMs }).ToListAsync(ct);
        var lists = await db.DeviceLists.Where(x => users.Contains(x.UserId)).Select(x => new { x.UserId, x.Body, x.ExpiresAtMs }).ToDictionaryAsync(x => x.UserId, ct);
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        var listed = new Dictionary<Guid, HashSet<string>>();
        var approved = new HashSet<(Guid, string)>();
        foreach (var device in devices)
        {
            if (!lists.TryGetValue(device.UserId, out var list)
                || !DeviceApproval.IsCurrent(device.Revoked, device.ExpiresAtMs, list.ExpiresAtMs, now, allowExpiredList: false))
                continue;
            if (!listed.TryGetValue(device.UserId, out var entries)) listed[device.UserId] = entries = DeviceApproval.Listed(list.Body);
            if (entries.Contains(device.Id)) approved.Add((device.UserId, device.Id));
        }
        return approved;
    }

    public async Task<ApprovedDevice> RequireProofAsync(HttpContext context, Guid userId, CancellationToken ct, bool allowExpiredList = false)
    {
        var deviceId = context.Request.Headers["X-Kodosi-Device-Id"].ToString();
        if (!sessions.Holds(userId, deviceId, context.Request.Headers["X-Kodosi-Device-Session"]))
            throw new ApiException(428, "A device session is required.");
        return await RequireDeviceAsync(userId, deviceId, ct, allowExpiredList);
    }

    public object CreateChallenge(Guid userId)
    {
        var challenge = sessions.NewChallenge(userId);
        return new { challengeId = challenge.Id, challengeBytes = Convert.ToBase64String(challenge.Bytes) };
    }

    public async Task<object> OpenSessionAsync(Guid userId, OpenDeviceSession request, CancellationToken ct)
    {
        var deviceId = DeviceIdRules.Require(request.DeviceId);
        var challenge = sessions.ConsumeChallenge(userId, request.ChallengeId);
        var device = await RequireDeviceAsync(userId, deviceId, ct, allowExpiredList: true);
        if (IsRecoveryId(deviceId)) throw ApiException.Forbidden("A recovery key cannot open a device session.");
        var signature = Limits.Base64(request.Signature, "Device signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!SignatureVerifier.Verify(device.SigningPublicKey, Proofs.DeviceSession(userId, deviceId, request.ChallengeId, challenge), signature))
            throw ApiException.Forbidden("Invalid device proof.");
        return new { session = sessions.Open(userId, deviceId) };
    }

    private static (byte[] Cert, byte[] CertSig, byte[] List, byte[] ListSig) SignedParts(string certificate, string certificateSignature, string list, string listSignature) =>
        (Limits.Base64(certificate, "Device certificate", IdentityWireFormat.MaxDeviceCertificateBodyLength),
         Limits.Base64(certificateSignature, "Certificate signature", IdentityWireFormat.MlDsa65SignatureLength),
         Limits.Base64(list, "Signed device list", IdentityWireFormat.MaxSignedDeviceListBodyLength),
         Limits.Base64(listSignature, "List signature", IdentityWireFormat.MlDsa65SignatureLength));

    private async Task<byte[]?> NewDeviceKeyAsync(Guid userId, RegisterDevice request, byte[] certBytes, byte[] certSig, CancellationToken ct)
    {
        var existing = await db.Devices.AsNoTracking().SingleOrDefaultAsync(x => x.Id == request.DeviceId, ct);
        if (existing is not null)
        {
            if (existing.UserId == userId && !existing.Revoked && existing.Certificate.AsSpan().SequenceEqual(certBytes)
                && existing.CertificateSignature.AsSpan().SequenceEqual(certSig)) return null;
            throw ApiException.Conflict("This device identity is already registered.");
        }
        var challenge = sessions.ConsumeChallenge(userId, request.ChallengeId);
        var signingKey = Limits.Base64(request.SigningPublicKey, "Signing key", IdentityWireFormat.MlDsa65PublicKeyLength);
        var pop = Limits.Base64(request.PopSignature, "Possession signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!SignatureVerifier.Verify(signingKey, Proofs.Tagged(DomainTags.DevicePopV1, challenge), pop))
            throw ApiException.Forbidden("The device possession proof is invalid.");
        if (IsRecoveryId(request.DeviceId)) throw ApiException.Invalid("This device identifier is for a recovery key.");
        return signingKey;
    }

    internal static bool IsRecoveryId(string deviceId) => Guid.TryParse(deviceId, out var id) && id.Version == 8;

    public async Task EnrollAsync(Guid userId, RegisterDevice request, CancellationToken ct)
    {
        var (certBytes, certSig, listBytes, listSig) = SignedParts(request.DeviceCertificate, request.DeviceCertificateSignature, request.SignedDeviceList, request.SignedDeviceListSignature);
        if (await NewDeviceKeyAsync(userId, request, certBytes, certSig, ct) is not { } signingKey) return;
        if (await db.DeviceLists.AnyAsync(x => x.UserId == userId, ct)
            || await db.Devices.AnyAsync(x => x.UserId == userId && !x.Revoked, ct))
            throw ApiException.Forbidden("An existing trusted device must approve this device.");
        var cert = DeviceCertificate.Parse(certBytes);
        var list = SignedDeviceList.Parse(listBytes);
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
        connections.Notify([userId], "devices");
    }

    public async Task ReplaceListAsync(Guid userId, ReplaceDeviceList request, CancellationToken ct)
    {
        var current = await db.DeviceLists.SingleOrDefaultAsync(x => x.UserId == userId, ct)
            ?? throw ApiException.Missing();
        var bytes = Limits.Base64(request.SignedDeviceList, "Signed device list", IdentityWireFormat.MaxSignedDeviceListBodyLength);
        var signature = Limits.Base64(request.SignedDeviceListSignature, "List signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (current.Body.AsSpan().SequenceEqual(bytes) && current.Signature.AsSpan().SequenceEqual(signature)) return;
        var next = SignedDeviceList.Parse(bytes);
        var previous = SignedDeviceList.Parse(current.Body);
        var signer = await RequireDeviceAsync(userId, next.SignerDeviceId, ct, allowExpiredList: true);
        ValidateSuccessor(userId, previous, next);
        var old = previous.Entries.ToDictionary(x => x.DeviceId);
        if (next.Entries.Count == 0 || next.Entries.Any(x => !old.TryGetValue(x.DeviceId, out var entry) || entry.SignerDeviceId != x.SignerDeviceId))
            throw ApiException.Invalid("Device removal cannot add or change existing device identities.");
        VerifySignature(signer.SigningPublicKey, DomainTags.DeviceListV1, bytes, signature);
        var retained = next.Entries.Select(x => x.DeviceId).ToHashSet(StringComparer.Ordinal);
        var removed = await db.Devices.Where(x => x.UserId == userId && !x.Revoked && !retained.Contains(x.Id)).ToListAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        foreach (var device in removed) device.Revoked = true;
        UpdateList(current, next, bytes, signature);
        var affected = removed.Count == 0 ? [] : await InvalidateUserSessionsAsync(userId, removed, ct);
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        foreach (var device in removed) { sessions.Remove(userId, device.Id); connections.RemoveDevice(userId, device.Id); }
        await PublishSessionChangesAsync(userId, affected, removed, ct);
        connections.Notify([userId], "devices");
        await RoomMembership.NotifyRoomsOfAsync(db, connections, userId, ct);
    }

    public async Task ResetIdentityAsync(Guid userId, DateTimeOffset? authenticatedAt, CancellationToken ct)
    {
        CurrentUser.RequireRecentSignIn(authenticatedAt, clock.GetUtcNow(), "Sign in again to start fresh on this device.");
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
        connections.Notify([userId], "devices");
        await RoomMembership.NotifyRoomsOfAsync(db, connections, userId, ct);
    }

    public async Task RequireRelationAsync(Guid callerId, Guid userId, CancellationToken ct)
    {
        if (callerId == userId) return;
        var (first, second) = Friends.FriendService.Pair(callerId, userId);
        if (await db.Friendships.AnyAsync(x => x.FirstUserId == first && x.SecondUserId == second && x.Accepted, ct)) return;
        var people = TerminalAccess.People(db);
        if (await db.Sessions.AnyAsync(s => !s.Ended
                && ((s.OwnerUserId == userId && people.Any(p => p.SessionId == s.Id && p.UserId == callerId))
                    || (s.OwnerUserId == callerId && people.Any(p => p.SessionId == s.Id && p.UserId == userId))), ct))
            return;
        if (await RoomMembership.RoomsOf(db, callerId, invited: true).Intersect(RoomMembership.RoomsOf(db, userId, invited: true)).AnyAsync(ct)) return;
        throw ApiException.Missing();
    }

    public async Task<(IdentityBundle? Bundle, string Tag)> IdentityAsync(Guid callerId, Guid userId, string? known, CancellationToken ct)
    {
        await RequireRelationAsync(callerId, userId, ct);
        var user = await db.Users.AsNoTracking().Where(x => x.Id == userId)
            .Select(x => new { x.Id, x.IdentityRevision, x.IdentityIncarnationId }).SingleOrDefaultAsync(ct)
            ?? throw ApiException.Missing();
        var list = await db.DeviceLists.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId, ct)
            ?? throw ApiException.Missing();
        if (callerId != userId) ValidateTime(list.IssuedAtMs, list.ExpiresAtMs);
        var incarnation = user.IdentityIncarnationId ?? throw ApiException.Conflict("Identity is not enrolled.");
        var tag = $"\"{incarnation:N}-{list.Generation}\"";
        if (known == tag) return (null, tag);
        var devices = await db.Devices.Where(x => x.UserId == userId)
            .Select(x => new IdentityDevice(x.Id, x.SignerDeviceId, x.Revoked, x.Certificate, x.CertificateSignature)).ToListAsync(ct);
        var ids = DeviceApproval.Listed(list.Body);
        var active = devices.Where(x => ids.Contains(x.Id) && !x.Revoked).ToArray();
        var all = devices.ToDictionary(x => x.Id, StringComparer.Ordinal);
        var ancestors = new Dictionary<string, IdentityDevice>(StringComparer.Ordinal);
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
        var hosts = removed.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
        var affected = await db.Sessions.UsableBy(db, userId).Where(x => !x.Ended).ToListAsync(ct);
        foreach (var session in affected)
        {
            session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
            if (hosts.Contains(session.HostDeviceId)) SessionService.End(session, clock.GetUtcNow());
        }
        return affected;
    }

    private async Task PublishSessionChangesAsync(Guid userId, List<Session> affected, IReadOnlyCollection<Device> removed, CancellationToken ct)
    {
        var gone = removed.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
        await TerminalAccess.RevokeAsync(db, connections, affected, ct, (user, device) => user != userId || !gone.Contains(device));
        var ended = affected.Where(x => x.Ended).Select(x => x.Id).ToArray();
        if (ended.Length == 0) return;
        connections.Notify((await TerminalAccess.PeopleAsync(db, ended, ct)).SelectMany(x => x).Append(userId), "sessions");
    }

    private static Certificate CertificateWire(IdentityDevice device) => new(Convert.ToBase64String(device.Certificate), Convert.ToBase64String(device.CertificateSignature));
    private void ValidateTime(long issuedAt, long? expiresAt)
    {
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        if (issuedAt > now + 300_000 || expiresAt <= now)
            throw ApiException.Invalid("The device proof is expired or dated in the future.");
    }
    private static void ValidateCertificate(Guid userId, string deviceId, DeviceCertificate cert, byte[] signing)
    {
        if (cert.UserId != userId.ToString("D") || cert.DeviceId != deviceId || !cert.SigPublicKey.AsSpan().SequenceEqual(signing))
            throw ApiException.Invalid("Certificate identity and keys do not match the submitted device.");
    }
    private void ValidateSuccessor(Guid userId, SignedDeviceList previous, SignedDeviceList next)
    {
        ValidateTime(next.IssuedAtMs, next.ExpiresAtMs);
        if (next.UserId != userId.ToString("D") || next.Generation != checked(previous.Generation + 1)
            || next.IssuedAtMs <= previous.IssuedAtMs)
            throw ApiException.Conflict("The signed device list does not extend the current generation.");
    }
    private static void VerifySignature(byte[] key, ReadOnlySpan<byte> domain, byte[] body, byte[] signature)
    {
        if (!SignatureVerifier.Verify(key, Proofs.Tagged(domain, body), signature)) throw ApiException.Forbidden("Invalid identity signature.");
    }
    private static Device ToDevice(Guid userId, DeviceCertificate cert, byte[] bytes, byte[] signature) => new()
    {
        Id = cert.DeviceId,
        UserId = userId,
        SignerDeviceId = cert.SignerDeviceId,
        Certificate = bytes,
        CertificateSignature = signature,
        SigningPublicKey = cert.SigPublicKey,
        IssuedAtMs = cert.IssuedAtMs,
        ExpiresAtMs = cert.ExpiresAtMs,
    };
    private static DeviceList ToList(Guid userId, SignedDeviceList list, byte[] bytes, byte[] signature)
    {
        var stored = new DeviceList { UserId = userId }; UpdateList(stored, list, bytes, signature); return stored;
    }
    private static void UpdateList(DeviceList stored, SignedDeviceList list, byte[] bytes, byte[] signature)
    {
        stored.Generation = list.Generation; stored.SignerDeviceId = list.SignerDeviceId;
        stored.Body = bytes; stored.Signature = signature; stored.IssuedAtMs = list.IssuedAtMs; stored.ExpiresAtMs = list.ExpiresAtMs;
    }

    private sealed record IdentityDevice(string Id, string SignerDeviceId, bool Revoked, byte[] Certificate, byte[] CertificateSignature);

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
