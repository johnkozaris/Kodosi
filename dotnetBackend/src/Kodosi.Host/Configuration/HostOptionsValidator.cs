namespace Kodosi.Host.Configuration;

internal static class HostOptionsValidator
{
    public static void Validate(
        RateLimitingOptions rateLimiting,
        WebSocketSecurityOptions webSockets,
        RequestLimitsOptions requestLimits)
    {
        ArgumentNullException.ThrowIfNull(rateLimiting);
        ArgumentNullException.ThrowIfNull(webSockets);
        ArgumentNullException.ThrowIfNull(requestLimits);

        ValidatePolicy($"{RateLimitingOptions.SectionName}:Api", rateLimiting.Api);
        ValidatePolicy($"{RateLimitingOptions.SectionName}:WebSockets", rateLimiting.WebSockets);
        ValidatePolicy(
            $"{RateLimitingOptions.SectionName}:DestructiveIdentity",
            rateLimiting.DestructiveIdentity);
        ValidatePolicy(
            $"{RateLimitingOptions.SectionName}:DestructiveEnrollment",
            rateLimiting.DestructiveEnrollment);
        ValidatePolicy(
            $"{RateLimitingOptions.SectionName}:DeviceProof",
            rateLimiting.DeviceProof);
        ValidatePolicy(
            $"{RateLimitingOptions.SectionName}:DeviceLinkInit",
            rateLimiting.DeviceLinkInit);
        ValidatePolicy(
            $"{RateLimitingOptions.SectionName}:DeviceLinkPoll",
            rateLimiting.DeviceLinkPoll);

        if (webSockets.KeepAliveIntervalSeconds < 1)
        {
            throw new InvalidOperationException(
                "WebSockets:KeepAliveIntervalSeconds must be at least 1.");
        }

        if (requestLimits.MaxRequestBodySizeBytes < 1)
        {
            throw new InvalidOperationException(
                "RequestLimits:MaxRequestBodySizeBytes must be at least 1.");
        }
    }

    private static void ValidatePolicy(string path, FixedWindowPolicyOptions? policy)
    {
        if (policy is null)
        {
            throw new InvalidOperationException($"{path} must be configured.");
        }

        if (policy.PermitLimit < 1)
        {
            throw new InvalidOperationException($"{path}:PermitLimit must be at least 1.");
        }

        if (policy.WindowSeconds < 1)
        {
            throw new InvalidOperationException($"{path}:WindowSeconds must be at least 1.");
        }

        if (policy.QueueLimit < 0)
        {
            throw new InvalidOperationException($"{path}:QueueLimit must not be negative.");
        }
    }
}
