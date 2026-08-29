using System.Data;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Host;

internal static class DeviceCertificateAuthorityMigrationPreflight
{
    public const string Migration = "20260820125920_CollapseUserDeviceCertificateAuthority";
    public const string DeviceListCollapseMigration =
        "20260820000539_CollapseUserDeviceListAuthority";
    public const string DeviceListCollapsePredecessor =
        "20260819104548_DropObsoleteConsumedTimestamps";

    public static async Task RequireLegacyListProjectionParityAsync(
        KodosiDbContext dbContext,
        CancellationToken ct = default)
    {
        var connection = dbContext.Database.GetDbConnection();
        var openedHere = connection.State != ConnectionState.Open;
        if (openedHere)
        {
            await connection.OpenAsync(ct);
        }
        try
        {
            await using var command = connection.CreateCommand();
            command.CommandText =
                """
                SELECT user_id, generation, device_ids::text, signer_device_id,
                       body, issued_at_ms, expires_at_ms
                FROM user_device_lists
                ORDER BY user_id, generation
                """;
            await using var reader = await command.ExecuteReaderAsync(ct);
            while (await reader.ReadAsync(ct))
            {
                var userId = reader.GetGuid(0);
                var generation = reader.GetInt64(1);
                var legacyEntries = ParseLegacyEntries(reader.GetString(2));
                var legacySigner = reader.GetString(3);
                var body = (byte[])reader.GetValue(4);
                var legacyIssuedAtMs = reader.GetInt64(5);
                var legacyExpiresAtMs = reader.IsDBNull(6)
                    ? (long?)null
                    : reader.GetInt64(6);
                SignedDeviceListParser.ParsedSignedDeviceList proof;
                try
                {
                    proof = new SignedDeviceListParser().Parse(body);
                }
                catch (SignedDeviceListFormatException exception)
                {
                    throw LegacyListFailed(userId, generation, exception.Message, exception);
                }
                var exactEntries = proof.Entries
                    .Select(entry => (entry.DeviceId, entry.SignerDeviceId))
                    .ToArray();
                if (proof.UserId != userId.ToString("D")
                    || proof.Generation != generation
                    || !exactEntries.SequenceEqual(legacyEntries)
                    || proof.SignerDeviceId != legacySigner
                    || proof.IssuedAtMs != legacyIssuedAtMs
                    || proof.ExpiresAtMs != legacyExpiresAtMs)
                {
                    throw LegacyListFailed(
                        userId,
                        generation,
                        "exact signed semantics differ from the legacy projections");
                }
            }
        }
        finally
        {
            if (openedHere)
            {
                await connection.CloseAsync();
            }
        }
    }

    public static async Task RunAsync(
        KodosiDbContext dbContext,
        IPopSignatureVerifier signatures,
        CancellationToken ct = default)
    {
        var devices = await dbContext.UserDevices
            .AsNoTracking()
            .ToListAsync(ct);
        var byDeviceId = devices.ToDictionary(device => device.DeviceId, StringComparer.Ordinal);
        var certificates = new Dictionary<
            string,
            DeviceCertificateParser.ParsedDeviceCertificate>(StringComparer.Ordinal);

        foreach (var device in devices)
        {
            device.RequireCertificateIntegrity();
            certificates.Add(
                device.DeviceId,
                new DeviceCertificateParser().Parse(device.DeviceCertificate));
        }

        foreach (var device in devices)
        {
            var certificate = certificates[device.DeviceId];
            if (!byDeviceId.TryGetValue(certificate.SignerDeviceId, out var signer))
            {
                throw Failed(device, $"signer device {certificate.SignerDeviceId} is unavailable");
            }
            if (signer.UserId != device.UserId)
            {
                throw Failed(device, $"signer device {certificate.SignerDeviceId} has a different owner");
            }

            var body = device.DeviceCertificate;
            if (!VerifyDomainSignature(
                    signatures,
                    DeviceCertificateParser.DomainTag,
                    body,
                    device.DeviceCertificateSignature,
                    signer.SigningPublicKey))
            {
                throw Failed(device, "signature verification failed");
            }
        }

        foreach (var device in devices)
        {
            RequireRootedSignerChain(device, byDeviceId, certificates);
        }

        var lists = await dbContext.UserDeviceLists
            .AsNoTracking()
            .ToListAsync(ct);
        foreach (var list in lists)
        {
            var proof = list.ParseBody();
            if (!byDeviceId.TryGetValue(proof.SignerDeviceId, out var signer))
            {
                throw Failed(list, $"signer device {proof.SignerDeviceId} is unavailable");
            }
            if (signer.UserId != list.UserId)
            {
                throw Failed(list, $"signer device {proof.SignerDeviceId} has a different owner");
            }
            foreach (var entry in proof.Entries)
            {
                if (!byDeviceId.TryGetValue(entry.DeviceId, out var listedDevice))
                {
                    throw Failed(list, $"entry device {entry.DeviceId} is unavailable");
                }
                if (listedDevice.UserId != list.UserId)
                {
                    throw Failed(list, $"entry device {entry.DeviceId} has a different owner");
                }
            }
            try
            {
                DeviceListEntrySet.RequireCertificateSignerContinuity(
                    proof.Entries.Select(entry => (entry.DeviceId, entry.SignerDeviceId)),
                    byDeviceId);
            }
            catch (DeviceEnrollmentException exception)
            {
                throw Failed(list, exception.Message, exception);
            }
            if (!VerifyDomainSignature(
                    signatures,
                    SignedDeviceListParser.DomainTag,
                    list.Body,
                    list.Signature,
                    signer.SigningPublicKey))
            {
                throw Failed(list, "signature verification failed");
            }
        }
    }

