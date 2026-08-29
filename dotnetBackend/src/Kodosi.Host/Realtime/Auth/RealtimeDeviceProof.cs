using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal static class RealtimeDeviceProof
{
    private const int ChallengeBytes = 32;

    public static async Task<bool> SendChallengeAsync(
        WebSocket socket,
        string connectionId,
        string purpose,
        string? sessionId,
        TimeSpan timeout,
        CancellationToken ct,
        Action<byte[]> retainChallenge)
    {
        var challenge = RandomNumberGenerator.GetBytes(ChallengeBytes);
        retainChallenge(challenge);
        var message = new DeviceProofChallengeMessage(
            connectionId,
            purpose,
            sessionId,
            Convert.ToBase64String(challenge));
        var payload = JsonSerializer.SerializeToUtf8Bytes(
            message,
            WsJsonContext.Default.DeviceProofChallengeMessage);
        return await WebSocketSendDeadline.SendAsync(
            socket,
            payload,
            WebSocketMessageType.Text,
            timeout,
            ct);
    }

    public static async Task<ActiveDeviceAuthorizationDecision?> VerifyAsync(
        IServiceScopeFactory scopeFactory,
        RealtimeDeviceAuthorizationReader authorization,
        UserId userId,
        string deviceId,
        string signatureBase64,
        string connectionId,
        string purpose,
        string? sessionId,
        Guid? incarnationId,
        byte[] challenge,
        CancellationToken ct)
    {
        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(signatureBase64);
        }
        catch (FormatException)
        {
            return null;
        }

        var decision = await authorization.EvaluateAsync(userId, deviceId, ct);
        if (!decision.Authorized)
        {
            return null;
        }

        await using var scope = scopeFactory.CreateAsyncScope();
        var devices = scope.ServiceProvider.GetRequiredService<IUserDeviceRepository>();
        var verifier = scope.ServiceProvider.GetRequiredService<IPopSignatureVerifier>();
        var device = await devices.GetByDeviceIdAsync(deviceId, ct);
        if (device is null || device.UserId != userId || device.RevokedAt.HasValue)
        {
            return null;
        }

        var preimage = DeviceConnectionProofPreimage.Create(
            userId,
            deviceId,
            connectionId,
            purpose,
            sessionId,
            incarnationId,
            challenge);
        return verifier.Verify(device.SigningPublicKey, preimage, signature)
            ? decision
            : null;
    }
}
