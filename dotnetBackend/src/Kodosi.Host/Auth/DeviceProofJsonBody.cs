using System.Reflection;
using System.Security.Cryptography;
using System.Text.Json;

namespace Kodosi.Host.Auth;

internal sealed class DeviceProofJsonBody<T>(T value, string sha256)
{
    private const int MaxBodyBytes = 64 * 1024;

    public T Value { get; } = value;
    public string Sha256 { get; } = sha256;

    public static async ValueTask<DeviceProofJsonBody<T>?> BindAsync(
        HttpContext context,
        ParameterInfo parameter)
    {
        _ = parameter;
        if (!context.Request.HasJsonContentType())
        {
            throw new BadHttpRequestException(
                "Request content type must be JSON.",
                StatusCodes.Status415UnsupportedMediaType);
        }
        if (context.Request.ContentLength is > MaxBodyBytes)
        {
            throw new BadHttpRequestException(
                "Request body exceeds the configured limit.",
                StatusCodes.Status413PayloadTooLarge);
        }

        using var stream = new MemoryStream();
        var buffer = new byte[16 * 1024];
        while (true)
        {
            var read = await context.Request.Body.ReadAsync(
                buffer,
                context.RequestAborted);
            if (read == 0)
            {
                break;
            }
            if (stream.Length + read > MaxBodyBytes)
            {
                throw new BadHttpRequestException(
                    "Request body exceeds the configured limit.",
                    StatusCodes.Status413PayloadTooLarge);
            }
            stream.Write(buffer, 0, read);
        }

        var bytes = stream.ToArray();
        T? value;
        try
        {
            value = JsonSerializer.Deserialize<T>(
                bytes,
                new JsonSerializerOptions(JsonSerializerDefaults.Web));
        }
        catch (JsonException error)
        {
            throw new BadHttpRequestException("Request body is not valid JSON.", error);
        }
        if (value is null)
        {
            throw new BadHttpRequestException("Request body must not be null.");
        }
        return new DeviceProofJsonBody<T>(
            value,
            Convert.ToHexStringLower(SHA256.HashData(bytes)));
    }
}
