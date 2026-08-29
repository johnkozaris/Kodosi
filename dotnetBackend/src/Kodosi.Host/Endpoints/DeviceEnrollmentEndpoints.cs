using Microsoft.AspNetCore.Mvc;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Configuration;
using Kodosi.Host.Identity;

namespace Kodosi.Host.Endpoints;

public static class DeviceEnrollmentEndpoints
{
    private sealed record RegisterDeviceRequest(
        string DeviceId,
        string KemPublicKey,
        string SigningPublicKey,
        Guid ChallengeId,
        string PopSignature,
        string DeviceCertificate,
        string DeviceCertificateSignature,
        string SignedDeviceList,
        string SignedDeviceListSignature);

    public static RouteGroupBuilder MapDeviceEnrollmentEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api")
            .WithTags("Devices");

        group.MapPost("/me/devices/challenge", async (
            DeviceRegistrationChallengeService service,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var challenge = await service.CreateAsync(currentUser.UserId, ct);
            return Results.Created(
                $"/api/me/devices/challenge/{challenge.Id}",
                new DeviceRegistrationChallengeResponse(
                    challenge.Id,
                    Convert.ToBase64String(challenge.Challenge),
                    challenge.ExpiresAt));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DestructiveEnrollment);

        group.MapPost("/me/device-proofs/challenge", async (
            DeviceRegistrationChallengeService service,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var challenge = await service.CreateAsync(currentUser.UserId, ct);
            return Results.Created(
                $"/api/me/device-proofs/challenge/{challenge.Id}",
                new DeviceRegistrationChallengeResponse(
                    challenge.Id,
                    Convert.ToBase64String(challenge.Challenge),
                    challenge.ExpiresAt));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DeviceProof);

        group.MapPost("/me/devices", async (
            RegisterDeviceRequest request,
            DeviceEnrollmentService service,
            DeviceEnrollmentPublisher publisher,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (string.IsNullOrWhiteSpace(request.DeviceId))
            {
                throw new DeviceEnrollmentException("DeviceId is required.");
            }
            if (string.IsNullOrWhiteSpace(request.PopSignature))
            {
                throw new DeviceEnrollmentException("PopSignature is required.");
            }

            DeviceEnrollmentCommand command;
            try
            {
                command = new DeviceEnrollmentCommand(
                    currentUser.UserId,
                    request.DeviceId,
                    DecodeRequiredBase64(request.KemPublicKey),
                    DecodeRequiredBase64(request.SigningPublicKey),
                    request.ChallengeId,
                    DecodeRequiredBase64(request.PopSignature),
                    DecodeRequiredBase64(request.DeviceCertificate),
                    DecodeRequiredBase64(request.DeviceCertificateSignature),
                    DecodeRequiredBase64(request.SignedDeviceList),
                    DecodeRequiredBase64(request.SignedDeviceListSignature));
            }
            catch (FormatException)
            {
                throw new DeviceEnrollmentException(
                    "Public keys, PoP signature, certificate, and device list fields must be valid base64.");
            }

            var result = await service.EnrollAsync(command, ct);
            await publisher.PublishAsync(currentUser.UserId, result);
            return Results.Created($"/api/me/devices/{result.DeviceId}", null);
        })
        .RequireRateLimiting(RateLimitPolicyNames.DestructiveEnrollment)
        .WithMetadata(new RequestSizeLimitAttribute(IdentityRequestLimits.MutationBodyBytes));

        return group;
    }

    private static byte[] DecodeRequiredBase64(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new FormatException("Base64 field is required.");
        }
        return Convert.FromBase64String(value);
    }
}
