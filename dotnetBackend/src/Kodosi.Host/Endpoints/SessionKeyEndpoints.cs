using System.Text.Json.Serialization;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Endpoints;

public static class SessionKeyEndpoints
{
    private sealed record StoreKeyBlobsRequest(
        Guid IncarnationId,
        IReadOnlyList<KeyBlobEntry> Blobs);

    private sealed record ClaimNextGenerationRequest(
        Guid IncarnationId,
        int ExpectedCurrentGeneration);

    internal sealed record KeyBlobEntry(
        string RecipientDeviceId,
        string EncryptedSessionKey,
        string SenderDeviceId,
        int KeyGeneration,
        long IssuedAtMs,
        string? Signature,
        [property: JsonRequired] int SignatureVersion);

    public static RouteGroupBuilder MapSessionKeyEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/sessions/{id:guid}/keys")
            .WithTags("SessionKeys");

        group.MapPost("/", async (
            Guid id,
            StoreKeyBlobsRequest request,
            SessionKeyDistributionService sessionKeyDistributionService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (request.IncarnationId == default)
            {
                throw new InvalidSessionKeyPayloadException(
                    "IncarnationId is required for session key publication.");
            }
            if (request.Blobs is null || request.Blobs.Count == 0)
            {
                throw new InvalidSessionKeyPayloadException("At least one key blob is required.");
            }

            var result = await sessionKeyDistributionService.ReplaceKeyBlobsAsync(
                SessionId.From(id),
                currentUser.UserId,
                request.IncarnationId,
                request.Blobs.Select(entry => new SessionKeyBlobSubmission(
                    entry.RecipientDeviceId,
                    entry.EncryptedSessionKey,
                    entry.SenderDeviceId,
                    entry.KeyGeneration,
                    entry.IssuedAtMs,
                    entry.Signature,
                    entry.SignatureVersion)).ToArray(),
                ct);

            return result.State switch
            {
                StoreSessionKeyBlobsState.Stored => Results.NoContent(),
                StoreSessionKeyBlobsState.NotFound => Results.NotFound(),
                StoreSessionKeyBlobsState.StaleIncarnation =>
                    throw new InvalidStateException(
                        "Session incarnation changed; key publication was rejected."),
                StoreSessionKeyBlobsState.InvalidRequest =>
                    throw new InvalidSessionKeyPayloadException(
                        result.ErrorMessage ?? "Invalid session key payload."),
                _ => throw new InvalidOperationException(
                    $"Unexpected store-session-key result: {result.State}")
            };
        });

        group.MapGet("/mine", async (
            Guid id,
            SessionKeyQueryService sessionKeyQueryService,
            DeviceHttpRequestProofVerifier deviceProof,
            SessionBroadcaster broadcaster,
            ICurrentUser currentUser,
            ILoggerFactory loggerFactory,
            HttpContext context,
            CancellationToken ct) =>
        {
            var authenticatedDevice = await deviceProof.VerifyAsync(
                context.Request,
                currentUser.UserId,
                DeviceHttpRequestProofVerifier.EmptyBodySha256,
                ct);
            if (authenticatedDevice is null)
            {
                return Results.Forbid();
            }
            var deviceId = authenticatedDevice.DeviceId;

            var sessionId = SessionId.From(id);
            SessionKeyFetchResult result;
            try
            {
                result = await sessionKeyQueryService.GetMySessionKeyAsync(
                    sessionId,
                    currentUser.UserId,
                    deviceId,
                    ct);
            }



            catch (NotFoundException ex)
            {
                loggerFactory.CreateLogger("SessionKeyEndpoints").LogDebug(
                    "GET /sessions/{SessionId}/keys/mine → 404 (NotFound: {Reason})",
                    sessionId, ex.Message);
                return Results.NotFound();
            }
            catch (PolicyViolationException ex)
            {
                loggerFactory.CreateLogger("SessionKeyEndpoints").LogDebug(
                    "GET /sessions/{SessionId}/keys/mine → 404 (PolicyViolation: {Reason})",
                    sessionId, ex.Message);
                return Results.NotFound();
            }

            if (result.State == SessionKeyFetchState.PendingDistribution)
            {
                var sessionCanRequestDistribution =
                    await sessionKeyQueryService.ShouldRequestHostKeyDistributionAsync(
                        sessionId,
                        ct);
                if (sessionCanRequestDistribution)
                {
                    broadcaster.NotifyHostKeyDistributionRequested(sessionId);
                }
            }

            return Results.Ok(ToSessionKeyFetchResponse(result));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceProof);




        group.MapPost("/claim-next-generation", async (
            Guid id,
            ClaimNextGenerationRequest request,
            SessionKeyGenerationClaimer claimer,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (request.IncarnationId == default)
            {
                throw new InvalidSessionKeyPayloadException(
                    "IncarnationId is required for a session key generation claim.");
            }
            if (request.ExpectedCurrentGeneration < 0)
            {
                throw new InvalidSessionKeyPayloadException(
                    "ExpectedCurrentGeneration must be non-negative.");
            }
            return await ClaimNextGenerationAsync(
                id,
                claimer,
                currentUser.UserId,
                request.IncarnationId,
                request.ExpectedCurrentGeneration,
                ct);
        });

        group.MapGet("/current-generation", async (
            Guid id,
            Guid incarnationId,
            SessionKeyGenerationClaimer claimer,
            ICurrentUser currentUser,
            CancellationToken ct) =>
            await ReadCurrentGenerationAsync(
                id,
                claimer,
                currentUser.UserId,
                incarnationId,
                ct));

        group.MapGet("/authorized-devices", async (
            Guid id,
            AuthorizedSessionDeviceQueryService query,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var devices = await query.ListAsync(
                SessionId.From(id),
                currentUser.UserId,
                ct);
            return devices is null
                ? Results.NotFound()
                : Results.Ok(devices.Select(device => new AuthorizedDeviceResponse(
                    device.UserId.Value,
                    device.DeviceId)));
        });

        return group;
    }

