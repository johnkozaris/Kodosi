using System.Buffers.Binary;
using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;

namespace Kodosi.Security;

/// <summary>
/// The device sessions and challenges of the one serving process. A check of a session reads without a lock. The
/// server keeps only a hash of each session secret, and removes expired entries once each minute.
/// </summary>
public sealed class DeviceSessions(TimeProvider clock)
{
    private const int ChallengesForEachUser = 16;
    private const int SessionsForEachDevice = 4;
    private const int MaximumChallenges = 20_000;
    private const int MaximumSessions = 200_000;
    private const int MaximumSecretLength = 64;
    private static readonly TimeSpan ChallengeLife = TimeSpan.FromMinutes(2);
    private static readonly TimeSpan SessionLife = TimeSpan.FromHours(12);
    private static readonly TimeSpan SweepPeriod = TimeSpan.FromMinutes(1);
    private readonly Lock sync = new();
    private readonly Dictionary<Guid, Challenge> challenges = [];
    private readonly Dictionary<Guid, int> challengesByUser = [];
    private readonly ConcurrentDictionary<SessionKey, Session> sessions = new();
    private readonly Dictionary<Guid, Dictionary<string, List<SessionKey>>> byDevice = [];
    private DateTimeOffset swept;

    public int Count => sessions.Count;

    public (Guid Id, byte[] Bytes) NewChallenge(Guid userId)
    {
        var now = clock.GetUtcNow();
        var challenge = new Challenge(userId, RandomNumberGenerator.GetBytes(32), now + ChallengeLife);
        var id = Guid.CreateVersion7();
        lock (sync)
        {
            SweepLocked(now);
            var held = challengesByUser.GetValueOrDefault(userId);
            if (challenges.Count >= MaximumChallenges || held >= ChallengesForEachUser)
                throw new ApiException(429, "Too many pending device challenges.");
            challenges.Add(id, challenge);
            challengesByUser[userId] = held + 1;
        }
        return (id, challenge.Bytes);
    }

    public byte[] ConsumeChallenge(Guid userId, Guid id)
    {
        lock (sync)
        {
            if (challenges.Remove(id, out var challenge))
            {
                ForgetChallengeLocked(challenge.UserId);
                if (challenge.UserId == userId && challenge.ExpiresAt > clock.GetUtcNow()) return challenge.Bytes;
            }
        }
        throw ApiException.Forbidden("The device challenge expired or was already used.");
    }

    public string Open(Guid userId, string deviceId)
    {
        var now = clock.GetUtcNow();
        var secret = Convert.ToBase64String(RandomNumberGenerator.GetBytes(32));
        var key = SessionKey.Of(secret);
        lock (sync)
        {
            SweepLocked(now);
            if (!byDevice.TryGetValue(userId, out var devices)) byDevice[userId] = devices = new Dictionary<string, List<SessionKey>>(StringComparer.Ordinal);
            if (!devices.TryGetValue(deviceId, out var held)) devices[deviceId] = held = [];
            while (held.Count >= SessionsForEachDevice)
            {
                sessions.TryRemove(held[0], out _);
                held.RemoveAt(0);
            }
            if (sessions.Count >= MaximumSessions) throw new ApiException(503, "Too many device sessions.");
            sessions[key] = new Session(userId, deviceId, now + SessionLife);
            held.Add(key);
        }
        return secret;
    }

    public bool Holds(Guid userId, string deviceId, string? secret) =>
        secret is { Length: > 0 and <= MaximumSecretLength }
        && sessions.TryGetValue(SessionKey.Of(secret), out var session)
        && session.UserId == userId && session.DeviceId == deviceId && session.ExpiresAt > clock.GetUtcNow();

    public void Remove(Guid userId, string? deviceId = null)
    {
        lock (sync)
        {
            if (!byDevice.TryGetValue(userId, out var devices)) return;
            foreach (var device in deviceId is null ? devices.Keys.ToArray() : [deviceId])
                if (devices.Remove(device, out var held))
                    foreach (var key in held) sessions.TryRemove(key, out _);
            if (devices.Count == 0) byDevice.Remove(userId);
        }
    }

    private void SweepLocked(DateTimeOffset now)
    {
        if (now - swept < SweepPeriod) return;
        swept = now;
        foreach (var (id, challenge) in challenges.Where(x => x.Value.ExpiresAt <= now).ToArray())
        {
            challenges.Remove(id);
            ForgetChallengeLocked(challenge.UserId);
        }
        foreach (var (userId, devices) in byDevice.ToArray())
        {
            foreach (var (deviceId, held) in devices.ToArray())
            {
                held.RemoveAll(key =>
                {
                    if (sessions.TryGetValue(key, out var session) && session.ExpiresAt > now) return false;
                    sessions.TryRemove(key, out _);
                    return true;
                });
                if (held.Count == 0) devices.Remove(deviceId);
            }
            if (devices.Count == 0) byDevice.Remove(userId);
        }
    }

    private void ForgetChallengeLocked(Guid userId)
    {
        var held = challengesByUser.GetValueOrDefault(userId) - 1;
        if (held <= 0) challengesByUser.Remove(userId);
        else challengesByUser[userId] = held;
    }

    private sealed record Challenge(Guid UserId, byte[] Bytes, DateTimeOffset ExpiresAt);

    private sealed record Session(Guid UserId, string DeviceId, DateTimeOffset ExpiresAt);

    /// <summary>The SHA-256 of a session secret, so that the memory of the server never holds the secret itself.</summary>
    private readonly record struct SessionKey(UInt128 First, UInt128 Second)
    {
        public static SessionKey Of(string secret)
        {
            Span<byte> text = stackalloc byte[MaximumSecretLength * 3];
            var length = Encoding.UTF8.GetBytes(secret, text);
            Span<byte> hash = stackalloc byte[SHA256.HashSizeInBytes];
            SHA256.HashData(text[..length], hash);
            return new SessionKey(BinaryPrimitives.ReadUInt128LittleEndian(hash), BinaryPrimitives.ReadUInt128LittleEndian(hash[16..]));
        }
    }
}
