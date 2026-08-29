using Kodosi.Domain;

namespace Kodosi.Application;

public static class RoomChatReadPolicy
{
    public const int DefaultLimit = 100;
    public const int MaxLimit = 1000;
    public const int MaxPageBodyCharacters =
        2 * RoomInputRules.EncryptedContentMaxLength;

    public static int Normalize(int? limit) =>
        Math.Clamp(limit ?? DefaultLimit, 1, MaxLimit);

    public static int CandidateLimit(int? limit) => Normalize(limit) + 1;

    public static RoomChatReadPage CreateTailPage(
        IReadOnlyList<RoomChatMessage> descendingCandidates,
        int? limit)
    {
        var normalizedLimit = Normalize(limit);
        var descending = new List<RoomChatMessage>(
            Math.Min(descendingCandidates.Count, normalizedLimit));
        long bodyCharacters = 0;
        foreach (var candidate in descendingCandidates)
        {
            if (descending.Count == normalizedLimit
                || (descending.Count > 0
                    && bodyCharacters + candidate.Body.Length > MaxPageBodyCharacters))
            {
                break;
            }

            descending.Add(candidate);
            bodyCharacters += candidate.Body.Length;
        }

        var hasMore = descendingCandidates.Count > descending.Count;
        long? nextBefore = hasMore ? descending[^1].Seq : null;
        descending.Reverse();
        return new RoomChatReadPage(descending, null, nextBefore, hasMore);
    }

    public static RoomChatReadPage CreatePage(
        IReadOnlyList<RoomChatMessage> candidates,
        int? limit)
    {
        var normalizedLimit = Normalize(limit);
        var items = new List<RoomChatMessage>(Math.Min(candidates.Count, normalizedLimit));
        long bodyCharacters = 0;
        foreach (var candidate in candidates)
        {
            if (items.Count == normalizedLimit
                || (items.Count > 0
                    && bodyCharacters + candidate.Body.Length > MaxPageBodyCharacters))
            {
                break;
            }

            items.Add(candidate);
            bodyCharacters += candidate.Body.Length;
        }

        var hasMore = candidates.Count > items.Count;
        return new RoomChatReadPage(
            items,
            hasMore ? items[^1].Seq : null,
            null,
            hasMore);
    }
}

public sealed record RoomChatReadPage(
    IReadOnlyList<RoomChatMessage> Items,
    long? NextSince,
    long? NextBefore,
    bool HasMore);
