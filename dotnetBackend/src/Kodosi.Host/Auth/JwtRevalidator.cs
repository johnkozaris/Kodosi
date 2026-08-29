using System.Diagnostics;
using System.IdentityModel.Tokens.Jwt;
using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.Extensions.Options;
using Microsoft.IdentityModel.Tokens;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Auth;

public interface IJwtRevalidator
{
    Task<JwtRevalidationResult> RevalidateAsync(string token, CancellationToken ct);
}

public enum JwtRevalidationResult
{
    Valid,
    Invalid,
    Transient,
}

internal sealed class JwtRevalidator : IJwtRevalidator
{
    private readonly IOptionsMonitor<JwtBearerOptions> _optionsMonitor;
    private readonly IReadOnlyDictionary<string, string> _schemesByIssuer;
    private readonly string _fallbackScheme;
    private readonly ILogger<JwtRevalidator> _logger;
    private readonly OperationalMetrics? _metrics;
    private readonly JwtSecurityTokenHandler _handler = new();
    private readonly Func<string, TokenValidationParameters, Task<TokenValidationResult>> _validateToken;

    public JwtRevalidator(
        IOptionsMonitor<JwtBearerOptions> optionsMonitor,
        IReadOnlyDictionary<string, string> schemesByIssuer,
        string fallbackScheme,
        ILogger<JwtRevalidator> logger,
        OperationalMetrics metrics)
        : this(optionsMonitor, schemesByIssuer, fallbackScheme, logger, metrics, validateToken: null)
    {
    }

    internal JwtRevalidator(
        IOptionsMonitor<JwtBearerOptions> optionsMonitor,
        IReadOnlyDictionary<string, string> schemesByIssuer,
        string fallbackScheme,
        ILogger<JwtRevalidator> logger,
        OperationalMetrics? metrics,
        Func<string, TokenValidationParameters, Task<TokenValidationResult>>? validateToken)
    {
        _optionsMonitor = optionsMonitor;
        _schemesByIssuer = schemesByIssuer;
        _fallbackScheme = fallbackScheme;
        _logger = logger;
        _metrics = metrics;
        _validateToken = validateToken ?? ((t, p) => _handler.ValidateTokenAsync(t, p));
    }

    public async Task<JwtRevalidationResult> RevalidateAsync(string token, CancellationToken ct)
    {
        if (string.IsNullOrWhiteSpace(token) || !_handler.CanReadToken(token))
        {
            return JwtRevalidationResult.Invalid;
        }

        string schemeName;
        try
        {
            var jwt = _handler.ReadJwtToken(token);
            var issuer = jwt.Issuer?.Trim();
            schemeName = !string.IsNullOrWhiteSpace(issuer)
                && _schemesByIssuer.TryGetValue(issuer, out var resolvedScheme)
                    ? resolvedScheme
                    : _fallbackScheme;
        }
        catch (Exception ex) when (ex is ArgumentException or SecurityTokenException)
        {

            return JwtRevalidationResult.Invalid;
        }

        var options = _optionsMonitor.Get(schemeName);

        var parameters = options.TokenValidationParameters.Clone();
        parameters.ValidateLifetime = true;



        var fetchStopwatch = Stopwatch.StartNew();
        try
        {
            if (options.ConfigurationManager is not null
                && (parameters.IssuerSigningKeys is null
                    || !parameters.IssuerSigningKeys.Any()))
            {
                var config = await options.ConfigurationManager.GetConfigurationAsync(ct);
                parameters.IssuerSigningKeys = config.SigningKeys;
            }

            var validationResult = await _validateToken(token, parameters);
            fetchStopwatch.Stop();
            _metrics?.RecordJwtRevalidationDuration(fetchStopwatch.Elapsed.TotalMilliseconds);
            if (validationResult.IsValid)
            {
                return JwtRevalidationResult.Valid;
            }

            _metrics?.RecordJwtRevalidationInvalid();
            _logger.LogInformation(
                validationResult.Exception,
                "JWT revalidation failed for scheme {Scheme}: {Error}",
                schemeName,
                validationResult.Exception?.GetType().Name);
            return JwtRevalidationResult.Invalid;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (SecurityTokenException ex)
        {
            fetchStopwatch.Stop();
            _metrics?.RecordJwtRevalidationDuration(fetchStopwatch.Elapsed.TotalMilliseconds);
            _metrics?.RecordJwtRevalidationInvalid();
            _logger.LogInformation(
                ex,
                "JWT revalidation threw definitive invalidation for scheme {Scheme}: {Error}",
                schemeName,
                ex.GetType().Name);
            return JwtRevalidationResult.Invalid;
        }
        catch (Exception ex)
        {
            fetchStopwatch.Stop();
            _metrics?.RecordJwtRevalidationDuration(fetchStopwatch.Elapsed.TotalMilliseconds);
            _metrics?.RecordJwtRevalidationTransient();
            _logger.LogWarning(
                ex,
                "JWT revalidation threw for scheme {Scheme}; transient",
                schemeName);
            return JwtRevalidationResult.Transient;
        }
    }
}
