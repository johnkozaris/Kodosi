using System.Security.Cryptography;
using System.Text;
using Kodosi.Data;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService
{
    private const string CodeAlphabet = "BCDFGHJKLMNPQRSTVWXYZ23456789";

    public async Task<object> StartLinkAsync(Guid userId, LinkInit request, CancellationToken ct)
    {
        var deviceId = DeviceIdRules.Require(request.DeviceId);
        var label = Limits.Text(request.DeviceLabel, "Device label", 128);
        var signing = Limits.Base64(request.SigningPublicKey, "Signing key", IdentityWireFormat.MlDsa65PublicKeyLength);
        var kem = Limits.Base64(request.KemPublicKey, "KEM key", IdentityWireFormat.MlKem768PublicKeyLength);
        if (signing.Length != IdentityWireFormat.MlDsa65PublicKeyLength || kem.Length != IdentityWireFormat.MlKem768PublicKeyLength)
            throw ApiException.Invalid("Device keys have invalid lengths.");
        var now = clock.GetUtcNow();
        await db.DeviceLinks.Where(x => x.ExpiresAt < now.AddDays(-1)).ExecuteDeleteAsync(ct);
        if (!await db.DeviceLists.AnyAsync(x => x.UserId == userId, ct))
            throw ApiException.Conflict("Register the first device directly.");
        if (await db.Devices.AnyAsync(x => x.Id == deviceId, ct))
            throw ApiException.Conflict("This device identity has already been used.");
        var pending = await db.DeviceLinks.SingleOrDefaultAsync(x => x.UserId == userId && x.DeviceId == deviceId && x.State == "pending" && x.ExpiresAt > now, ct);
        if (pending is not null)
        {
            if (!pending.SigningPublicKey.AsSpan().SequenceEqual(signing) || !pending.KemPublicKey.AsSpan().SequenceEqual(kem))
                throw ApiException.Conflict("Pending device identity has different keys.");
            var retrySecret = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
            pending.DeviceCodeHash = HashCode(retrySecret);
            await db.SaveChangesAsync(ct);
            return new { deviceCode = retrySecret, userCode = pending.UserCode, pending.ExpiresAt };
        }
        if (await db.DeviceLinks.CountAsync(x => x.UserId == userId && x.State == "pending" && x.ExpiresAt > now, ct) >= 8)
            throw new ApiException(429, "Too many pending device approvals.");
        var secret = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
        var code = NewUserCode();
        while (await db.DeviceLinks.AnyAsync(x => x.UserCode == code, ct)) code = NewUserCode();
        var link = new DeviceLink
        {
            Id = Guid.CreateVersion7(),
            UserId = userId,
            DeviceId = deviceId,
            Label = label,
            SigningPublicKey = signing,
            KemPublicKey = kem,
            DeviceCodeHash = HashCode(secret),
            UserCode = code,
            ExpiresAt = now.AddMinutes(10)
        };
        db.DeviceLinks.Add(link); await db.SaveChangesAsync(ct);
        relay.Notify(userId, "devices");
        return new { deviceCode = secret, userCode = link.UserCode, link.ExpiresAt };
    }

    public async Task<object> PendingLinkAsync(Guid userId, string userCode, CancellationToken ct)
    {
        var code = NormalizeCode(userCode);
        var link = await db.DeviceLinks.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId && x.UserCode == code, ct);
        if (link is null || link.State != "pending" || link.ExpiresAt <= clock.GetUtcNow()) throw ApiException.Missing();
        return new { link.DeviceId, deviceLabel = link.Label, kemPublicKey = Convert.ToBase64String(link.KemPublicKey), signingPublicKey = Convert.ToBase64String(link.SigningPublicKey) };
    }

    public async Task<object> PendingLinksAsync(Guid userId, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        return await db.DeviceLinks.AsNoTracking().Where(x => x.UserId == userId && x.State == "pending" && x.ExpiresAt > now)
            .Select(x => new { x.UserCode, deviceLabel = x.Label, x.ExpiresAt }).ToListAsync(ct);
    }

    public async Task ApproveLinkAsync(Guid userId, Device approvingDevice, LinkApprove request, CancellationToken ct)
    {
        var code = NormalizeCode(request.UserCode);
        var link = await db.DeviceLinks.SingleOrDefaultAsync(x => x.UserId == userId && x.UserCode == code, ct)
            ?? throw ApiException.Missing();
        var certBytes = Limits.Base64(request.DeviceCertificate, "Device certificate", IdentityWireFormat.MaxDeviceCertificateBodyLength);
        var certSig = Limits.Base64(request.DeviceCertificateSignature, "Certificate signature", IdentityWireFormat.MlDsa65SignatureLength);
        var listBytes = Limits.Base64(request.SignedDeviceList, "Signed device list", IdentityWireFormat.MaxSignedDeviceListBodyLength);
        var listSig = Limits.Base64(request.SignedDeviceListSignature, "List signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (link.State == "approved")
        {
            var registered = await db.Devices.SingleOrDefaultAsync(x => x.Id == link.DeviceId && !x.Revoked, ct);
            if (registered is not null && registered.Certificate.AsSpan().SequenceEqual(certBytes)
                && registered.CertificateSignature.AsSpan().SequenceEqual(certSig)) return;
            throw ApiException.Conflict("The approval has already changed.");
        }
        if (link.State != "pending" || link.ExpiresAt <= clock.GetUtcNow()) throw ApiException.Missing();
        var current = await db.DeviceLists.SingleAsync(x => x.UserId == userId, ct);
        var previous = lists.Parse(current.Body);
        var next = lists.Parse(listBytes);
        var cert = certificates.Parse(certBytes);
        ValidateCertificate(userId, link.DeviceId, cert, link.SigningPublicKey, link.KemPublicKey);
        ValidateTime(cert.IssuedAtMs, cert.ExpiresAtMs);
        ValidateSuccessor(userId, previous, next);
        approvingDevice = await RequireDeviceAsync(userId, approvingDevice.Id, ct);
        if (cert.SignerDeviceId != approvingDevice.Id || next.SignerDeviceId != approvingDevice.Id || cert.IsSelfSigned)
            throw ApiException.Forbidden("The approving device must sign the new device and list.");
        var expected = previous.Entries.ToDictionary(x => x.DeviceId, x => x.SignerDeviceId, StringComparer.Ordinal);
        if (!expected.TryAdd(cert.DeviceId, cert.SignerDeviceId) || next.Entries.Count != expected.Count
            || next.Entries.Any(x => !expected.TryGetValue(x.DeviceId, out var signer) || signer != x.SignerDeviceId))
            throw ApiException.Invalid("Device approval must preserve the current devices and add only the requested device.");
        if (await db.Devices.AnyAsync(x => x.Id == cert.DeviceId, ct)) throw ApiException.Conflict("This device ID cannot be reused.");
        VerifySignature(approvingDevice.SigningPublicKey, DomainTags.DeviceCertV2, certBytes, certSig);
        VerifySignature(approvingDevice.SigningPublicKey, DomainTags.DeviceListV1, listBytes, listSig);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        db.Devices.Add(ToDevice(userId, cert, certBytes, certSig));
        UpdateList(current, next, listBytes, listSig);
        link.State = "approved"; link.ApprovedGeneration = next.Generation;
        var affected = await InvalidateUserSessionsAsync(userId, ct);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        foreach (var session in affected) relay.Invalidate(session);
        relay.Notify(userId, "devices");
    }

    public async Task<object> PollLinkAsync(Guid userId, string deviceCode, CancellationToken ct)
    {
        if (string.IsNullOrEmpty(deviceCode) || deviceCode.Length != 64 || deviceCode.Any(c => !Uri.IsHexDigit(c))) throw ApiException.Missing();
        var hash = HashCode(deviceCode);
        var link = await db.DeviceLinks.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId && x.DeviceCodeHash == hash, ct)
            ?? throw ApiException.Missing();
        if (link.ExpiresAt <= clock.GetUtcNow()) return new { state = "expired" };
        if (link.State != "approved") return new { state = link.State };
        if (!await db.Devices.AnyAsync(x => x.Id == link.DeviceId && !x.Revoked, ct)) return new { state = "cancelled" };
        return new { state = "approved", deviceListGeneration = link.ApprovedGeneration, identityBundle = await IdentityAsync(userId, userId, ct) };
    }

    public async Task AcknowledgeLinkAsync(Guid userId, LinkAcknowledge request, CancellationToken ct)
    {
        if (string.IsNullOrEmpty(request.DeviceCode) || request.DeviceCode.Length != 64) throw ApiException.Invalid("Invalid device code.");
        var hash = HashCode(request.DeviceCode);
        await db.DeviceLinks.Where(x => x.UserId == userId && x.DeviceCodeHash == hash && x.DeviceId == request.DeviceId && x.State == "approved").ExecuteDeleteAsync(ct);
    }

    public async Task CancelLinkAsync(Guid userId, string userCode, CancellationToken ct)
    {
        var code = NormalizeCode(userCode);
        var changed = await db.DeviceLinks.Where(x => x.UserId == userId && x.UserCode == code && x.State == "pending")
            .ExecuteUpdateAsync(set => set.SetProperty(x => x.State, "cancelled"), ct);
        if (changed != 1) throw ApiException.Missing();
        relay.Notify(userId, "devices");
    }

    private static string HashCode(string value) => Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(value)));
    private static string NewUserCode()
    {
        Span<char> chars = stackalloc char[9];
        for (var i = 0; i < chars.Length; i++) chars[i] = i == 4 ? '-' : CodeAlphabet[RandomNumberGenerator.GetInt32(CodeAlphabet.Length)];
        return new string(chars);
    }
    private static string NormalizeCode(string text)
    {
        var code = text.Trim().ToUpperInvariant();
        if (code.Length != 9 || code[4] != '-' || code.Where((_, i) => i != 4).Any(c => !CodeAlphabet.Contains(c)))
            throw ApiException.Invalid("Enter the eight-character device approval code.");
        return code;
    }

    public sealed record LinkInit(string DeviceId, string DeviceLabel, string KemPublicKey, string SigningPublicKey);
    public sealed record LinkApprove(string UserCode, string DeviceCertificate, string DeviceCertificateSignature, string SignedDeviceList, string SignedDeviceListSignature);
    public sealed record LinkPoll(string DeviceCode);
    public sealed record LinkAcknowledge(string DeviceCode, string DeviceId);
}
