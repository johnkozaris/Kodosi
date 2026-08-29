namespace Kodosi.Host.Configuration;

public sealed class TelemetryOptions
{
    public const string SectionName = "Telemetry";

    public string ServiceName { get; init; } = "Kodosi.Backend";
    public string ServiceVersion { get; init; } = string.Empty;
    public string OtlpEndpoint { get; init; } = string.Empty;
    public string Protocol { get; init; } = "grpc";
    public bool UseGzipCompression { get; init; }
    public Dictionary<string, string> ResourceAttributes { get; init; } = new(StringComparer.OrdinalIgnoreCase);
}

public sealed class OidcAuthOptions
{
    public const string SectionName = "Auth";

    public ExternalAuthProviderOptions[] Providers { get; init; } = [];
    public int LocalProfileSyncTtlSeconds { get; init; } = 60;
}

public sealed class ExternalAuthProviderOptions
{
    public bool Enabled { get; init; } = true;
    public string Scheme { get; init; } = string.Empty;
    public string Provider { get; init; } = string.Empty;
    public string Authority { get; init; } = string.Empty;
    public string CanonicalIssuer { get; init; } = string.Empty;
    public string[] ValidIssuers { get; init; } = [];
    public string[] Audiences { get; init; } = [];
    public string[] AllowedAlgorithms { get; init; } = ["RS256", "ES256"];
    public string SubjectClaim { get; init; } = "sub";
    public string UserNameClaim { get; init; } = "preferred_username";
    public string EmailClaim { get; init; } = "email";
    public string DisplayNameClaim { get; init; } = "name";
    public string AvatarUrlClaim { get; init; } = "picture";
    public LinkedIdentityClaimOptions[] LinkedIdentityClaims { get; init; } = [];
    public bool RequireHttpsMetadata { get; init; } = true;
}

public sealed class LinkedIdentityClaimOptions
{
    public string Provider { get; init; } = string.Empty;
    public string Issuer { get; init; } = string.Empty;
    public string SubjectClaim { get; init; } = string.Empty;
}

public sealed class WebSocketSecurityOptions
{
    public const string SectionName = "WebSockets";

    public string[] AllowedOrigins { get; init; } = [];
    public int KeepAliveIntervalSeconds { get; init; } = 30;
}

public sealed class FixedWindowPolicyOptions
{
    public int PermitLimit { get; init; } = 120;
    public int WindowSeconds { get; init; } = 60;
    public int QueueLimit { get; init; }
}

public sealed class RateLimitingOptions
{
    public const string SectionName = "RateLimiting";

    public FixedWindowPolicyOptions Api { get; init; } = new();
    public FixedWindowPolicyOptions WebSockets { get; init; } = new();


    public FixedWindowPolicyOptions DestructiveIdentity { get; init; } = new()
    {
        PermitLimit = 5,
        WindowSeconds = 3600,
    };



    public FixedWindowPolicyOptions DestructiveEnrollment { get; init; } = new()
    {
        PermitLimit = 10,
        WindowSeconds = 3600,
    };





    public FixedWindowPolicyOptions DeviceProof { get; init; } = new()
    {
        PermitLimit = 1_200,
        WindowSeconds = 60,
    };



    public FixedWindowPolicyOptions DeviceLinkInit { get; init; } = new()
    {
        PermitLimit = 3,
        WindowSeconds = 3600,
    };



    public FixedWindowPolicyOptions DeviceLinkPoll { get; init; } = new()
    {
        PermitLimit = 20,
        WindowSeconds = 60,
    };
}

public sealed class ProxyOptions
{
    public const string SectionName = "Proxy";

    public string[] KnownProxies { get; init; } = [];
    public int ForwardLimit { get; init; } = 1;
}

public sealed class RequestLimitsOptions
{
    public const string SectionName = "RequestLimits";

    public long MaxRequestBodySizeBytes { get; init; } = 64 * 1024;
}

public static class RateLimitPolicyNames
{
    public const string Api = "api";
    public const string WebSocket = "websocket";
    public const string DestructiveIdentity = "destructive-identity";
    public const string DestructiveEnrollment = "destructive-enrollment";
    public const string DeviceProof = "device-proof";
    public const string DeviceLinkInit = "device-link-init";
}
