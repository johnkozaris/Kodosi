namespace Kodosi;

public sealed class ApiException(int status, string message, string? limit = null, long? most = null) : Exception(message)
{
    public int Status { get; } = status;
    /// <summary>The plan limit that refused the request and its number, so that each app says it in its own language.</summary>
    public string? Limit { get; } = limit;
    public long? Most { get; } = most;
    public static ApiException OverLimit(string limit, long most, string message) => new(403, message, limit, most);
    public static ApiException Invalid(string message) => new(400, message);
    public static ApiException Forbidden(string message = "This operation is not authorized.") => new(403, message);
    public static ApiException Missing() => new(404, "The requested resource is unavailable.");
    public static ApiException Conflict(string message) => new(409, message);
    public static ApiException Gone() => new(410, "This Kodosi account was deleted.");
}
