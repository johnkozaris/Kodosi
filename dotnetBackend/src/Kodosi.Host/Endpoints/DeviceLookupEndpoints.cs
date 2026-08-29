using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;

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

        return group;
    }

    private static UserDeviceCertificateResponse ToCertificateResponse(UserDevice device) => new(
        Convert.ToBase64String(device.DeviceCertificate!),
        Convert.ToBase64String(device.DeviceCertificateSignature!));
}
