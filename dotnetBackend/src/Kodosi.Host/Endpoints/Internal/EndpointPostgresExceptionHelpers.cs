using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Kodosi.Host.Endpoints;

internal static class EndpointPostgresExceptionHelpers
{
    private const string RoomSlugConstraint = "IX_rooms_slug";
    private const string FriendshipConstraint = "PK_friendships";

    public static bool IsRoomSlugConflict(DbUpdateException exception) =>
        IsExpectedUniqueViolation(exception, RoomSlugConstraint);

    public static bool IsFriendshipConflict(DbUpdateException exception) =>
        IsExpectedUniqueViolation(exception, FriendshipConstraint);

    private static bool IsExpectedUniqueViolation(
        DbUpdateException exception,
        string constraintName) =>
        exception.InnerException is PostgresException
        {
            SqlState: PostgresErrorCodes.UniqueViolation,
            ConstraintName: var actualConstraint,
        }
        && string.Equals(actualConstraint, constraintName, StringComparison.Ordinal);
}
