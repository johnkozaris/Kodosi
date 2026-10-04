using System.Security.Cryptography;

namespace Kodosi.Security;

public sealed class DeviceSessions(TimeProvider clock)
{
    private const int ChallengesForEachUser = 16;
    private const int SessionsForEachDevice = 4;
    private const int MaximumChallenges = 20_000;
    private const int MaximumSessions = 200_000;
    private static readonly TimeSpan ChallengeLife = TimeSpan.FromMinutes(2);
    private static readonly TimeSpan SessionLife = TimeSpan.FromHours(12);
    private readonly Lock sync = new();
    private readonly Dictionary<Guid, Challenge> challenges = [];
    private readonly Dictionary<string, Session> sessions = [];

    public (Guid Id, byte[] Bytes, DateTimeOffset ExpiresAt) NewChallenge(Guid userId)
    {
        var now = clock.GetUtcNow();
        var challenge = new Challenge(userId, RandomNumberGenerator.GetBytes(32), now + ChallengeLife);
        var id = Guid.CreateVersion7();
        lock (sync)
        {
            foreach (var expired in challenges.Where(x => x.Value.ExpiresAt <= now).Select(x => x.Key).ToArray()) challenges.Remove(expired);
            if (challenges.Count >= MaximumChallenges || challenges.Values.Count(x => x.UserId == userId) >= ChallengesForEachUser)
                throw new ApiException(429, "Too many pending device challenges.");
            challenges.Add(id, challenge);
        }
        return (id, challenge.Bytes, challenge.ExpiresAt);
    }

    public byte[] ConsumeChallenge(Guid userId, Guid id)
    {
        lock (sync)
        {
            if (challenges.Remove(id, out var challenge) && challenge.UserId == userId && challenge.ExpiresAt > clock.GetUtcNow())
                return challenge.Bytes;
        }
        throw ApiException.Forbidden("The device challenge expired or was already used.");
    }

    public (string Token, DateTimeOffset ExpiresAt) Open(Guid userId, string deviceId)
    {
        var now = clock.GetUtcNow();
        var token = Convert.ToBase64String(RandomNumberGenerator.GetBytes(32));
        var session = new Session(userId, deviceId, now + SessionLife);
        lock (sync)
        {
            foreach (var expired in sessions.Where(x => x.Value.ExpiresAt <= now).Select(x => x.Key).ToArray()) sessions.Remove(expired);
            var held = sessions.Where(x => x.Value.UserId == userId && x.Value.DeviceId == deviceId).OrderBy(x => x.Value.ExpiresAt).ToArray();
            foreach (var oldest in held.Take(Math.Max(0, held.Length - SessionsForEachDevice + 1))) sessions.Remove(oldest.Key);
            if (sessions.Count >= MaximumSessions) throw new ApiException(503, "Too many device sessions.");
            sessions.Add(Key(token), session);
        }
        return (token, session.ExpiresAt);
    }

    public bool Holds(Guid userId, string deviceId, string? token)
    {
        if (string.IsNullOrEmpty(token) || token.Length > 64) return false;
        lock (sync)
            return sessions.TryGetValue(Key(token), out var session) && session.UserId == userId && session.DeviceId == deviceId
                && session.ExpiresAt > clock.GetUtcNow();
    }

    public void Remove(Guid userId, string? deviceId = null)
    {
        lock (sync)
            foreach (var key in sessions.Where(x => x.Value.UserId == userId && (deviceId is null || x.Value.DeviceId == deviceId)).Select(x => x.Key).ToArray())
                sessions.Remove(key);
    }

    private static string Key(string token) => Convert.ToHexStringLower(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(token)));
    private sealed record Challenge(Guid UserId, byte[] Bytes, DateTimeOffset ExpiresAt);
    private sealed record Session(Guid UserId, string DeviceId, DateTimeOffset ExpiresAt);
}
