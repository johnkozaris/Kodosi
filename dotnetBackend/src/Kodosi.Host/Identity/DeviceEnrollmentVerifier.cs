using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.Host.Identity;

public sealed class DeviceEnrollmentVerifier(
    IUserDeviceRepository devices,
    IUserDeviceListRepository lists,
    IPopSignatureVerifier verifier,
    DeviceCertificateParser certParser,
    SignedDeviceListParser listParser,
    TimeProvider timeProvider) : IDeviceEnrollmentVerifier
{


    private const long IssuedAtClockSkewMs = 5 * 60 * 1000;

    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _lists = lists;
    private readonly IPopSignatureVerifier _verifier = verifier;
    private readonly DeviceCertificateParser _certParser = certParser;
    private readonly SignedDeviceListParser _listParser = listParser;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<VerifiedDeviceLinkEnrollment> VerifyAsync(
        UserId userId,
        string deviceId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        byte[] certificate,
        byte[] certificateSignature,
        byte[] signedDeviceList,
        byte[] signedDeviceListSignature,
        CancellationToken ct = default)
    {
        var verified = await VerifySelfEnrollmentAsync(
            userId,
            deviceId,
            kemPublicKey,
            signingPublicKey,
            certificate,
            certificateSignature,
            signedDeviceList,
            signedDeviceListSignature,
            ct);
        return new VerifiedDeviceLinkEnrollment(
            verified.List,
            verified.AuthorizationAt);
    }

    private async Task<VerifiedEnrollmentBundle> VerifySelfEnrollmentAsync(
        UserId userId,
        string deviceId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        byte[] certBytes,
        byte[] certSignature,
        byte[] listBytes,
        byte[] listSignature,
        CancellationToken ct = default)
    {
        var (cert, list) = ParseAndCrossCheck(
            deviceId, userId, kemPublicKey, signingPublicKey, certBytes, listBytes);
        var receivedAt = _timeProvider.GetUtcNow();
        VerifySubmittedTemporalBounds(cert, list, receivedAt);

        var currentList = await _lists.GetLatestAsync(userId, ct);

        byte[] certSignerPubKey;
        byte[] listSignerPubKey;
        DateTimeOffset authorizationAt;
        if (currentList is null)
        {
            (certSignerPubKey, listSignerPubKey) =
                VerifyBootstrapShape(cert, list, signingPublicKey);
            authorizationAt = _timeProvider.GetUtcNow();
        }
        else
        {
            VerifySubsequentStructure(cert, list, currentList);
            var existingDevices = (await _devices.GetByUserIdAsync(userId, ct))
                .ToDictionary(device => device.DeviceId, StringComparer.Ordinal);
            authorizationAt = _timeProvider.GetUtcNow();
            (certSignerPubKey, listSignerPubKey) = VerifySubsequentAuthorization(
                cert,
                list,
                currentList,
                existingDevices,
                userId,
                authorizationAt);
        }

        VerifyBundleSignatures(
            certBytes, certSignature, certSignerPubKey,
            listBytes, listSignature, listSignerPubKey);
        VerifySubmittedExpiryAtAuthorization(cert, list, authorizationAt);

        return new VerifiedEnrollmentBundle(cert, list, authorizationAt);
    }

    private (DeviceCertificateParser.ParsedDeviceCertificate Cert,
             SignedDeviceListParser.ParsedSignedDeviceList List) ParseAndCrossCheck(
        string deviceId,
        UserId userId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        byte[] certBytes,
        byte[] listBytes)
    {
        DeviceCertificateParser.ParsedDeviceCertificate cert;
        SignedDeviceListParser.ParsedSignedDeviceList list;
        try
        {
            cert = _certParser.Parse(certBytes);
            list = _listParser.Parse(listBytes);
        }
        catch (DeviceCertificateFormatException ex)
        {
            throw new DeviceEnrollmentException($"Invalid device certificate: {ex.Message}");
        }
        catch (SignedDeviceListFormatException ex)
        {
            throw new DeviceEnrollmentException($"Invalid signed device list: {ex.Message}");
        }

        if (cert.DeviceId != deviceId)
        {
            throw new DeviceEnrollmentException(
                "Certificate device_id does not match request device_id.");
        }
        var expectedUserGuid = userId.Value;
        if (!Guid.TryParse(cert.UserId, out var certUserGuid) || certUserGuid != expectedUserGuid)
        {
            throw new DeviceEnrollmentException(
                "Certificate user_id does not match authenticated user.");
        }
        if (!cert.KemPublicKey.AsSpan().SequenceEqual(kemPublicKey))
        {
            throw new DeviceEnrollmentException(
                "Certificate KEM public key does not match request.");
        }
        if (!cert.SigPublicKey.AsSpan().SequenceEqual(signingPublicKey))
        {
            throw new DeviceEnrollmentException(
                "Certificate signing public key does not match request.");
        }
        if (!Guid.TryParse(list.UserId, out var listUserGuid) || listUserGuid != expectedUserGuid)
        {
            throw new DeviceEnrollmentException(
                "Signed device list user_id does not match authenticated user.");
        }



        var newEntry = list.Entries.FirstOrDefault(e => e.DeviceId == cert.DeviceId);
        if (newEntry is null)
        {
            throw new DeviceEnrollmentException(
                "Signed device list does not contain the new device's entry.");
        }
        if (newEntry.SignerDeviceId != cert.SignerDeviceId)
        {
            throw new DeviceEnrollmentException(
                "Cert and list disagree on which device signed the new device.");
        }

        return (cert, list);
    }

    private static void VerifySubmittedTemporalBounds(
        DeviceCertificateParser.ParsedDeviceCertificate cert,
        SignedDeviceListParser.ParsedSignedDeviceList list,
        DateTimeOffset receivedAt)
    {
        var receivedAtMs = receivedAt.ToUnixTimeMilliseconds();
        if (cert.IssuedAtMs > receivedAtMs + IssuedAtClockSkewMs)
        {
            throw new DeviceEnrollmentException(
                "Device certificate IssuedAtMs is too far in the future; check your system clock.");
        }
        if (cert.ExpiresAtMs is { } certExpiresAtMs)
        {
            if (certExpiresAtMs <= cert.IssuedAtMs)
            {
                throw new DeviceEnrollmentException(
                    "Device certificate ExpiresAtMs must be strictly after IssuedAtMs.");
            }
            if (certExpiresAtMs <= receivedAtMs)
            {
                throw new DeviceEnrollmentException(
                    "Device certificate is expired at receipt time.");
            }
        }
        if (list.IssuedAtMs > receivedAtMs + IssuedAtClockSkewMs)
        {
            throw new DeviceEnrollmentException(
                "Signed device list IssuedAtMs is too far in the future; check your system clock.");
        }
        if (list.ExpiresAtMs is { } listExpiresAtMs)
        {
            if (listExpiresAtMs <= list.IssuedAtMs)
            {
                throw new DeviceEnrollmentException(
                    "Signed device list ExpiresAtMs must be strictly after IssuedAtMs.");
            }
            if (listExpiresAtMs <= receivedAtMs)
            {
                throw new DeviceEnrollmentException(
                    "Signed device list is expired at receipt time.");
            }
        }
    }

    private static void VerifySubmittedExpiryAtAuthorization(
        DeviceCertificateParser.ParsedDeviceCertificate cert,
        SignedDeviceListParser.ParsedSignedDeviceList list,
        DateTimeOffset authorizationAt)
    {
        var authorizationAtMs = authorizationAt.ToUnixTimeMilliseconds();
        if (cert.ExpiresAtMs is { } certExpiresAtMs
            && certExpiresAtMs <= authorizationAtMs)
        {
            throw new DeviceEnrollmentException(
                "Device certificate is expired at authorization time.");
        }
        if (list.ExpiresAtMs is { } listExpiresAtMs
            && listExpiresAtMs <= authorizationAtMs)
        {
            throw new DeviceEnrollmentException(
                "Signed device list is expired at authorization time.");
        }
    }

    private static (byte[] CertSignerPubKey, byte[] ListSignerPubKey) VerifyBootstrapShape(
        DeviceCertificateParser.ParsedDeviceCertificate cert,
        SignedDeviceListParser.ParsedSignedDeviceList list,
        byte[] signingPublicKey)
    {
        if (!cert.IsSelfSigned)
        {
            throw new DeviceEnrollmentException("First device's certificate must be self-signed.");
        }
        if (list.Generation != 1)
        {
            throw new DeviceEnrollmentException("First device's signed list must have generation = 1.");
        }
        if (list.Entries.Count != 1 || !list.Entries[0].IsSelfSigned)
        {
            throw new DeviceEnrollmentException(
                "First device's signed list must contain a single self-signed entry.");
        }
        if (list.SignerDeviceId != cert.DeviceId)
        {
            throw new DeviceEnrollmentException(
                "First device's signed list must be signed by itself.");
        }
        return (signingPublicKey, signingPublicKey);
    }

    private static void VerifySubsequentStructure(
        DeviceCertificateParser.ParsedDeviceCertificate cert,
        SignedDeviceListParser.ParsedSignedDeviceList list,
        UserDeviceList currentList)
    {
        if (cert.IsSelfSigned)
        {
            throw new DeviceEnrollmentException(
                "Subsequent device certificate must be signed by an already-enrolled device.");
        }
        if (list.Generation != currentList.Generation + 1)
        {
            throw new DeviceEnrollmentException(
                $"Signed list generation {list.Generation} must equal current generation + 1 ({currentList.Generation + 1}).");
        }
        var currentProof = currentList.ParseBody();
        if (list.IssuedAtMs <= currentProof.IssuedAtMs)
        {
            throw new DeviceEnrollmentException(
                "Signed list IssuedAtMs must be greater than the current generation's IssuedAtMs.");
        }
        if (list.SignerDeviceId != cert.SignerDeviceId)
        {
            throw new DeviceEnrollmentException(
                "Signed list signer must equal certificate signer.");
        }

        var previousDeviceIds = currentProof.Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        if (!previousDeviceIds.Contains(cert.SignerDeviceId))
        {
            throw new DeviceEnrollmentException(
                "Signer device is not in the current generation's entry set.");
        }
        var expectedDeviceIds = previousDeviceIds.ToHashSet(StringComparer.Ordinal);
        expectedDeviceIds.Add(cert.DeviceId);
        var submittedDeviceIds = list.Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        if (!submittedDeviceIds.SetEquals(expectedDeviceIds))
        {
            throw new DeviceEnrollmentException(
                "Device enrollment must preserve every current device and add exactly the new device; use the revocation flow to remove devices.");
        }
    }

    private static (byte[] CertSignerPubKey, byte[] ListSignerPubKey)
        VerifySubsequentAuthorization(
            DeviceCertificateParser.ParsedDeviceCertificate cert,
            SignedDeviceListParser.ParsedSignedDeviceList list,
            UserDeviceList currentList,
            IReadOnlyDictionary<string, UserDevice> existingDevices,
            UserId userId,
            DateTimeOffset authorizationAt)
    {
        DeviceListEntrySet.RequireCertificateSignerContinuity(
            list.Entries
                .Where(entry => entry.DeviceId != cert.DeviceId)
                .Select(static entry => (entry.DeviceId, entry.SignerDeviceId)),
            existingDevices);

        var signerDevice = existingDevices.GetValueOrDefault(cert.SignerDeviceId);
        if (!ActiveDeviceAuthorization.IsAuthorized(
                signerDevice,
                currentList,
                userId,
                authorizationAt)
            || signerDevice!.SigningPublicKey is not { Length: > 0 })
        {
            throw new DeviceEnrollmentException(
                "Signer device is not an enrolled, active device of the authenticated user.");
        }
        return (signerDevice.SigningPublicKey, signerDevice.SigningPublicKey);
    }

    private void VerifyBundleSignatures(
        byte[] certBytes,
        byte[] certSignature,
        byte[] certSignerPubKey,
        byte[] listBytes,
        byte[] listSignature,
        byte[] listSignerPubKey)
    {
        if (!VerifyDomainSignature(
                DeviceCertificateParser.DomainTag,
                certBytes,
                certSignature,
                certSignerPubKey))
        {
            throw new DeviceEnrollmentException("Device certificate signature is invalid.");
        }
        if (!VerifyDomainSignature(
                SignedDeviceListParser.DomainTag,
                listBytes,
                listSignature,
                listSignerPubKey))
        {
            throw new DeviceEnrollmentException("Signed device list signature is invalid.");
        }
    }

    private bool VerifyDomainSignature(
        ReadOnlySpan<byte> domainTag,
        ReadOnlySpan<byte> body,
        ReadOnlySpan<byte> signature,
        ReadOnlySpan<byte> signerPubKey)
    {
        var payload = new byte[domainTag.Length + body.Length];
        domainTag.CopyTo(payload);
        body.CopyTo(payload.AsSpan(domainTag.Length));
        return _verifier.Verify(signerPubKey, payload, signature);
    }

    private sealed record VerifiedEnrollmentBundle(
        DeviceCertificateParser.ParsedDeviceCertificate Cert,
        SignedDeviceListParser.ParsedSignedDeviceList List,
        DateTimeOffset AuthorizationAt);
}
