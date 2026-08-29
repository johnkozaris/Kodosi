using System.Globalization;
using System.Text;
using Kodosi.Domain;

namespace Kodosi.Application;


public sealed record FeedCursor(
    DateTimeOffset StartedAt,
    Guid Id)
{
    public string Encode()
    {
        var raw = FormattableString.Invariant(
            $"v2_{StartedAt.UtcDateTime.Ticks}_{Id:D}");
        return Convert.ToBase64String(Encoding.UTF8.GetBytes(raw));
    }

    public static FeedCursor? Decode(string? cursor)
    {
        if (string.IsNullOrWhiteSpace(cursor)) return null;

        try
        {
            var raw = Encoding.UTF8.GetString(Convert.FromBase64String(cursor));
            var parts = raw.Split('_');
            if (parts is ["v2", var ticksText, var v2IdText]
                && long.TryParse(
                    ticksText,
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out var ticks)
                && Guid.TryParse(v2IdText, out var v2Id))
            {
                return new FeedCursor(
                    new DateTimeOffset(ticks, TimeSpan.Zero),
                    v2Id);
            }
        }
        catch (Exception ex) when (ex is FormatException or ArgumentOutOfRangeException)
        {
            throw InvalidCursor();
        }

        throw InvalidCursor();
    }

    private static DomainException InvalidCursor() =>
        new("The feed cursor is invalid.");
}
