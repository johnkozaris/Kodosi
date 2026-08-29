using System.Buffers;
using System.Net.WebSockets;

namespace Kodosi.Host.Realtime;

internal static class WebSocketMessageReader
{
    internal enum ReadStatus
    {
        Message,
        Closed,
        TooLarge,
    }

    internal readonly record struct Result(
        ReadStatus Status,
        WebSocketMessageType MessageType,
        byte[] Payload);

    public static async Task<Result> ReceiveAsync(
        WebSocket webSocket,
        int maxMessageBytes,
        CancellationToken ct)
    {
        var chunkSize = Math.Min(maxMessageBytes, 8 * 1024);
        var chunkBuffer = ArrayPool<byte>.Shared.Rent(chunkSize);
        byte[]? payloadBuffer = null;

        try
        {
            var result = await webSocket.ReceiveAsync(
                new ArraySegment<byte>(chunkBuffer, 0, chunkSize),
                ct);

            if (result.MessageType == WebSocketMessageType.Close)
            {
                return new Result(ReadStatus.Closed, result.MessageType, []);
            }

            if (result.Count > maxMessageBytes || (result.Count == maxMessageBytes && !result.EndOfMessage))
            {
                return new Result(ReadStatus.TooLarge, result.MessageType, []);
            }

            if (result.EndOfMessage)
            {
                return new Result(ReadStatus.Message, result.MessageType, CopyPayload(chunkBuffer, result.Count));
            }

            payloadBuffer = ArrayPool<byte>.Shared.Rent(maxMessageBytes);
            Buffer.BlockCopy(chunkBuffer, 0, payloadBuffer, 0, result.Count);
            var payloadLength = result.Count;

            while (true)
            {
                var remainingBytes = maxMessageBytes - payloadLength;
                if (remainingBytes <= 0)
                {
                    return new Result(ReadStatus.TooLarge, result.MessageType, []);
                }

                result = await webSocket.ReceiveAsync(
                    new ArraySegment<byte>(payloadBuffer, payloadLength, Math.Min(chunkSize, remainingBytes)),
                    ct);

                if (result.MessageType == WebSocketMessageType.Close)
                {
                    return new Result(ReadStatus.Closed, result.MessageType, []);
                }

                payloadLength += result.Count;

                if (payloadLength > maxMessageBytes || (payloadLength == maxMessageBytes && !result.EndOfMessage))
                {
                    return new Result(ReadStatus.TooLarge, result.MessageType, []);
                }

                if (result.EndOfMessage)
                {
                    return new Result(ReadStatus.Message, result.MessageType, CopyPayload(payloadBuffer, payloadLength));
                }
            }
        }
        finally
        {
            if (payloadBuffer is not null)
            {
                ArrayPool<byte>.Shared.Return(payloadBuffer);
            }

            ArrayPool<byte>.Shared.Return(chunkBuffer);
        }
    }

    private static byte[] CopyPayload(byte[] source, int length)
    {
        var payload = GC.AllocateUninitializedArray<byte>(length);
        Buffer.BlockCopy(source, 0, payload, 0, length);
        return payload;
    }
}
