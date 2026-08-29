using System.Security.Cryptography;
using System.Text;
using Kodosi.Host.Auth;
using Kodosi.Host.Middleware;
using Kodosi.Host.Observability;
using Kodosi.Infrastructure.Realtime;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class DeviceProofJsonBodyTests
{
    private sealed record Payload(string Signature, string DeviceId);

    private static void Parameter(Payload payload) => _ = payload;

    [Fact]
    public async Task Binder_Hashes_Exact_Raw_Json_Bytes()
    {
        const string json = "{\n  \"signature\": \"ab+c/==&x=y\", \"deviceId\": \"设备-é&+/%\"\n}";
        var bytes = Encoding.UTF8.GetBytes(json);
        var context = JsonContext(bytes);

        var body = await DeviceProofJsonBody<Payload>.BindAsync(context, ParameterInfo());

        Assert.NotNull(body);
        Assert.Equal("ab+c/==&x=y", body.Value.Signature);
        Assert.Equal("设备-é&+/%", body.Value.DeviceId);
        Assert.Equal(
            Convert.ToHexStringLower(SHA256.HashData(bytes)),
            body.Sha256);
        Assert.NotEqual(
            body.Sha256,
            Convert.ToHexStringLower(SHA256.HashData(
                System.Text.Json.JsonSerializer.SerializeToUtf8Bytes(body.Value))));
    }

    [Fact]
    public async Task Binder_Rejects_NonJson_Media_Type()
    {
        var context = JsonContext("{}"u8.ToArray());
        context.Request.ContentType = "text/plain";

        var error = await Assert.ThrowsAsync<BadHttpRequestException>(async () =>
            await DeviceProofJsonBody<Payload>.BindAsync(context, ParameterInfo()));

        Assert.Equal(StatusCodes.Status415UnsupportedMediaType, error.StatusCode);
    }

    [Fact]
    public async Task Binder_Rejects_Malformed_Or_Null_Json_As_Bad_Request()
    {
        foreach (var bytes in new[] { "{"u8.ToArray(), "null"u8.ToArray(), [] })
        {
            var context = JsonContext(bytes);
            var error = await Assert.ThrowsAsync<BadHttpRequestException>(async () =>
                await DeviceProofJsonBody<Payload>.BindAsync(context, ParameterInfo()));
            Assert.Equal(StatusCodes.Status400BadRequest, error.StatusCode);
        }
    }

    [Fact]
    public async Task Binder_Rejects_Chunked_Body_Above_Exact_Limit()
    {
        var context = JsonContext(new byte[64 * 1024 + 1]);
        context.Request.ContentLength = null;

        var error = await Assert.ThrowsAsync<BadHttpRequestException>(async () =>
            await DeviceProofJsonBody<Payload>.BindAsync(context, ParameterInfo()));

        Assert.Equal(StatusCodes.Status413PayloadTooLarge, error.StatusCode);
    }

    [Theory]
    [InlineData("text/plain", "{}", StatusCodes.Status415UnsupportedMediaType)]
    [InlineData("application/json", "{", StatusCodes.Status400BadRequest)]
    public async Task Binder_Errors_Preserve_Client_Status_Through_Production_Middleware(
        string contentType,
        string body,
        int expectedStatus)
    {
        var context = JsonContext(Encoding.UTF8.GetBytes(body));
        context.Request.ContentType = contentType;
        context.Response.Body = new MemoryStream();
        var middleware = new ExceptionMappingMiddleware(
            async requestContext =>
            {
                _ = await DeviceProofJsonBody<Payload>.BindAsync(
                    requestContext,
                    ParameterInfo());
            },
            new OperationalMetrics(new LiveSessionStateDirectory()),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await middleware.InvokeAsync(context);

        Assert.Equal(expectedStatus, context.Response.StatusCode);
        Assert.StartsWith(
            "application/problem+json",
            context.Response.ContentType,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task Chunked_OverLimit_Body_Preserves_413_Through_Production_Middleware()
    {
        var context = JsonContext(new byte[64 * 1024 + 1]);
        context.Request.ContentLength = null;
        context.Response.Body = new MemoryStream();
        var middleware = new ExceptionMappingMiddleware(
            async requestContext =>
            {
                _ = await DeviceProofJsonBody<Payload>.BindAsync(
                    requestContext,
                    ParameterInfo());
            },
            new OperationalMetrics(new LiveSessionStateDirectory()),
            NullLogger<ExceptionMappingMiddleware>.Instance);

        await middleware.InvokeAsync(context);

        Assert.Equal(StatusCodes.Status413PayloadTooLarge, context.Response.StatusCode);
        Assert.StartsWith(
            "application/problem+json",
            context.Response.ContentType,
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task EmptyBodyProof_Rejects_Declared_And_Chunked_Body_Bytes()
    {
        foreach (var contentLength in new long?[] { 1, null })
        {
            var context = new DefaultHttpContext();
            context.Request.ContentLength = contentLength;
            context.Request.Body = new MemoryStream([0x01]);

            Assert.False(await DeviceHttpRequestProofVerifier.HasExpectedBodyAsync(
                context.Request,
                DeviceHttpRequestProofVerifier.EmptyBodySha256,
                TestContext.Current.CancellationToken));
        }

        var empty = new DefaultHttpContext();
        empty.Request.Body = new MemoryStream();
        Assert.True(await DeviceHttpRequestProofVerifier.HasExpectedBodyAsync(
            empty.Request,
            DeviceHttpRequestProofVerifier.EmptyBodySha256,
            TestContext.Current.CancellationToken));
    }

    private static DefaultHttpContext JsonContext(byte[] bytes)
    {
        var context = new DefaultHttpContext();
        context.Request.ContentType = "application/json; charset=utf-8";
        context.Request.ContentLength = bytes.Length;
        context.Request.Body = new MemoryStream(bytes);
        return context;
    }

    private static System.Reflection.ParameterInfo ParameterInfo() =>
        typeof(DeviceProofJsonBodyTests)
            .GetMethod(nameof(Parameter), System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Static)!
            .GetParameters()
            .Single();
}
