using System.Text;
using Kodosi.Accounts;

namespace Kodosi.Friends;

// Reports are log-only and intentionally skip user lookup so they cannot be used to enumerate
// usernames.
internal static partial class Reports
{
    public const int NoteBytes = 1000;

    public static void MapReports(this RouteGroupBuilder friends) =>
        friends.MapPost("/reports", (Report body, DeviceCaller caller, ILoggerFactory loggers) =>
        {
            var username = Limits.Text(body.Username, "Username", 64).ToLowerInvariant();
            Received(loggers.CreateLogger("Kodosi.Reports"), caller.User.Id, username, Note(body.Note));
            return Results.NoContent();
        }).RequireRateLimiting(RateLimits.Report);

    private static string Note(string? note)
    {
        var line = new string([.. Limits.Visible(note ?? "").Select(letter => char.IsControl(letter) ? ' ' : letter)]).Trim();
        if (Encoding.UTF8.GetByteCount(line) > NoteBytes)
            throw ApiException.Invalid($"A note has at most {NoteBytes} UTF-8 bytes.");
        return line;
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "Report from account {Reporter} about {Username}: {Note}")]
    private static partial void Received(ILogger logger, Guid reporter, string username, string note);

    private sealed record Report(string Username, string? Note);
}
