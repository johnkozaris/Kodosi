using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomInvitationProofVerifier(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IPopSignatureVerifier signatureVerifier,
    TimeProvider timeProvider)
{
    private const int CurrentVersion = 1;
    private static readonly TimeSpan MaxFutureClockSkew = TimeSpan.FromMinutes(5);
    private static readonly TimeSpan MaxInvitationLifetime = TimeSpan.FromDays(7);

    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly IPopSignatureVerifier _signatureVerifier = signatureVerifier;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<VerifiedRoomInvitationProposal> VerifyProposalAsync(
        Room room,
        Guid invitationId,
        UserId inviteeUserId,
        byte[] proposalBody,
        byte[] proposalSignature,
        string proposalSignerDeviceId,
        byte[] proposedRosterBody,
        CancellationToken ct = default)
    {
        RequireProofFields(
            proposalBody,
            proposalSignature,
            proposalSignerDeviceId,
            "Invitation proposal");

        var proposal = ParseProposal(proposalBody);
        var now = _timeProvider.GetUtcNow();
        if (proposal.Version != CurrentVersion
            || proposal.InvitationId != invitationId
            || proposal.RoomId != room.Id
            || proposal.OwnerUserId != room.OwnerUserId
            || proposal.InviteeUserId != inviteeUserId
            || proposal.BaseRosterGeneration != room.RosterGeneration
            || room.RosterGeneration == long.MaxValue
            || proposal.ProposedRosterGeneration != room.RosterGeneration + 1
            || !string.Equals(
                proposal.SignerDeviceId,
                proposalSignerDeviceId,
                StringComparison.Ordinal))
        {
            throw new DomainException(
                "Invitation proposal metadata does not match the requested invitation.");
        }
        if (proposal.IssuedAt <= DateTimeOffset.UnixEpoch
            || proposal.IssuedAt > now + MaxFutureClockSkew
            || proposal.ExpiresAt <= now
            || proposal.ExpiresAt <= proposal.IssuedAt
            || proposal.ExpiresAt > proposal.IssuedAt + MaxInvitationLifetime)
        {
            throw new DomainException("Invitation proposal timestamps are invalid.");
        }

        var proposedRosterBodyHash = SHA256.HashData(proposedRosterBody);
        if (!CryptographicOperations.FixedTimeEquals(
                proposal.ProposedRosterBodyHash,
                proposedRosterBodyHash))
        {
            throw new PolicyViolationException(
                "Invitation proposal does not authorize the proposed roster body.");
        }

        await VerifyDeviceSignatureAsync(
            room.OwnerUserId,
            proposalSignerDeviceId,
            DomainTags.RoomInvitationProposalV1.ToArray(),
            proposalBody,
            proposalSignature,
            "Invitation proposal",
            now,
            ct);

        return new VerifiedRoomInvitationProposal(
            proposal.BaseRosterGeneration,
            proposal.ProposedRosterGeneration,
            SHA256.HashData(proposalBody),
            proposal.IssuedAt,
            proposal.ExpiresAt);
    }

    public async Task<VerifiedRoomInvitationProposal> VerifyStoredProposalAsync(
        RoomInvitation invitation,
        Room room,
        CancellationToken ct = default)
    {
        var verified = await VerifyProposalAsync(
            room,
            invitation.Id,
            invitation.InviteeUserId,
            invitation.ProposalBody,
            invitation.ProposalSignature,
            invitation.ProposalSignerDeviceId,
            invitation.ProposedRosterBody,
            ct);
        if (verified.BaseRosterGeneration != invitation.BaseRosterGeneration
            || verified.ProposedRosterGeneration != invitation.ProposedRosterGeneration
            || verified.IssuedAt != invitation.ProposalIssuedAt
            || verified.ExpiresAt != invitation.ExpiresAt
            || !CryptographicOperations.FixedTimeEquals(
                verified.ProposalHash,
                invitation.ProposalHash))
        {
            throw new PolicyViolationException(
                "Stored invitation proposal proof is internally inconsistent.");
        }
        return verified;
    }

    public async Task<VerifiedRoomInvitationDecision> VerifyDecisionAsync(
        RoomInvitation invitation,
        UserId actingUserId,
        string expectedDecision,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId,
        CancellationToken ct = default)
    {
        RequireProofFields(
            decisionBody,
            decisionSignature,
            decisionSignerDeviceId,
            "Invitation decision");

        var decision = ParseDecision(decisionBody);
        var now = _timeProvider.GetUtcNow();
        if (actingUserId != invitation.InviteeUserId
            || decision.Version != CurrentVersion
            || decision.InvitationId != invitation.Id
            || decision.RoomId != invitation.RoomId
            || decision.InviteeUserId != invitation.InviteeUserId
            || !string.Equals(decision.Decision, expectedDecision, StringComparison.Ordinal)
            || !string.Equals(
                decision.SignerDeviceId,
                decisionSignerDeviceId,
                StringComparison.Ordinal)
            || !CryptographicOperations.FixedTimeEquals(
                decision.ProposalHash,
                invitation.ProposalHash))
        {
            throw new DomainException(
                "Invitation decision metadata does not match the pending proposal.");
        }
        if (invitation.IsExpired(now)
            || decision.IssuedAt < invitation.ProposalIssuedAt
            || decision.IssuedAt > now + MaxFutureClockSkew
            || decision.IssuedAt > invitation.ExpiresAt)
        {
            throw new DomainException("Invitation decision timestamp is invalid.");
        }

        await VerifyDeviceSignatureAsync(
            invitation.InviteeUserId,
            decisionSignerDeviceId,
            DomainTags.RoomInvitationDecisionV1.ToArray(),
            decisionBody,
            decisionSignature,
            "Invitation decision",
            now,
            ct);

        return new VerifiedRoomInvitationDecision(decision.IssuedAt);
    }

    private async Task VerifyDeviceSignatureAsync(
        UserId expectedUserId,
        string signerDeviceId,
        byte[] domainTag,
        byte[] body,
        byte[] signature,
        string proofName,
        DateTimeOffset now,
        CancellationToken ct)
    {
        var device = await _devices.GetByDeviceIdAsync(signerDeviceId, ct);
        var deviceList = await _deviceLists.GetLatestAsync(expectedUserId, ct);
        if (!ActiveDeviceAuthorization.IsAuthorized(
                device,
                deviceList,
                expectedUserId,
                now))
        {
            throw new PolicyViolationException(
                $"{proofName} must be signed by an active device for the expected user.");
        }

        var tag = domainTag.AsSpan();
        var preimage = new byte[tag.Length + body.Length];
        tag.CopyTo(preimage);
        body.CopyTo(preimage.AsSpan(tag.Length));
        if (!_signatureVerifier.Verify(device!.SigningPublicKey, preimage, signature))
        {
            throw new PolicyViolationException($"{proofName} signature is invalid.");
        }
    }

    private static ParsedProposal ParseProposal(byte[] body)
    {
        try
        {
            using var document = JsonDocument.Parse(body, new JsonDocumentOptions
            {
                MaxDepth = 4,
            });
            var root = document.RootElement;
            RequireExactProperties(
                root,
                "version",
                "invitationId",
                "roomId",
                "ownerUserId",
                "inviteeUserId",
                "baseRosterGeneration",
                "proposedRosterGeneration",
                "proposedRosterBodyHash",
                "expiresAtMs",
                "signerDeviceId",
                "issuedAtMs");

            if (!root.GetProperty("version").TryGetInt32(out var version)
                || !TryReadGuid(root, "invitationId", out var invitationId)
                || !TryReadGuid(root, "roomId", out var roomId)
                || !TryReadGuid(root, "ownerUserId", out var ownerUserId)
                || !TryReadGuid(root, "inviteeUserId", out var inviteeUserId)
                || !root.GetProperty("baseRosterGeneration").TryGetInt64(
                    out var baseRosterGeneration)
                || !root.GetProperty("proposedRosterGeneration").TryGetInt64(
                    out var proposedRosterGeneration)
                || root.GetProperty("proposedRosterBodyHash").GetString()
                    is not { Length: > 0 } proposedRosterBodyHashBase64
                || root.GetProperty("signerDeviceId").GetString()
                    is not { Length: > 0 } signerDeviceId
                || !root.GetProperty("expiresAtMs").TryGetInt64(out var expiresAtMs)
                || !root.GetProperty("issuedAtMs").TryGetInt64(out var issuedAtMs))
            {
                throw new DomainException("Invitation proposal contains malformed metadata.");
            }

            var proposedRosterBodyHash = Convert.FromBase64String(
                proposedRosterBodyHashBase64);
            if (proposedRosterBodyHash.Length != SHA256.HashSizeInBytes)
            {
                throw new DomainException(
                    "Invitation proposal roster hash must be SHA-256.");
            }

            return new ParsedProposal(
                version,
                invitationId,
                RoomId.From(roomId),
                UserId.From(ownerUserId),
                UserId.From(inviteeUserId),
                baseRosterGeneration,
                proposedRosterGeneration,
                proposedRosterBodyHash,
                DateTimeOffset.FromUnixTimeMilliseconds(expiresAtMs),
                signerDeviceId,
                DateTimeOffset.FromUnixTimeMilliseconds(issuedAtMs));
        }
        catch (FormatException)
        {
            throw new DomainException("Invitation proposal contains invalid base64.");
        }
        catch (ArgumentOutOfRangeException)
        {
            throw new DomainException("Invitation proposal timestamp is out of range.");
        }
        catch (JsonException)
        {
            throw new DomainException("Invitation proposal is not valid JSON.");
        }
        catch (InvalidOperationException)
        {
            throw new DomainException("Invitation proposal contains malformed metadata.");
        }
        catch (KeyNotFoundException)
        {
            throw new DomainException("Invitation proposal is missing required metadata.");
        }
    }

    private static ParsedDecision ParseDecision(byte[] body)
    {
        try
        {
            using var document = JsonDocument.Parse(body, new JsonDocumentOptions
            {
                MaxDepth = 4,
            });
            var root = document.RootElement;
            RequireExactProperties(
                root,
                "version",
                "invitationId",
                "proposalHash",
                "roomId",
                "inviteeUserId",
                "decision",
                "signerDeviceId",
                "issuedAtMs");

            if (!root.GetProperty("version").TryGetInt32(out var version)
                || !TryReadGuid(root, "invitationId", out var invitationId)
                || !TryReadGuid(root, "roomId", out var roomId)
                || !TryReadGuid(root, "inviteeUserId", out var inviteeUserId)
                || root.GetProperty("proposalHash").GetString()
                    is not { Length: > 0 } proposalHashBase64
                || root.GetProperty("decision").GetString()
                    is not { Length: > 0 } decision
                || root.GetProperty("signerDeviceId").GetString()
                    is not { Length: > 0 } signerDeviceId
                || !root.GetProperty("issuedAtMs").TryGetInt64(out var issuedAtMs))
            {
                throw new DomainException("Invitation decision contains malformed metadata.");
            }

            var proposalHash = Convert.FromBase64String(proposalHashBase64);
            if (proposalHash.Length != SHA256.HashSizeInBytes)
            {
                throw new DomainException("Invitation decision proposal hash must be SHA-256.");
            }

            return new ParsedDecision(
                version,
                invitationId,
                proposalHash,
                RoomId.From(roomId),
                UserId.From(inviteeUserId),
                decision,
                signerDeviceId,
                DateTimeOffset.FromUnixTimeMilliseconds(issuedAtMs));
        }
        catch (FormatException)
        {
            throw new DomainException("Invitation decision contains invalid base64.");
        }
        catch (ArgumentOutOfRangeException)
        {
            throw new DomainException("Invitation decision timestamp is out of range.");
        }
        catch (JsonException)
        {
            throw new DomainException("Invitation decision is not valid JSON.");
        }
        catch (InvalidOperationException)
        {
            throw new DomainException("Invitation decision contains malformed metadata.");
        }
        catch (KeyNotFoundException)
        {
            throw new DomainException("Invitation decision is missing required metadata.");
        }
    }

    private static void RequireProofFields(
        byte[] body,
        byte[] signature,
        string signerDeviceId,
        string proofName)
    {
        if (body is not { Length: > 0 }
            || body.Length > RoomInputRules.EncryptedContentMaxLength
            || signature is not { Length: > 0 }
            || string.IsNullOrWhiteSpace(signerDeviceId))
        {
            throw new DomainException($"{proofName} fields are required.");
        }
    }

    private static void RequireExactProperties(
        JsonElement root,
        params string[] expectedNames)
    {
        if (root.ValueKind != JsonValueKind.Object)
        {
            throw new DomainException("Invitation proof must be a JSON object.");
        }

        HashSet<string> names = new(StringComparer.Ordinal);
        foreach (var property in root.EnumerateObject())
        {
            if (!names.Add(property.Name)
                || !expectedNames.Contains(property.Name, StringComparer.Ordinal))
            {
                throw new DomainException(
                    "Invitation proof contains duplicate or unknown fields.");
            }
        }
        if (names.Count != expectedNames.Length)
        {
            throw new DomainException("Invitation proof is missing required metadata.");
        }
    }

    private static bool TryReadGuid(
        JsonElement root,
        string propertyName,
        out Guid value)
    {
        value = Guid.Empty;
        var property = root.GetProperty(propertyName);
        return property.ValueKind == JsonValueKind.String
            && Guid.TryParse(property.GetString(), out value)
            && value != Guid.Empty;
    }

    private sealed record ParsedProposal(
        int Version,
        Guid InvitationId,
        RoomId RoomId,
        UserId OwnerUserId,
        UserId InviteeUserId,
        long BaseRosterGeneration,
        long ProposedRosterGeneration,
        byte[] ProposedRosterBodyHash,
        DateTimeOffset ExpiresAt,
        string SignerDeviceId,
        DateTimeOffset IssuedAt);

    private sealed record ParsedDecision(
        int Version,
        Guid InvitationId,
        byte[] ProposalHash,
        RoomId RoomId,
        UserId InviteeUserId,
        string Decision,
        string SignerDeviceId,
        DateTimeOffset IssuedAt);
}

public sealed record VerifiedRoomInvitationProposal(
    long BaseRosterGeneration,
    long ProposedRosterGeneration,
    byte[] ProposalHash,
    DateTimeOffset IssuedAt,
    DateTimeOffset ExpiresAt);

public sealed record VerifiedRoomInvitationDecision(DateTimeOffset IssuedAt);
