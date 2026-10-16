using System.Text.Json;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Kodosi;

internal sealed record ErrorBody(string Error);

internal sealed partial class ApiErrors(RequestDelegate next, ServerMetrics metrics, ILogger<ApiErrors> logger)
{
    public async Task InvokeAsync(HttpContext context)
    {
        try
        {
            await next(context);
        }
        catch (OperationCanceledException) when (context.RequestAborted.IsCancellationRequested)
        {
        }
        catch (Exception error) when (!context.Response.HasStarted)
        {
            var (status, message) = Answer(error);
            context.Response.Clear();
            context.Response.StatusCode = status;
            await context.Response.WriteAsJsonAsync(new ErrorBody(message), context.RequestAborted);
        }
    }

    private (int Status, string Message) Answer(Exception error)
    {
        switch (error)
        {
            case ApiException refusal:
                return (refusal.Status, refusal.Message);
            case BadHttpRequestException bad:
                return (bad.StatusCode, "The request is not valid.");
            case IdentityFormatException or JsonException:
                return (StatusCodes.Status400BadRequest, "Invalid request encoding.");
        }
        if (Failures.IsConflict(error))
        {
            Conflicted(logger, Failures.Describe(error));
            return (StatusCodes.Status409Conflict, error is DbUpdateConcurrencyException
                ? "The current state changed; refresh before retrying."
                : "The requested change conflicts with current state.");
        }
        metrics.Failed();
        if (Failures.IsDatabaseUnavailable(error))
        {
            DatabaseUnavailable(logger, Failures.Describe(error));
            return (StatusCodes.Status503ServiceUnavailable, "The Kodosi server could not reach its database. Try again in a moment.");
        }
        Unexpected(logger, error);
        return (StatusCodes.Status500InternalServerError, "The Kodosi server could not complete the request.");
    }

    [LoggerMessage(Level = LogLevel.Information, Message = "A request conflicted with the current state: {Failure}")]
    private static partial void Conflicted(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Warning, Message = "A request could not reach the database: {Failure}")]
    private static partial void DatabaseUnavailable(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Error, Message = "A request failed")]
    private static partial void Unexpected(ILogger logger, Exception error);
}

internal static class Failures
{
    public static bool IsDatabaseUnavailable(Exception error) => error switch
    {
        NpgsqlException npgsql => npgsql.IsTransient,
        TimeoutException => true,
        DbUpdateException or InvalidOperationException => error.InnerException is { } inner && IsDatabaseUnavailable(inner),
        _ => false,
    };

    public static bool IsConflict(Exception error) => error is DbUpdateConcurrencyException
        or DbUpdateException
        {
            InnerException: PostgresException
            {
                SqlState: PostgresErrorCodes.UniqueViolation or PostgresErrorCodes.ForeignKeyViolation
                    or PostgresErrorCodes.SerializationFailure or PostgresErrorCodes.DeadlockDetected
            }
        };

    public static string Describe(Exception error) => (error as PostgresException ?? error.InnerException as PostgresException) is { } postgres
        ? $"{postgres.SqlState} {postgres.ConstraintName ?? postgres.TableName}".TrimEnd()
        : error.GetType().Name;
}
