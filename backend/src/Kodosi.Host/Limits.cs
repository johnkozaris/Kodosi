namespace Kodosi;

internal static class Limits
{
    public const int HttpBodyBytes = 8 * 1024 * 1024;
    public const int MaxDevicesPerUser = 16;
    public const int MaxSessionsPerUser = 128;
    public const int MaxLiveSessionsPerUser = 2 * MaxSessionsPerUser;
    public const int MaxSessionMembers = 64;
    public const int MaxMissionsPerUser = 64;
    public const int MaxVisibleMissions = 256;
    public const int MaxPendingMissionInvitations = 128;
    public const int MaxMissionMembers = 64;
    public const int SessionCatalog = 4096;

    public static string Count(long count, string one, string many)
        => count == 1 ? $"1 {one}" : $"{count.ToString(System.Globalization.CultureInfo.InvariantCulture)} {many}";

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
