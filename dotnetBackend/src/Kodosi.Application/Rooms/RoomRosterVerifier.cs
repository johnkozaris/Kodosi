using System.Text.Json;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomRosterVerifier(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IPopSignatureVerifier signatureVerifier,
    TimeProvider timeProvider)
{
    private const int CurrentVersion = 1;
    private const int MaxMembers = 10_000;
    private static readonly TimeSpan MaxFutureClockSkew = TimeSpan.FromMinutes(5);

    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly IPopSignatureVerifier _signatureVerifier = signatureVerifier;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task VerifyAsync(
        RoomId roomId,
        UserId ownerUserId,
        long expectedGeneration,
        byte[] body,
        byte[] signature,
        string signerDeviceId,
        IReadOnlyCollection<UserId> expectedMembers,
        CancellationToken ct = default)
    {
        if (body is not { Length: > 0 } || body.Length > RoomInputRules.EncryptedContentMaxLength)
        {
            throw new DomainException("Room roster body is missing or too large.");
        }
        if (signature is not { Length: > 0 })
        {
            throw new DomainException("Room roster signature is required.");
        }
        if (string.IsNullOrWhiteSpace(signerDeviceId))
        {
            throw new DomainException("Room roster signer device ID is required.");
        }

        var roster = Parse(body);
        if (roster.Version != CurrentVersion
            || roster.RoomId != roomId
            || roster.OwnerUserId != ownerUserId
            || roster.Generation != expectedGeneration
            || !string.Equals(roster.SignerDeviceId, signerDeviceId, StringComparison.Ordinal))
        {
            throw new DomainException("Signed room roster metadata does not match the requested mutation.");
        }
        if (!roster.MemberUserIds.Contains(ownerUserId)
            || !roster.MemberUserIds.SetEquals(expectedMembers))
        {
            throw new DomainException("Signed room roster membership does not match the requested mutation.");
        }

        var now = _timeProvider.GetUtcNow();
        if (roster.IssuedAt <= DateTimeOffset.UnixEpoch
            || roster.IssuedAt > now + MaxFutureClockSkew)
        {
            throw new DomainException("Signed room roster issued-at timestamp is invalid.");
        }

        var device = await _devices.GetByDeviceIdAsync(signerDeviceId, ct);
        var deviceList = await _deviceLists.GetLatestAsync(ownerUserId, ct);
        if (!ActiveDeviceAuthorization.IsAuthorized(device, deviceList, ownerUserId, now))
        {
            throw new PolicyViolationException(
                "Room roster must be signed by an active owner device.");
        }

        var tag = DomainTags.RoomRosterV1;
        var preimage = new byte[tag.Length + body.Length];
        tag.CopyTo(preimage);
        body.CopyTo(preimage.AsSpan(tag.Length));
        if (!_signatureVerifier.Verify(device!.SigningPublicKey, preimage, signature))
        {
            throw new PolicyViolationException("Room roster signature is invalid.");
        }
    }

    private static ParsedRoomRoster Parse(byte[] body)
    {
        try
        {
            using var document = JsonDocument.Parse(body, new JsonDocumentOptions
            {
                MaxDepth = 8,
            });
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                throw new DomainException("Signed room roster must be a JSON object.");
            }

            HashSet<string> propertyNames = new(StringComparer.Ordinal);
            foreach (var property in root.EnumerateObject())
            {
                if (!propertyNames.Add(property.Name)
                    || property.Name is not (
                        "version"
                        or "roomId"
                        or "generation"
                        or "ownerUserId"
                        or "memberUserIds"
                        or "signerDeviceId"
                        or "issuedAtMs"))
                {
                    throw new DomainException("Signed room roster contains duplicate or unknown fields.");
                }
            }

            if (propertyNames.Count != 7
                || !root.GetProperty("version").TryGetInt32(out var version)
                || !root.GetProperty("generation").TryGetInt64(out var generation)
                || !TryReadUserId(root, "roomId", RoomId.From, out RoomId roomId)
                || !TryReadUserId(root, "ownerUserId", UserId.From, out UserId ownerUserId)
                || root.GetProperty("signerDeviceId").GetString() is not { Length: > 0 } signerDeviceId
                || !root.GetProperty("issuedAtMs").TryGetInt64(out var issuedAtMs))
            {
                throw new DomainException("Signed room roster contains malformed metadata.");
            }

            var membersElement = root.GetProperty("memberUserIds");
            if (membersElement.ValueKind != JsonValueKind.Array)
            {
                throw new DomainException("Signed room roster memberUserIds must be an array.");
            }

            HashSet<UserId> members = [];
            foreach (var memberElement in membersElement.EnumerateArray())
            {
                if (members.Count >= MaxMembers
                    || memberElement.ValueKind != JsonValueKind.String
                    || !Guid.TryParse(memberElement.GetString(), out var memberId)
                    || memberId == Guid.Empty
                    || !members.Add(UserId.From(memberId)))
                {
                    throw new DomainException("Signed room roster contains invalid or duplicate members.");
                }
            }

            DateTimeOffset issuedAt;
            try
            {
                issuedAt = DateTimeOffset.FromUnixTimeMilliseconds(issuedAtMs);
            }
            catch (ArgumentOutOfRangeException)
            {
                throw new DomainException("Signed room roster issued-at timestamp is out of range.");
            }

            return new ParsedRoomRoster(
                version,
                roomId,
                generation,
                ownerUserId,
                members,
                signerDeviceId,
                issuedAt);
        }
        catch (JsonException)
        {
            throw new DomainException("Signed room roster is not valid JSON.");
        }
        catch (InvalidOperationException)
        {
            throw new DomainException("Signed room roster contains malformed metadata.");
        }
        catch (KeyNotFoundException)
        {
            throw new DomainException("Signed room roster is missing required metadata.");
        }
    }

    private static bool TryReadUserId<T>(
        JsonElement root,
        string propertyName,
        Func<Guid, T> factory,
        out T value)
    {
        value = default!;
        var property = root.GetProperty(propertyName);
        if (property.ValueKind != JsonValueKind.String
            || !Guid.TryParse(property.GetString(), out var parsed)
            || parsed == Guid.Empty)
        {
            return false;
        }

        value = factory(parsed);
        return true;
    }

    private sealed record ParsedRoomRoster(
        int Version,
        RoomId RoomId,
        long Generation,
        UserId OwnerUserId,
        IReadOnlySet<UserId> MemberUserIds,
        string SignerDeviceId,
        DateTimeOffset IssuedAt);
}
