using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Middleware;

public sealed class ExceptionMappingMiddleware(
    RequestDelegate next,
    OperationalMetrics metrics,
    ILogger<ExceptionMappingMiddleware> logger)
{
    private readonly RequestDelegate _next = next;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<ExceptionMappingMiddleware> _logger = logger;

    public async Task InvokeAsync(HttpContext context)
    {
        try
        {
            await _next(context);
        }
        catch (DomainException ex) when (!context.Response.HasStarted)
        {
            _logger.LogWarning(ex, "Domain error: {Code}", ex.Code);

            var statusCode = ex switch
            {
                DeviceListCorruptionException or DeviceCertificateCorruptionException =>
                    StatusCodes.Status500InternalServerError,
                NotFoundException => StatusCodes.Status404NotFound,
                PolicyViolationException => StatusCodes.Status403Forbidden,
                InvalidStateException => StatusCodes.Status409Conflict,
                ConflictException => StatusCodes.Status409Conflict,
                FriendRequestThrottledException => StatusCodes.Status429TooManyRequests,
                DeviceLinkPollThrottledException => StatusCodes.Status429TooManyRequests,
                HandleAllocationExhaustedException => StatusCodes.Status503ServiceUnavailable,
                _ => StatusCodes.Status400BadRequest,
            };

            var retryAfter = ex switch
            {
                FriendRequestThrottledException throttled => throttled.RetryAfter,
                DeviceLinkPollThrottledException throttled => throttled.RetryAfter,
                _ => (TimeSpan?)null,
            };
            if (retryAfter is { } retry)
            {
                if (ex is FriendRequestThrottledException)
                {
                    _metrics.RecordFriendRequestThrottleRejection();
                }


                var seconds = Math.Max(1, (int)Math.Ceiling(retry.TotalSeconds));
                context.Response.Headers.RetryAfter = seconds.ToString(System.Globalization.CultureInfo.InvariantCulture);
            }


            var problem = new ProblemDetails
            {
                Type = $"https://Kodosi.dev/errors/{ex.Code.ToLowerInvariant()}",
                Title = ReasonPhraseFor(statusCode),
                Status = statusCode,
                Detail = ex.Message,
            };
            problem.Extensions["code"] = ex.Code;

            await WriteProblemAsync(context, problem);
        }
        catch (DomainException ex)
        {
            _logger.LogWarning(ex, "Domain error after response started: {Code}", ex.Code);
            throw;
        }
        catch (DbUpdateConcurrencyException ex) when (!context.Response.HasStarted)
        {


            _logger.LogWarning(ex, "Optimistic concurrency collision outside UnitOfWork");

            var problem = new ProblemDetails
            {
                Type = "https://Kodosi.dev/errors/concurrent_modification",
                Title = "Conflict",
                Status = StatusCodes.Status409Conflict,
                Detail = "Another request modified the same record concurrently. Refresh and retry.",
            };
            problem.Extensions["code"] = "CONCURRENT_MODIFICATION";

            await WriteProblemAsync(context, problem);
        }
        catch (DbUpdateConcurrencyException ex)
        {
            _logger.LogWarning(ex, "Concurrency collision after response started");
            throw;
        }
        catch (BadHttpRequestException ex) when (!context.Response.HasStarted)
        {
            _logger.LogWarning(ex, "Rejected malformed HTTP request");

            var problem = new ProblemDetails
            {
                Type = "https://Kodosi.dev/errors/bad_http_request",
                Title = ReasonPhraseFor(ex.StatusCode),
                Status = ex.StatusCode,
                Detail = ex.Message,
            };
            problem.Extensions["code"] = "BAD_HTTP_REQUEST";

            await WriteProblemAsync(context, problem);
        }
        catch (BadHttpRequestException ex)
        {
            _logger.LogWarning(ex, "Malformed HTTP request after response started");
            throw;
        }
        catch (OperationCanceledException) when (context.RequestAborted.IsCancellationRequested)
        {



            throw;
        }
        catch (Exception ex) when (!context.Response.HasStarted)
        {
            _logger.LogError(ex, "Unhandled exception");

            var problem = new ProblemDetails
            {
                Type = "https://Kodosi.dev/errors/internal",
                Title = "Internal Server Error",
                Status = StatusCodes.Status500InternalServerError,
                Detail = "An unexpected error occurred.",
            };

            await WriteProblemAsync(context, problem);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Unhandled exception after response started");
            throw;
        }
    }

    private static Task WriteProblemAsync(HttpContext context, ProblemDetails problem)
    {
        context.Response.StatusCode = problem.Status ?? StatusCodes.Status500InternalServerError;
        return context.Response.WriteAsJsonAsync(
            problem,
            options: (System.Text.Json.JsonSerializerOptions?)null,
            contentType: "application/problem+json",
            cancellationToken: context.RequestAborted);
    }

    private static string ReasonPhraseFor(int statusCode) => statusCode switch
    {
        StatusCodes.Status400BadRequest => "Bad Request",
        StatusCodes.Status401Unauthorized => "Unauthorized",
        StatusCodes.Status403Forbidden => "Forbidden",
        StatusCodes.Status404NotFound => "Not Found",
        StatusCodes.Status409Conflict => "Conflict",
        StatusCodes.Status413PayloadTooLarge => "Content Too Large",
        StatusCodes.Status415UnsupportedMediaType => "Unsupported Media Type",
        StatusCodes.Status429TooManyRequests => "Too Many Requests",
        StatusCodes.Status500InternalServerError => "Internal Server Error",
        StatusCodes.Status503ServiceUnavailable => "Service Unavailable",
        _ => "Request Failed",
    };
}
