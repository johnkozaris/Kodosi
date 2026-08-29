using System.Globalization;
using System.Text;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SemanticReceiptCursor(
    DateTimeOffset StoredAt,
    Guid Id)
{
    public string Encode()
    {
        var raw = FormattableString.Invariant(
            $"v1_{StoredAt.UtcDateTime.Ticks}_{Id:D}");
        return Convert.ToBase64String(Encoding.UTF8.GetBytes(raw));
    }

    public static SemanticReceiptCursor? Decode(string? cursor)
    {
        if (string.IsNullOrWhiteSpace(cursor))
        {
            return null;
        }

        try
        {
            var raw = Encoding.UTF8.GetString(Convert.FromBase64String(cursor));
            var parts = raw.Split('_');
            if (parts is ["v1", var ticksText, var idText]
                && long.TryParse(
                    ticksText,
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out var ticks)
                && Guid.TryParse(idText, out var id))
            {
                return new SemanticReceiptCursor(
                    new DateTimeOffset(ticks, TimeSpan.Zero),
                    id);
            }
        }
        catch (Exception exception)
            when (exception is FormatException or ArgumentOutOfRangeException)
        {
            throw InvalidCursor();
        }

        throw InvalidCursor();
    }

    private static DomainException InvalidCursor() =>
        new("The semantic receipt cursor is invalid.");
}

public sealed record SemanticReceiptPage(
    IReadOnlyList<SemanticRelayReceipt> Items,
    string? NextCursor);
