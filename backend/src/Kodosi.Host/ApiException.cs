namespace Kodosi;

public sealed class ApiException(int status, string message) : Exception(message)
{
    public int Status { get; } = status;
    public static ApiException Invalid(string message) => new(400, message);
    public static ApiException Forbidden(string message = "This operation is not authorized.") => new(403, message);
    public static ApiException Missing() => new(404, "The requested resource is unavailable.");
    public static ApiException Conflict(string message) => new(409, message);
    public static ApiException Gone() => new(410, "This Kodosi account was deleted.");
}
