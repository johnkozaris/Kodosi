namespace Kodosi.Application;





public sealed record PaginatedFeedResponse<T>(
    IReadOnlyList<T> Items,
    string? NextCursor,
    bool HasMore,
    bool Truncated = false);
