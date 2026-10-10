using System.Security.Claims;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Accounts;

public sealed class CurrentUser(KodosiDbContext db, TimeProvider clock)
{
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
            var signedIn = long.TryParse(context.User.FindFirstValue("auth_time"), out var seconds) ? DateTimeOffset.FromUnixTimeSeconds(seconds) : (DateTimeOffset?)null;
            if (remembered && !(signedIn > deleted.DeletedAt)) throw ApiException.Gone();
            db.DeletedAccounts.Remove(deleted);
        }
        var rawName = context.User.FindFirstValue("preferred_username") ?? context.User.FindFirstValue("name") ?? "user";
        var name = new string(rawName.Where(c => char.IsAsciiLetterOrDigit(c) || c is '_' or '-').Take(40).ToArray()).ToLowerInvariant();
        if (name.Length < 3) name = "user";
        var suffix = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(issuer + "\n" + subject)))[..10];
        for (var attempt = 0; attempt < 8; attempt++)
        {
            var handle = attempt switch
            {
                0 => name,
                1 => $"{name}-{suffix}",
                _ => $"{name}-{Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(8))}"
            };
            if (await db.Users.AnyAsync(x => x.Handle == handle, ct)) continue;
            var created = new User
            {
                Id = Guid.CreateVersion7(),
                Issuer = issuer,
                Subject = subject,
                Handle = handle,
                DisplayName = Truncate(context.User.FindFirstValue("name") ?? rawName, 128),
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
                if (loaded is not null) return loaded;
            }
        }
        throw ApiException.Conflict("The account could not be created; retry sign-in.");
    }
    public static DateTimeOffset? SignedInAt(ClaimsPrincipal principal)
    {
        var value = principal.FindFirstValue("auth_time") ?? principal.FindFirstValue("iat");
        return long.TryParse(value, out var seconds) ? DateTimeOffset.FromUnixTimeSeconds(seconds) : null;
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

internal static class AccountEndpoints
{
    public static void MapAccounts(this IEndpointRouteBuilder app)
    {
        app.MapGet("/api/me", async (HttpContext context, CurrentUser users, AccountDeletionSettings deletion, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            return Results.Ok(new { user.Id, user.Handle, user.DisplayName, deletionUri = deletion.ConfirmationUri() });
        }).RequireAuthorization();
        app.MapDelete("/api/me", async (HttpContext context, CurrentUser users, AccountService accounts, AccountDeletionSettings deletion, CancellationToken ct) =>
        {
            if (deletion.ReadsSignInService) throw ApiException.Conflict("Delete your account on the account page of the sign-in service.");
            var user = await users.GetAsync(context, ct);
            await accounts.DeleteSignedInAsync(user, CurrentUser.SignedInAt(context.User), ct);
            return Results.NoContent();
        }).RequireAuthorization().RequireRateLimiting("enrollment");
    }
}
