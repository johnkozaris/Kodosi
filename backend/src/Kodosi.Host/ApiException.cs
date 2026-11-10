namespace Kodosi;

public sealed class ApiException(int status, string message, string? limit = null, long? most = null) : Exception(message)
{
    public int Status { get; } = status;
    public string? Limit { get; } = limit;
    public long? Most { get; } = most;
    public static ApiException OverLimit(string limit, long most, string message) => new(403, message, limit, most);
    public static ApiException Invalid(string message) => new(400, message);
    public static ApiException Forbidden(string message = "This operation is not authorized.") => new(403, message);
    public static ApiException Missing(string message = "The requested resource is unavailable.") => new(404, message);
    public static ApiException Conflict(string message) => new(409, message);
    public static ApiException Gone() => new(410, "This Kodosi account was deleted.");
    public static ApiException Banned() =>
        new(423, "This account is banned because it tried too many user names. Ask the operator of this Kodosi server to remove the ban.");
}
