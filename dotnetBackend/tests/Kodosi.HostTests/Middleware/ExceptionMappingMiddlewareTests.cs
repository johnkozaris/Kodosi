using System.Text.Json;
using Kodosi.Application;
using Kodosi.Host.Middleware;
using Kodosi.Host.Observability;
using Kodosi.Infrastructure.Realtime;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class ExceptionMappingMiddlewareTests
{
    [Fact]
    public async Task RequestAbortCancellation_Propagates_Without_Writing_Problem()
    {
        using var requestAborted = new CancellationTokenSource();
        requestAborted.Cancel();
        var context = new DefaultHttpContext
        {
            RequestAborted = requestAborted.Token,
        };
        context.Response.Body = new MemoryStream();
        var runtimes = new LiveSessionStateDirectory();
        var middleware = new ExceptionMappingMiddleware(
            _ => throw new OperationCanceledException(requestAborted.Token),
            new OperationalMetrics(runtimes),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => middleware.InvokeAsync(context));

        Assert.Equal(0, context.Response.Body.Length);
        Assert.Null(context.Response.ContentType);
    }

    [Fact]
    public async Task DeviceListCorruption_Maps_To_Dedicated_Internal_Diagnostic()
    {
        var context = new DefaultHttpContext();
        context.Response.Body = new MemoryStream();
        var runtimes = new LiveSessionStateDirectory();
        var middleware = new ExceptionMappingMiddleware(
            _ => throw new Kodosi.Domain.DeviceListCorruptionException(
                "Persisted device list is corrupt: root must be an array."),
            new OperationalMetrics(runtimes),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await middleware.InvokeAsync(context);

        Assert.Equal(StatusCodes.Status500InternalServerError, context.Response.StatusCode);
        context.Response.Body.Position = 0;
        using var problem = await JsonDocument.ParseAsync(
            context.Response.Body,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(
            "Internal Server Error",
            problem.RootElement.GetProperty("title").GetString());
        Assert.Equal(
            "DEVICE_LIST_CORRUPT",
            problem.RootElement.GetProperty("code").GetString());
        Assert.Equal(
            "Persisted device list is corrupt: root must be an array.",
            problem.RootElement.GetProperty("detail").GetString());
    }

    [Fact]
    public async Task DeviceCertificateCorruption_Maps_To_Dedicated_Internal_Diagnostic()
    {
        var context = new DefaultHttpContext();
        context.Response.Body = new MemoryStream();
        var middleware = new ExceptionMappingMiddleware(
            _ => throw new Kodosi.Domain.DeviceCertificateCorruptionException(
                Kodosi.Domain.UserId.New(),
                "device-1",
                "certificate is malformed"),
            new OperationalMetrics(new LiveSessionStateDirectory()),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await middleware.InvokeAsync(context);

        Assert.Equal(StatusCodes.Status500InternalServerError, context.Response.StatusCode);
        context.Response.Body.Position = 0;
        using var problem = await JsonDocument.ParseAsync(
            context.Response.Body,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(
            "DEVICE_CERTIFICATE_CORRUPT",
            problem.RootElement.GetProperty("code").GetString());
    }

    [Fact]
    public async Task UnrelatedCancellation_Still_Maps_To_InternalProblem()
    {
        var context = new DefaultHttpContext();
        context.Response.Body = new MemoryStream();
        var runtimes = new LiveSessionStateDirectory();
        var middleware = new ExceptionMappingMiddleware(
            _ => throw new OperationCanceledException(),
            new OperationalMetrics(runtimes),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await middleware.InvokeAsync(context);

        Assert.Equal(StatusCodes.Status500InternalServerError, context.Response.StatusCode);
        Assert.StartsWith(
            "application/problem+json",
            context.Response.ContentType,
            StringComparison.Ordinal);
        Assert.True(context.Response.Body.Length > 0);
    }
}
