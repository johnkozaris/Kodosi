using Microsoft.AspNetCore.Mvc;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Configuration;
using Kodosi.Host.Identity;

namespace Kodosi.Host.Endpoints;

public static class DeviceLinkEndpoints
{
    private sealed record InitRequest(
        string DeviceId,
        string DeviceLabel,
        string KemPublicKey,
        string SigningPublicKey);

    private sealed record InitResponse(
        string DeviceCode,
        string UserCode,
        DateTimeOffset ExpiresAt);

    private sealed record PendingResponse(
        string DeviceId,
        string DeviceLabel,
        string KemPublicKey,
        string SigningPublicKey);

    private sealed record ApproveRequest(
        string UserCode,
        string DeviceCertificate,
        string DeviceCertificateSignature,
        string SignedDeviceList,
        string SignedDeviceListSignature);

    internal sealed record PollRequest(string DeviceCode);
    internal sealed record AcknowledgeRequest(string DeviceCode, string DeviceId);

    internal sealed record PollResponse(
        string State,
        long? DeviceListGeneration,
        UserIdentityBundleResponse? IdentityBundle);

    public static RouteGroupBuilder MapDeviceLinkEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/devices/link")
            .WithTags("DeviceLink");

        group.MapPost("/init", async (
            InitRequest request,
            DeviceLinkRequestService service,
            DeviceLinkRequestPublisher publisher,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var initiated = await service.InitiateAsync(
                currentUser.UserId,
                request.DeviceId,
                request.DeviceLabel,
                DecodeBase64(request.KemPublicKey, "KEM public key"),
                DecodeBase64(request.SigningPublicKey, "signing public key"),
                ct);
            publisher.PublishRequested(
                currentUser.UserId,
                initiated.UserCode,
                initiated.DeviceLabel,
                initiated.ExpiresAt);
            return Results.Ok(new InitResponse(
                initiated.DeviceCode,
                initiated.UserCode,
                initiated.ExpiresAt));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceLinkInit);

        group.MapGet("/pending", async (
            string userCode,
            DeviceLinkRequestService service,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var pending = await service.GetPendingAsync(
                currentUser.UserId,
                userCode,
                ct);
            return pending is null
                ? Results.NotFound()
                : Results.Ok(new PendingResponse(
                    pending.DeviceId,
                    pending.DeviceLabel,
                    Convert.ToBase64String(pending.KemPublicKey),
                    Convert.ToBase64String(pending.SigningPublicKey)));
        });

        group.MapPost("/approve", async (
            ApproveRequest request,
            DeviceLinkApprovalService approvalService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (string.IsNullOrWhiteSpace(request.UserCode))
            {
                throw new DeviceEnrollmentException("userCode is required.");
            }

            var approved = await approvalService.ApproveAsync(
                currentUser.UserId,
                request.UserCode,
                DecodeBase64(request.DeviceCertificate, "device certificate"),
                DecodeBase64(
                    request.DeviceCertificateSignature,
                    "device certificate signature"),
                DecodeBase64(request.SignedDeviceList, "signed device list"),
                DecodeBase64(
                    request.SignedDeviceListSignature,
                    "signed device list signature"),
                ct);
            return approved ? Results.Ok() : Results.NotFound();
        })
        .RequireRateLimiting(RateLimitPolicyNames.DestructiveEnrollment)
        .WithMetadata(new RequestSizeLimitAttribute(IdentityRequestLimits.MutationBodyBytes));

        group.MapPost("/poll", async (
            PollRequest request,
            DeviceLinkPollService pollService,
            IDeviceLinkPollThrottle pollThrottle,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (!string.IsNullOrWhiteSpace(request.DeviceCode)
                && pollThrottle.TryClaim(request.DeviceCode) is { } retryAfter)
            {
                throw new DeviceLinkPollThrottledException(retryAfter);
            }
            return MapPollResult(await pollService.PollAsync(
                currentUser.UserId,
                request.DeviceCode,
                ct));
        });

        group.MapPost("/ack", async (
            AcknowledgeRequest request,
            DeviceLinkRequestService service,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var result = await service.AcknowledgeAsync(
                currentUser.UserId,
                request.DeviceCode,
                request.DeviceId,
                ct);
            return result == DeviceLinkAcknowledgeResult.Acknowledged
                ? Results.NoContent()
                : Results.NotFound();
        });

        group.MapDelete("/requests/{userCode}", async (
            string userCode,
            DeviceLinkRequestService service,
            DeviceLinkRequestPublisher publisher,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var cancelled = await service.CancelAsync(
                currentUser.UserId,
                userCode,
                ct);
            if (cancelled is null)
            {
                return Results.NotFound();
            }
            publisher.PublishCancelled(currentUser.UserId, cancelled.UserCode);
            return Results.NoContent();
        });

        return group;
    }

    private static IResult MapPollResult(DeviceLinkPollResult result) => result.State switch
    {
        DeviceLinkPollState.NotFound => Results.NotFound(),
        _ => Results.Ok(new PollResponse(
            result.State.ToString().ToLowerInvariant(),
            result.DeviceListGeneration,
            result.IdentityBundle)),
    };

    private static byte[] DecodeBase64(string? value, string fieldName)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new DeviceEnrollmentException($"{fieldName} is required.");
        }

        try
        {
            return Convert.FromBase64String(value);
        }
        catch (FormatException)
        {
            throw new DeviceEnrollmentException($"{fieldName} must be valid base64.");
        }
    }
}
