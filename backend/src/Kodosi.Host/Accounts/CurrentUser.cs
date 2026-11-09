using System.Security.Claims;
using System.Text;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Accounts;

public sealed class CurrentUser(KodosiDbContext db, TimeProvider clock)
{
    public static readonly TimeSpan ReauthenticationWindow = TimeSpan.FromMinutes(15);
    private User? loaded;

    public async Task<User> GetAsync(HttpContext context, CancellationToken ct)
    {
        if (loaded is not null) return loaded;
        var issuer = context.User.FindFirstValue("iss");
        var subject = context.User.FindFirstValue("sub");
        if (string.IsNullOrWhiteSpace(issuer) || string.IsNullOrWhiteSpace(subject) || issuer.Length > 512 || subject.Length > 512)
            throw new ApiException(401, "Sign in to continue.");
        loaded = await db.Users.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
        if (loaded is not null) return loaded;
        var deleted = await db.DeletedAccounts.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
        if (deleted is not null)
        {
            var remembered = clock.GetUtcNow() - deleted.DeletedAt <= AccountService.DeletionMemory;
            if (remembered && !(SignedInAt(context.User) > deleted.DeletedAt)) throw ApiException.Gone();
            db.DeletedAccounts.Remove(deleted);
        }
        var handle = context.User.FindFirstValue("preferred_username")?.ToLowerInvariant();
        if (handle is null || handle.Length is < 3 or > 64 || !handle.All(c => char.IsAsciiLetterLower(c) || char.IsAsciiDigit(c) || c is '_' or '-'))
            throw ApiException.Forbidden("The sign-in service gave no user name that Kodosi can use.");
        if (await db.Users.AnyAsync(x => x.Handle == handle, ct))
            throw ApiException.Conflict("Another Kodosi account has this user name.");
        var created = new User
        {
            Id = Guid.CreateVersion7(),
            Issuer = issuer,
            Subject = subject,
            Handle = handle,
            DisplayName = Truncate(context.User.FindFirstValue("name") ?? handle, 128),
        };
        db.Users.Add(created);
        try
        {
            await db.SaveChangesAsync(ct);
            loaded = created;
            return loaded;
        }
        catch (DbUpdateException error) when (error.InnerException is Npgsql.PostgresException { SqlState: Npgsql.PostgresErrorCodes.UniqueViolation })
        {
            db.Entry(created).State = EntityState.Detached;
            loaded = await db.Users.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
            return loaded ?? throw ApiException.Conflict("Another Kodosi account has this user name.");
        }
    }

    public static DateTimeOffset? SignedInAt(ClaimsPrincipal principal) =>
        long.TryParse(principal.FindFirstValue("auth_time"), out var seconds) ? DateTimeOffset.FromUnixTimeSeconds(seconds) : null;

    public static void RequireRecentSignIn(DateTimeOffset? signedInAt, DateTimeOffset now, string refusal)
    {
        if (signedInAt is null || signedInAt > now.AddMinutes(5) || now - signedInAt > ReauthenticationWindow)
            throw ApiException.Forbidden(refusal);
    }

    private static string Truncate(string? value, int maximum)
    {
        var result = new StringBuilder();
        var bytes = 0;
        foreach (var rune in (value ?? "").EnumerateRunes())
        {
            if (Rune.IsControl(rune)) continue;
            if (bytes + rune.Utf8SequenceLength > maximum) break;
            result.Append(rune); bytes += rune.Utf8SequenceLength;
        }
        return result.ToString().Trim();
    }
}
