using Kodosi.Data;
using Kodosi.Missions;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService
{
    private const int RecoveryBoxLength = 4096;

    private async Task<Device> StageDeviceAsync(Guid userId, ApprovedDevice signer, string deviceId, byte[] signingKey,
        byte[] certBytes, byte[] certSig, byte[] listBytes, byte[] listSig, CancellationToken ct)
    {
        var current = await db.DeviceLists.SingleAsync(x => x.UserId == userId, ct);
        var previous = SignedDeviceList.Parse(current.Body);
        var next = SignedDeviceList.Parse(listBytes);
        var cert = DeviceCertificate.Parse(certBytes);
        ValidateCertificate(userId, deviceId, cert, signingKey);
        ValidateTime(cert.IssuedAtMs, cert.ExpiresAtMs);
        ValidateSuccessor(userId, previous, next);
        if (cert.SignerDeviceId != signer.Id || next.SignerDeviceId != signer.Id || cert.IsSelfSigned)
            throw ApiException.Forbidden("The approving device must sign the new device and list.");
        if (cert.IssuedAtMs < signer.IssuedAtMs || signer.ExpiresAtMs <= cert.IssuedAtMs)
            throw ApiException.Invalid("The device certificate was issued outside its signer's validity.");
        var expected = previous.Entries.ToDictionary(x => x.DeviceId, x => x.SignerDeviceId, StringComparer.Ordinal);
        if (!expected.TryAdd(cert.DeviceId, cert.SignerDeviceId) || next.Entries.Count != expected.Count
            || next.Entries.Any(x => !expected.TryGetValue(x.DeviceId, out var entry) || entry != x.SignerDeviceId))
            throw ApiException.Invalid("Device approval must preserve the current devices and add only the requested device.");
        if (await db.Devices.AnyAsync(x => x.Id == cert.DeviceId, ct)) throw ApiException.Conflict("This device ID cannot be reused.");
        VerifySignature(signer.SigningPublicKey, DomainTags.DeviceCertV3, certBytes, certSig);
        VerifySignature(signer.SigningPublicKey, DomainTags.DeviceListV1, listBytes, listSig);
        var device = ToDevice(userId, cert, certBytes, certSig);
        db.Devices.Add(device);
        UpdateList(current, next, listBytes, listSig);
        return device;
    }

    private Task<Device?> RecoveryDeviceAsync(Guid userId, CancellationToken ct) =>
        db.Devices.SingleOrDefaultAsync(x => x.UserId == userId && !x.Revoked && x.RecoveryBox != null, ct);

    public async Task AddRecoveryKeyAsync(Guid userId, ApprovedDevice device, RecoveryKeyWrite request, CancellationToken ct)
    {
        var (certBytes, certSig, listBytes, listSig) = SignedParts(request.DeviceCertificate, request.DeviceCertificateSignature, request.SignedDeviceList, request.SignedDeviceListSignature);
        var box = Limits.Base64(request.RoomKeyBox, "Recovery room key", RecoveryBoxLength);
        if (box.Length == 0) throw ApiException.Invalid("The recovery room key is missing.");
        var cert = DeviceCertificate.Parse(certBytes);
        if (!IsRecoveryId(cert.DeviceId)) throw ApiException.Invalid("This is not the device identifier of a recovery key.");
        var (roomKey, roomKeySignature) = MissionService.ValidRoomKey(userId, cert.DeviceId, cert.SigPublicKey, request.RoomKey);
        if (await RecoveryDeviceAsync(userId, ct) is not null) throw ApiException.Conflict("Remove the current recovery key first.");
        var added = await StageDeviceAsync(userId, device, cert.DeviceId, cert.SigPublicKey, certBytes, certSig, listBytes, listSig, ct);
        added.RecoveryBox = box;
        db.RoomRecipientKeys.Add(new RoomRecipientKey { DeviceId = added.Id, UserId = userId, PublicKey = roomKey, Signature = roomKeySignature });
        await db.SaveChangesAsync(ct);
        connections.Notify(userId, "devices");
        await RoomMembership.NotifyRoomsOfAsync(db, connections, userId, ct);
    }

    public async Task<object> RecoveryBoxAsync(Guid userId, CancellationToken ct)
    {
        var device = await RecoveryDeviceAsync(userId, ct) ?? throw ApiException.Missing();
        return new { deviceId = device.Id, roomKeyBox = Convert.ToBase64String(device.RecoveryBox!) };
    }

    public async Task RecoverAsync(Guid userId, RegisterDevice request, CancellationToken ct)
    {
        var (certBytes, certSig, listBytes, listSig) = SignedParts(request.DeviceCertificate, request.DeviceCertificateSignature, request.SignedDeviceList, request.SignedDeviceListSignature);
        if (await NewDeviceKeyAsync(userId, request, certBytes, certSig, ct) is not { } signingKey) return;
        var recovery = await RecoveryDeviceAsync(userId, ct) ?? throw ApiException.Forbidden("This account has no recovery key.");
        await StageDeviceAsync(userId, ApprovedDevice.From(recovery), request.DeviceId, signingKey, certBytes, certSig, listBytes, listSig, ct);
        await db.SaveChangesAsync(ct);
        connections.Notify(userId, "devices");
    }

    public sealed record RecoveryKeyWrite(string DeviceCertificate, string DeviceCertificateSignature,
        string SignedDeviceList, string SignedDeviceListSignature, string RoomKeyBox, MissionService.RecipientKeyWrite RoomKey);
}
