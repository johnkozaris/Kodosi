using Kodosi.Domain;

namespace Kodosi.Host.Endpoints;

internal static class FeedQueryParsers
{
    public static bool TryParseToolKind(string? value, out ToolKind? toolKind)
    {
        toolKind = null;
        if (string.IsNullOrWhiteSpace(value))
        {
            return true;
        }

        if (!Enum.TryParse<ToolKind>(value, ignoreCase: true, out var result)
            || !Enum.IsDefined(result))
        {
            return false;
        }

        toolKind = result;
        return true;
    }
}
