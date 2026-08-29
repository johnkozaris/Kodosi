using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class SemanticReceiptEndpoints
{
    private const int DefaultLimit = 32;
    private const int MaxLimit = 64;

    private sealed record UploadSemanticReceiptRequest(
        Guid SessionId,
        Guid IncarnationId,
        Guid RequestId,
        string Mode,
        string PayloadSha256,
        string Outcome,
        Guid RequesterUserId,
        string RequesterDeviceId,
        Guid OwnerUserId,
        string OwnerDeviceId,
        string Signature);

    private sealed record AcknowledgeSemanticReceiptRequest(
        Guid SessionId,
        Guid IncarnationId,
        Guid RequestId,
        string RequesterUserId,
        string RequesterDeviceId,
        string Signature);

    private sealed record SemanticReceiptResponse(
        Guid SessionId,
        Guid IncarnationId,
        Guid RequestId,
        string Mode,
        string PayloadSha256,
        string Outcome,
        Guid RequesterUserId,
        string RequesterDeviceId,
        Guid OwnerUserId,
        string OwnerDeviceId,
        string Signature);

    private sealed record SemanticReceiptPageResponse(
        IReadOnlyList<SemanticReceiptResponse> Items,
        string? NextCursor);

    public static RouteGroupBuilder MapSemanticReceiptEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/me/semantic-receipts")
            .WithTags("SemanticReceipts");

        group.MapPost("/", async (
            DeviceProofJsonBody<UploadSemanticReceiptRequest> body,
            SemanticReceiptMailboxService mailbox,
            DeviceHttpRequestProofVerifier deviceProof,
            ICurrentUser currentUser,
            HttpContext context,
            CancellationToken ct) =>
        {
            var request = body.Value;
            var authenticatedDevice = await deviceProof.VerifyAsync(
                context.Request,
                currentUser.UserId,
                body.Sha256,
                ct);
            if (authenticatedDevice is null)
            {
                return Results.Forbid();
            }
            var deviceId = authenticatedDevice.DeviceId;
            var outcome = await mailbox.UploadAsync(
                currentUser.UserId,
                deviceId,
                new SemanticReceiptEnvelope(
                    SessionId.From(request.SessionId),
                    request.IncarnationId,
                    request.RequestId,
                    request.Mode,
                    request.PayloadSha256,
                    request.Outcome,
                    UserId.From(request.RequesterUserId),
                    request.RequesterDeviceId,
                    UserId.From(request.OwnerUserId),
                    request.OwnerDeviceId,
                    request.Signature),
                ct);
            return outcome switch
            {
                SemanticReceiptUploadOutcome.Stored => Results.NoContent(),
                SemanticReceiptUploadOutcome.Conflict => Results.Conflict(),
                SemanticReceiptUploadOutcome.UnauthorizedDevice => Results.Forbid(),
                _ => throw new InvalidOperationException(
                    $"Unexpected semantic receipt upload outcome {outcome}.")
            };
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceProof);

        group.MapGet("/", async (
            SemanticReceiptMailboxService mailbox,
            DeviceHttpRequestProofVerifier deviceProof,
            ICurrentUser currentUser,
            HttpContext context,
            int? limit,
            string? cursor,
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
            var pageSize = Math.Clamp(limit ?? DefaultLimit, 1, MaxLimit);
            var page = await mailbox.ListAsync(
                currentUser.UserId,
                deviceId,
                pageSize,
                cursor,
                ct);
            return page is null
                ? Results.Forbid()
                : Results.Ok(new SemanticReceiptPageResponse(
                    page.Items.Select(ToResponse).ToArray(),
                    page.NextCursor));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceProof);

        group.MapPost("/{requestId:guid}/ack", async (
            Guid requestId,
            DeviceProofJsonBody<AcknowledgeSemanticReceiptRequest> body,
            SemanticReceiptMailboxService mailbox,
            DeviceHttpRequestProofVerifier deviceProof,
            ICurrentUser currentUser,
            HttpContext context,
            CancellationToken ct) =>
        {
            var request = body.Value;
            if (requestId != request.RequestId)
            {
                return Results.BadRequest();
            }
            var authenticatedDevice = await deviceProof.VerifyAsync(
                context.Request,
                currentUser.UserId,
                body.Sha256,
                ct);
            if (authenticatedDevice is null)
            {
                return Results.Forbid();
            }
            var deviceId = authenticatedDevice.DeviceId;
            var outcome = await mailbox.AcknowledgeAsync(
                currentUser.UserId,
                deviceId,
                new SemanticReceiptAcknowledgement(
                    SessionId.From(request.SessionId),
                    request.IncarnationId,
                    request.RequestId,
                    request.RequesterUserId,
                    request.RequesterDeviceId,
                    request.Signature),
                ct);
            return outcome switch
            {
                SemanticReceiptAckOutcome.Acknowledged => Results.NoContent(),
                SemanticReceiptAckOutcome.NotFound => Results.NotFound(),
                SemanticReceiptAckOutcome.InvalidSignature
                    or SemanticReceiptAckOutcome.UnauthorizedDevice => Results.Forbid(),
                _ => throw new InvalidOperationException(
                    $"Unexpected semantic receipt acknowledgement outcome {outcome}.")
            };
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceProof);

        return group;
    }

    private static SemanticReceiptResponse ToResponse(SemanticRelayReceipt receipt) =>
        new(
            receipt.SessionId.Value,
            receipt.IncarnationId,
            receipt.RequestId,
            receipt.Mode,
            receipt.PayloadSha256,
            receipt.Outcome,
            receipt.RequesterUserId.Value,
            receipt.RequesterDeviceId,
            receipt.OwnerUserId.Value,
            receipt.OwnerDeviceId,
            receipt.Signature);
}
