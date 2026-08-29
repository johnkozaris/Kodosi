using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;

namespace Kodosi.Host.Endpoints;

public static class UserEndpoints
{
    public static RouteGroupBuilder MapUserEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api")
            .WithTags("Users");

        group.MapGet("/me", async (
            UserService userService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var user = await userService.GetByIdAsync(currentUser.UserId, ct);
            if (user is null) return Results.NotFound();

            return Results.Ok(ToProfileResponse(user));
        });

        group.MapGet("/users/{id:guid}", async (
            Guid id,
            UserService userService,
            CancellationToken ct) =>
        {
            var user = await userService.GetByIdAsync(Domain.UserId.From(id), ct);
            if (user is null) return Results.NotFound();

            return Results.Ok(new UserSummaryResponse(
                user.Id.Value, user.Handle, user.DisplayName, user.AvatarUrl));
        });

        return group;
    }

    internal static UserProfileResponse ToProfileResponse(User user)
        => new(
            user.Id.Value,
            user.Handle,
            user.DisplayName,
            ResolvePublicEmail(user.Email),
            user.AvatarUrl);

    internal static string? ResolvePublicEmail(string email)
        => email.EndsWith("@users.Kodosi.invalid", StringComparison.OrdinalIgnoreCase)
            ? null
            : email;
}