    internal static async Task<IResult> ClaimNextGenerationAsync(
        Guid id,
        SessionKeyGenerationClaimer claimer,
        UserId requestorId,
        Guid expectedIncarnationId,
        int expectedCurrentGeneration,
        CancellationToken ct)
    {
        var result = await claimer.ClaimNextAsync(
            SessionId.From(id),
            requestorId,
            expectedIncarnationId,
            expectedCurrentGeneration,
            ct);
        return result.State switch
        {
            SessionKeyGenerationClaimState.Claimed when result.Generation is { } generation =>
                Results.Ok(new SessionKeyGenerationClaimResponse(
                    SessionKeyGenerationClaimResponseState.Claimed,
                    generation)),
            SessionKeyGenerationClaimState.NotFound => Results.NotFound(),
            SessionKeyGenerationClaimState.StaleIncarnation => throw new InvalidStateException(
                "Session incarnation changed; key generation claim was rejected."),
            SessionKeyGenerationClaimState.GenerationChanged
                when result.Generation is { } currentGeneration =>
                Results.Ok(new SessionKeyGenerationClaimResponse(
                    SessionKeyGenerationClaimResponseState.GenerationChanged,
                    currentGeneration)),
            SessionKeyGenerationClaimState.Ended => throw new InvalidStateException(
                "Session is ended; no further key rotations accepted."),
            SessionKeyGenerationClaimState.Exhausted => throw new InvalidStateException(
                "Session key generation is exhausted."),
            _ => throw new InvalidOperationException(
                $"Unexpected session-key generation claim result: {result.State}"),
        };
    }

    internal static async Task<IResult> ReadCurrentGenerationAsync(
        Guid id,
        SessionKeyGenerationClaimer claimer,
        UserId requestorId,
        Guid expectedIncarnationId,
        CancellationToken ct)
    {
        if (expectedIncarnationId == default)
        {
            throw new InvalidSessionKeyPayloadException(
                "IncarnationId is required to query the session key generation.");
        }

        var result = await claimer.ReadCurrentAsync(
            SessionId.From(id),
            requestorId,
            expectedIncarnationId,
            ct);
        return result.State switch
        {
            SessionKeyGenerationReadState.Found when result.Generation is { } generation =>
                Results.Ok(new CurrentKeyGenerationResponse(generation)),
            SessionKeyGenerationReadState.NotFound => Results.NotFound(),
            SessionKeyGenerationReadState.StaleIncarnation => throw new InvalidStateException(
                "Session incarnation changed; key generation query was rejected."),
            SessionKeyGenerationReadState.Ended => throw new InvalidStateException(
                "Session is ended; no further key rotations accepted."),
            _ => throw new InvalidOperationException(
                $"Unexpected session-key generation read result: {result.State}"),
        };
    }

    private static SessionKeyFetchResponse ToSessionKeyFetchResponse(SessionKeyFetchResult result)
    {
        return result.Blob is null
            ? new SessionKeyFetchResponse(result.State)
            : new SessionKeyFetchResponse(
                result.State,
                new SessionKeyBlobResponse(
                    result.IncarnationId
                        ?? throw new InvalidOperationException(
                            "Ready session key response is missing its incarnation."),
                    result.IncarnationProtocolVersion
                        ?? throw new InvalidOperationException(
                            "Ready session key response is missing its incarnation protocol."),
                    Convert.ToBase64String(result.Blob.EncryptedSessionKey),
                    result.Blob.SenderDeviceId,
                    Convert.ToBase64String(result.Blob.SenderKemPublicKey),
                    result.Blob.Signature.Length > 0 ? Convert.ToBase64String(result.Blob.Signature) : null,
                    result.Blob.SignatureVersion,
                    result.Blob.KeyGeneration,
                    result.Blob.IssuedAtMs));
    }
}
