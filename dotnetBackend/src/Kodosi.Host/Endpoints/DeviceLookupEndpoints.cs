using System.ComponentModel.DataAnnotations;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class DeviceLookupEndpoints
{
    public static RouteGroupBuilder MapDeviceLookupEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api")
            .WithTags("Devices");

        group.MapGet("/users/{userId:guid}/identity", async (
            HttpContext http,
            Guid userId,
            UserIdentityBundleService bundleService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var bundle = await bundleService.GetAsync(
                currentUser.UserId,
                UserId.From(userId),
                ct);

            if (bundle is null)
            {
                return Results.NotFound();
            }

            var response = new UserIdentityBundleResponse(
                bundle.UserId.Value,
                bundle.IdentityRevision,
                bundle.IdentityIncarnationId,
                new UserDeviceListResponse(
                    Convert.ToBase64String(bundle.DeviceList.Body),
                    Convert.ToBase64String(bundle.DeviceList.Signature)),
                bundle.Devices.Select(ToCertificateResponse).ToList(),
                bundle.HistoricalDevices.Select(ToCertificateResponse).ToList());

            return Results.Ok(response);
        });

        group.MapPut("/me/artifact-endorsements", async (
            PutArtifactEndorsementRequest request,
            ArtifactEndorsementService endorsements,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            await endorsements.PutAsync(
                currentUser.UserId,
                request.IdentityIncarnationId,
                request.ArtifactDigest,
                request.EndorserDeviceId,
                Convert.FromBase64String(request.Signature),
                ct);
            return Results.NoContent();
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<PutArtifactEndorsementRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(16 * 1024));

        group.MapGet("/users/{userId:guid}/artifact-endorsements", async (
            Guid userId,
            string digest,
            ArtifactEndorsementService endorsements,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var rows = await endorsements.GetAsync(currentUser.UserId, UserId.From(userId), digest, ct);
            return rows is null ? Results.NotFound() : Results.Ok(rows.Select(ToEndorsementResponse));
        });

        group.MapGet("/me/artifact-endorsements", async (
            int? offset,
            int? limit,
            ArtifactEndorsementService endorsements,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {
            var page = await endorsements.ListAsync(currentUser.UserId, offset ?? 0, limit ?? 100, ct);
            http.Response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
            if (page.NextOffset is { } nextOffset)
            {
                http.Response.Headers["Kodosi-Next-Offset"] = nextOffset.ToString(System.Globalization.CultureInfo.InvariantCulture);
            }
            return Results.Ok(page.Items.Select(ToEndorsementResponse));
        });

        return group;
    }

    private static ArtifactEndorsementResponse ToEndorsementResponse(ArtifactEndorsement endorsement) => new(
        endorsement.UserId.Value,
        endorsement.IdentityIncarnationId,
        endorsement.ArtifactDigest,
        endorsement.EndorserDeviceId,
        Convert.ToBase64String(endorsement.Signature));

    private static UserDeviceCertificateResponse ToCertificateResponse(UserDevice device) => new(
        Convert.ToBase64String(device.DeviceCertificate!),
        Convert.ToBase64String(device.DeviceCertificateSignature!));
}

public sealed record PutArtifactEndorsementRequest(
    Guid IdentityIncarnationId,
    [property: Required, RegularExpression("^[0-9a-f]{64}$")] string ArtifactDigest,
    [property: Required, StringLength(DeviceIdRules.MaximumLength)] string EndorserDeviceId,
    [property: Required, Base64String, StringLength(4412, MinimumLength = 4412)] string Signature);

public sealed record ArtifactEndorsementResponse(
    Guid UserId,
    Guid IdentityIncarnationId,
    string ArtifactDigest,
    string EndorserDeviceId,
    string Signature);
