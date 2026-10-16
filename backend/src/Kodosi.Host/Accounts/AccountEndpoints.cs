namespace Kodosi.Accounts;

internal static class AccountEndpoints
{
    public static void MapAccounts(this IEndpointRouteBuilder app)
    {
        app.MapGet("/api/me", (SignedIn me, AccountDeletionSettings deletion) =>
            Results.Ok(new { me.User.Id, me.User.Handle, me.User.DisplayName, deletionUri = deletion.ConfirmationUri() })).RequireAuthorization();
        app.MapDelete("/api/me", async (SignedIn me, HttpContext context, AccountService accounts, AccountDeletionSettings deletion, CancellationToken ct) =>
        {
            if (deletion.ReadsSignInService) throw ApiException.Conflict("Delete your account on the account page of the sign-in service.");
            await accounts.DeleteSignedInAsync(me.User, CurrentUser.SignedInAt(context.User), ct);
            return Results.NoContent();
        }).RequireAuthorization().RequireRateLimiting(RateLimits.Enrollment);
    }
}
