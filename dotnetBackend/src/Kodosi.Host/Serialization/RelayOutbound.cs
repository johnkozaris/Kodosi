using System.Text.Json;

namespace Kodosi.Host.Serialization;

internal static class RelayOutbound
{
    public static byte[] Encode<T>(T message)
        where T : notnull
    {
        return JsonSerializer.SerializeToUtf8Bytes(message, typeof(T), WsJsonContext.Default);
    }
}
