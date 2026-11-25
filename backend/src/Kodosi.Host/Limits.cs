namespace Kodosi;

internal static class Limits
{
    public const int HttpBodyBytes = 8 * 1024 * 1024;
    public const int MaxDevicesPerUser = 16;
    public const int MaxSessionsPerUser = 128;
    public const int MaxLiveSessionsPerUser = 2 * MaxSessionsPerUser;
    public const int MaxSessionMembers = 64;
    public const int MaxProjectsPerUser = 64;
    public const int MaxVisibleProjects = 256;
    public const int MaxPendingProjectInvitations = 128;
    public const int MaxProjectMembers = 64;
    public const int SessionCatalog = 4096;

    public static string Count(long count, string one, string many)
        => count == 1 ? $"1 {one}" : $"{count.ToString(System.Globalization.CultureInfo.InvariantCulture)} {many}";

    public static string Text(string? value, string field, int max)
    {
        value = Visible(value).Trim();
        if (string.IsNullOrWhiteSpace(value) || System.Text.Encoding.UTF8.GetByteCount(value) > max
            || value.Any(char.IsControl))
        {
            throw ApiException.Invalid($"{field} must contain 1 to {max} UTF-8 bytes without control characters.");
        }
        return value;
    }

    // Strips invisible Unicode format characters (such as bidi overrides) to prevent name spoofing,
    // while keeping ZWNJ (U+200C) and ZWJ (U+200D) for scripts and emoji.
    public static string Visible(string? value)
    {
        var visible = new System.Text.StringBuilder();
        foreach (var rune in (value ?? "").EnumerateRunes())
        {
            if (System.Text.Rune.GetUnicodeCategory(rune) != System.Globalization.UnicodeCategory.Format || rune.Value is 0x200C or 0x200D)
                visible.Append(rune);
        }
        return visible.ToString();
    }

    public static Guid Id(Guid value, string field)
        => value != Guid.Empty ? value : throw ApiException.Invalid($"{field} is required.");

    public static byte[] Base64(string? text, string field, int max)
    {
        if (string.IsNullOrEmpty(text) || text.Length > ((max + 2) / 3) * 4)
            throw ApiException.Invalid($"{field} has an invalid length.");
        if (!System.Buffers.Text.Base64.IsValid(text, out var length))
            throw ApiException.Invalid($"{field} must be base64.");
        return length <= max ? Convert.FromBase64String(text) : throw ApiException.Invalid($"{field} is too large.");
    }
}
