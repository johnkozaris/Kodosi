using Microsoft.AspNetCore.Mvc;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

public static class DeviceListEndpoints
{
    private sealed record SubmitDeviceListRequest(
        string SignedDeviceList,
        string SignedDeviceListSignature);

    public static RouteGroupBuilder MapDeviceListEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api")
            .WithTags("Devices");

        group.MapPost("/me/identity/device-list", async (
            SubmitDeviceListRequest request,
            DeviceListReplacementService service,
            HttpContext httpContext,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            byte[] listBytes;
            byte[] signatureBytes;
            try
            {
                listBytes = DecodeRequiredBase64(request.SignedDeviceList);
                signatureBytes = DecodeRequiredBase64(request.SignedDeviceListSignature);
            }
            catch (FormatException)
            {
                throw new DeviceEnrollmentException(
                    "Signed device list and signature must be valid base64.");
            }

            var (clientIp, userAgent) = httpContext.ExtractAuditContext();
            await service.ReplaceAsync(
                currentUser.UserId,
                listBytes,
                signatureBytes,
                new RequestAuditContext(clientIp, userAgent),
                ct);

            return Results.NoContent();
        })
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
