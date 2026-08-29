using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;
using Microsoft.IdentityModel.Tokens;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;

namespace Kodosi.HostTests;

public sealed class JwtRevalidatorTests
{
    [Theory]
    [InlineData("")]
    [InlineData("not-a-jwt")]
    public async Task RevalidateAsync_Returns_Invalid_For_Unparseable_Tokens(string token)
    {
        var revalidator = CreateRevalidator();
        var result = await revalidator.RevalidateAsync(
            token,
            TestContext.Current.CancellationToken);
        Assert.Equal(JwtRevalidationResult.Invalid, result);
    }

    [Fact]
    public async Task RevalidateAsync_Returns_Transient_On_Transient_Jwks_Failure()
    {
        var validToken = BuildUnsignedJwt(
            issuer: "https://test.invalid",
            expiresUtc: DateTimeOffset.UtcNow.AddMinutes(5));
        var metrics = new OperationalMetrics(new FakeRuntimeDirectory());
        var baselineTransient = metrics.Snapshot().JwtRevalidationTransientCount;
        var revalidator = CreateRevalidator(
            (_, _) => throw new InvalidOperationException(
                "IDX20803: Unable to obtain configuration from JWKS (simulated blip)"),
            metrics);
        var result = await revalidator.RevalidateAsync(
            validToken,
            TestContext.Current.CancellationToken);
        Assert.Equal(JwtRevalidationResult.Transient, result);
        Assert.Equal(baselineTransient + 1, metrics.Snapshot().JwtRevalidationTransientCount);
    }

    [Fact]
    public async Task RevalidateAsync_Returns_Invalid_On_Definitive_SecurityTokenException()
    {
        var validToken = BuildUnsignedJwt(
            issuer: "https://test.invalid",
            expiresUtc: DateTimeOffset.UtcNow.AddMinutes(5));
        var metrics = new OperationalMetrics(new FakeRuntimeDirectory());
        var baseline = metrics.Snapshot();
        var revalidator = CreateRevalidator(
            (_, _) => throw new SecurityTokenSignatureKeyNotFoundException("simulated rotation"),
            metrics);
        var result = await revalidator.RevalidateAsync(
            validToken,
            TestContext.Current.CancellationToken);
        Assert.Equal(JwtRevalidationResult.Invalid, result);
        var after = metrics.Snapshot();
        Assert.Equal(baseline.JwtRevalidationTransientCount, after.JwtRevalidationTransientCount);
        Assert.Equal(baseline.JwtRevalidationInvalidCount + 1, after.JwtRevalidationInvalidCount);
    }

    private static JwtRevalidator CreateRevalidator(
        Func<string, TokenValidationParameters, Task<TokenValidationResult>>? validateToken = null,
        OperationalMetrics? metrics = null)
    {
        var options = new JwtBearerOptions();
        var monitor = new TestOptionsMonitor<JwtBearerOptions>(options);
        return new JwtRevalidator(
            monitor,
            new Dictionary<string, string>(StringComparer.Ordinal),
            fallbackScheme: "Bearer",
            NullLogger<JwtRevalidator>.Instance,
            metrics,
            validateToken);
    }

    private static string BuildUnsignedJwt(string issuer, DateTimeOffset expiresUtc)
    {
        var header = Base64UrlEncode("""{"alg":"none","typ":"JWT"}""");
        var payload = Base64UrlEncode(
            $$"""{"iss":"{{issuer}}","exp":{{expiresUtc.ToUnixTimeSeconds()}}}""");
        return $"{header}.{payload}.";
    }

    private static string Base64UrlEncode(string input)
    {
        var bytes = System.Text.Encoding.UTF8.GetBytes(input);
        return Convert.ToBase64String(bytes)
            .TrimEnd('=')
            .Replace('+', '-')
            .Replace('/', '_');
    }

    private sealed class TestOptionsMonitor<T>(T value) : IOptionsMonitor<T>
    {
        public T CurrentValue { get; } = value;

        public T Get(string? name) => CurrentValue;

        public IDisposable? OnChange(Action<T, string?> listener) => null;
    }
}
