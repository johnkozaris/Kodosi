namespace Kodosi;

public sealed class ApiException(int status, string message) : Exception(message)
{
    public int Status { get; } = status;
    public static ApiException Invalid(string message) => new(400, message);
    public static ApiException Forbidden(string message = "This operation is not authorized.") => new(403, message);
    public static ApiException Missing() => new(404, "The requested resource is unavailable.");
    public static ApiException Conflict(string message) => new(409, message);
}

internal static class Limits
{
    public const int HttpBodyBytes = 2 * 1024 * 1024;
    public const int MaxSessionsPerUser = 128;
    public const int MaxSessionMembers = 64;
    public const int MaxRoomsPerUser = 64;
    public const int MaxVisibleRooms = 256;
    public const int MaxPendingRoomInvitations = 128;
    public const int MaxRoomMembers = 64;

    public static string Text(string? value, string field, int max)
    {
        value = value?.Trim();
        if (string.IsNullOrWhiteSpace(value) || System.Text.Encoding.UTF8.GetByteCount(value) > max
            || value.Any(char.IsControl))
        {
            throw ApiException.Invalid($"{field} must contain 1–{max} UTF-8 bytes without control characters.");
        }
        return value;
    }

    public static Guid Id(Guid value, string field)
        => value != Guid.Empty ? value : throw ApiException.Invalid($"{field} is required.");

    public static byte[] Base64(string? text, string field, int max)
    {
        if (string.IsNullOrEmpty(text) || text.Length > ((max + 2) / 3) * 4)
            throw ApiException.Invalid($"{field} has an invalid length.");
        try
        {
            var bytes = Convert.FromBase64String(text);
            return bytes.Length <= max ? bytes : throw ApiException.Invalid($"{field} is too large.");
        }
        catch (FormatException)
        {
            throw ApiException.Invalid($"{field} must be base64.");
        }
    }
}
