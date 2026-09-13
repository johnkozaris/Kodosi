using System.Security.Claims;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Accounts;

public sealed class CurrentUser(KodosiDbContext db)
{
    private User? loaded;
    public async Task<User> GetAsync(HttpContext context, CancellationToken ct)
    {
        if (loaded is not null) return loaded;
        var issuer = context.Items[Authentication.CanonicalIssuerItem] as string ?? context.User.FindFirstValue("iss");
        var subject = context.User.FindFirstValue("sub");
        if (string.IsNullOrWhiteSpace(issuer) || string.IsNullOrWhiteSpace(subject) || issuer.Length > 512 || subject.Length > 512)
            throw new ApiException(401, "Sign in to continue.");
        loaded = await db.Users.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct);
        if (loaded is not null) return loaded;
        var rawName = context.User.FindFirstValue("preferred_username") ?? context.User.FindFirstValue("name") ?? "user";
        var name = new string(rawName.Where(c => char.IsAsciiLetterOrDigit(c) || c is '_' or '-').Take(40).ToArray()).ToLowerInvariant();
        if (name.Length < 3) name = "user";
        var suffix = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(issuer + "\n" + subject)))[..10];
        var handle = await db.Users.AnyAsync(x => x.Handle == name, ct) ? $"{name}-{suffix}" : name;
        loaded = new User
        {
            Id = Guid.CreateVersion7(),
            Issuer = issuer,
            Subject = subject,
            Handle = handle,
            DisplayName = Truncate(context.User.FindFirstValue("name") ?? rawName, 128),
            Email = Truncate(context.User.FindFirstValue("email"), 320),
            AvatarUrl = Truncate(context.User.FindFirstValue("picture"), 2048),
        };
        db.Users.Add(loaded);
        try { await db.SaveChangesAsync(ct); }
        catch (DbUpdateException)
        {
            db.Entry(loaded).State = EntityState.Detached;
            loaded = await db.Users.SingleOrDefaultAsync(x => x.Issuer == issuer && x.Subject == subject, ct)
                ?? throw ApiException.Conflict("The account could not be created; retry sign-in.");
        }
        return loaded;
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
        app.MapGet("/api/me", async (HttpContext context, CurrentUser users, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            return Results.Ok(new { user.Id, user.Handle, user.DisplayName, user.Email, user.AvatarUrl });
        }).RequireAuthorization();
    }
}