    private static void RequireRootedSignerChain(
        UserDevice origin,
        IReadOnlyDictionary<string, UserDevice> devices,
        IReadOnlyDictionary<string, DeviceCertificateParser.ParsedDeviceCertificate> certificates)
    {
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var current = origin;
        while (true)
        {
            if (!seen.Add(current.DeviceId))
            {
                throw Failed(origin, "the certificate signer chain contains a cycle");
            }
            var certificate = certificates[current.DeviceId];
            if (certificate.IsSelfSigned)
            {
                return;
            }
            current = devices[certificate.SignerDeviceId];
        }
    }

    private static (string DeviceId, string SignerDeviceId)[] ParseLegacyEntries(string json)
    {
        try
        {
            using var document = JsonDocument.Parse(json);
            if (document.RootElement.ValueKind != JsonValueKind.Array)
            {
                throw new JsonException("root must be an array");
            }
            return document.RootElement.EnumerateArray().Select(element =>
            {
                if (element.ValueKind != JsonValueKind.Object)
                {
                    throw new JsonException("every entry must be an object");
                }
                string? deviceId = null;
                string? signerDeviceId = null;
                foreach (var property in element.EnumerateObject())
                {
                    if (property.Value.ValueKind != JsonValueKind.String)
                    {
                        throw new JsonException($"{property.Name} must be a string");
                    }
                    switch (property.Name)
                    {
                        case "deviceId" when deviceId is null:
                            deviceId = property.Value.GetString();
                            break;
                        case "signerDeviceId" when signerDeviceId is null:
                            signerDeviceId = property.Value.GetString();
                            break;
                        case "deviceId" or "signerDeviceId":
                            throw new JsonException($"duplicate property {property.Name}");
                        default:
                            throw new JsonException($"unknown property {property.Name}");
                    }
                }
                if (string.IsNullOrWhiteSpace(deviceId)
                    || string.IsNullOrWhiteSpace(signerDeviceId))
                {
                    throw new JsonException("entry IDs must be non-blank");
                }
                return (deviceId, signerDeviceId);
            }).ToArray();
        }
        catch (JsonException exception)
        {
            throw new InvalidOperationException(
                $"Cannot collapse user-device list authority: legacy device_ids is invalid: "
                + exception.Message,
                exception);
        }
    }

    private static InvalidOperationException LegacyListFailed(
        Guid userId,
        long generation,
        string reason,
        Exception? innerException = null) =>
        new(
            $"Cannot collapse user-device list authority: stored list for user {userId} "
            + $"generation {generation} failed semantic preflight because {reason}.",
            innerException);

    private static bool VerifyDomainSignature(
        IPopSignatureVerifier signatures,
        ReadOnlySpan<byte> domainTag,
        ReadOnlySpan<byte> body,
        ReadOnlySpan<byte> signature,
        ReadOnlySpan<byte> signerPublicKey)
    {
        var payload = new byte[domainTag.Length + body.Length];
        domainTag.CopyTo(payload);
        body.CopyTo(payload.AsSpan(domainTag.Length));
        return signatures.Verify(signerPublicKey, payload, signature);
    }

    private static InvalidOperationException Failed(
        UserDeviceList list,
        string reason,
        Exception? innerException = null) =>
        new(
            $"Cannot enforce identity-wire v3 device-list proof bounds: stored list for "
            + $"user {list.UserId.Value} generation {list.Generation} failed cryptographic "
            + $"preflight because {reason}.",
            innerException);

    private static InvalidOperationException Failed(UserDevice device, string reason) =>
        new(
            $"Cannot collapse user-device certificate authority: stored certificate for "
            + $"user {device.UserId.Value} and device {device.DeviceId} failed cryptographic "
            + $"preflight because {reason}.");
}
