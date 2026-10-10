using System.Security.Cryptography;
using System.Text;
using Kodosi.Data;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService
{
    private const int LinkNonceLength = 16;
    private const int LinkProofLength = 32;

    public async Task<object> StartLinkAsync(Guid userId, LinkInit request, CancellationToken ct)
    {
        var deviceId = DeviceIdRules.Require(request.DeviceId);
        var label = Limits.Text(request.DeviceLabel, "Device label", 128);
        var signing = Limits.Base64(request.SigningPublicKey, "Signing key", IdentityWireFormat.MlDsa65PublicKeyLength);
        if (IsRecoveryId(deviceId)) throw ApiException.Invalid("This device identifier is for a recovery key.");
        var nonce = Limits.Base64(request.Nonce, "Nonce", LinkNonceLength);
        var proof = Limits.Base64(request.Proof, "Proof", LinkProofLength);
        if (signing.Length != IdentityWireFormat.MlDsa65PublicKeyLength || nonce.Length != LinkNonceLength || proof.Length != LinkProofLength)
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
            if (!pending.SigningPublicKey.AsSpan().SequenceEqual(signing))
                throw ApiException.Conflict("Pending device identity has different keys.");
            var retrySecret = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
            pending.DeviceCodeHash = HashCode(retrySecret);
            pending.Nonce = nonce;
            pending.Proof = proof;
            await db.SaveChangesAsync(ct);
            return new { deviceCode = retrySecret, requestId = pending.Id, pending.ExpiresAt };
        }
        if (await db.DeviceLinks.CountAsync(x => x.UserId == userId && x.State == "pending" && x.ExpiresAt > now, ct) >= 5)
            throw new ApiException(429, "Too many pending device approvals.");
        var secret = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
        var link = new DeviceLink
        {
            Id = Guid.CreateVersion7(),
            UserId = userId,
            DeviceId = deviceId,
            Label = label,
            SigningPublicKey = signing,
            Nonce = nonce,
            Proof = proof,
            DeviceCodeHash = HashCode(secret),
            ExpiresAt = now.AddMinutes(10)
        };
        db.DeviceLinks.Add(link); await db.SaveChangesAsync(ct);
        connections.Notify(userId, "devices");
        return new { deviceCode = secret, requestId = link.Id, link.ExpiresAt };
    }

    public async Task<object> PendingLinksAsync(Guid userId, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        var links = await db.DeviceLinks.AsNoTracking().Where(x => x.UserId == userId && x.State == "pending" && x.ExpiresAt > now)
            .OrderBy(x => x.Id).ToListAsync(ct);
        return links.Select(x => new
        {
            requestId = x.Id,
            x.DeviceId,
            deviceLabel = x.Label,
            signingPublicKey = Convert.ToBase64String(x.SigningPublicKey),
            nonce = Convert.ToBase64String(x.Nonce),
            proof = Convert.ToBase64String(x.Proof),
            x.ExpiresAt
        });
    }

    public async Task ApproveLinkAsync(Guid userId, Device approvingDevice, LinkApprove request, CancellationToken ct)
    {
        var link = await db.DeviceLinks.SingleOrDefaultAsync(x => x.UserId == userId && x.Id == request.RequestId, ct)
            ?? throw ApiException.Missing();
        var (certBytes, certSig, listBytes, listSig) = SignedParts(request.DeviceCertificate, request.DeviceCertificateSignature, request.SignedDeviceList, request.SignedDeviceListSignature);
        var approval = Limits.Base64(request.ApprovalProof, "Approval proof", LinkProofLength);
        if (approval.Length != LinkProofLength) throw ApiException.Invalid("The approval proof has an invalid length.");
        if (link.State == "approved")
        {
            var registered = await db.Devices.SingleOrDefaultAsync(x => x.Id == link.DeviceId && !x.Revoked, ct);
            if (registered is not null && registered.Certificate.AsSpan().SequenceEqual(certBytes)
                && registered.CertificateSignature.AsSpan().SequenceEqual(certSig)) return;
            throw ApiException.Conflict("The approval has already changed.");
        }
        if (link.State != "pending" || link.ExpiresAt <= clock.GetUtcNow()) throw ApiException.Missing();
        approvingDevice = await RequireDeviceAsync(userId, approvingDevice.Id, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var (_, generation) = await StageDeviceAsync(userId, approvingDevice, link.DeviceId, link.SigningPublicKey, certBytes, certSig, listBytes, listSig, ct);
        link.State = "approved"; link.ApprovedGeneration = generation; link.ApprovalProof = approval;
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        connections.Notify(userId, "devices");
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
        return new { state = "approved", approvalProof = Convert.ToBase64String(link.ApprovalProof ?? []) };
    }

    public async Task AcknowledgeLinkAsync(Guid userId, LinkAcknowledge request, CancellationToken ct)
    {
        if (string.IsNullOrEmpty(request.DeviceCode) || request.DeviceCode.Length != 64) throw ApiException.Invalid("Invalid device code.");
        var hash = HashCode(request.DeviceCode);
        await db.DeviceLinks.Where(x => x.UserId == userId && x.DeviceCodeHash == hash && x.DeviceId == request.DeviceId && x.State == "approved").ExecuteDeleteAsync(ct);
    }

    public async Task CancelLinkAsync(Guid userId, Guid requestId, CancellationToken ct)
    {
        var changed = await db.DeviceLinks.Where(x => x.UserId == userId && x.Id == requestId && x.State == "pending")
            .ExecuteUpdateAsync(set => set.SetProperty(x => x.State, "cancelled"), ct);
        if (changed != 1) throw ApiException.Missing();
        connections.Notify(userId, "devices");
    }

    private static string HashCode(string value) => Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(value)));

    public sealed record LinkInit(string DeviceId, string DeviceLabel, string SigningPublicKey, string Nonce, string Proof);
    public sealed record LinkApprove(Guid RequestId, string DeviceCertificate, string DeviceCertificateSignature, string SignedDeviceList, string SignedDeviceListSignature, string ApprovalProof);
    public sealed record LinkPoll(string DeviceCode);
    public sealed record LinkAcknowledge(string DeviceCode, string DeviceId);
}
