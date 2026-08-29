using Kodosi.Host.Endpoints;
using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Kodosi.HostTests;

public sealed class EndpointPostgresExceptionHelpersTests
{
    [Theory]
    [InlineData("IX_rooms_slug", true)]
    [InlineData("PK_friendships", false)]
    [InlineData("IX_unrelated", false)]
    public void RoomSlugConflict_Requires_Expected_Unique_Constraint(
        string constraintName,
        bool expected)
    {
        var exception = WrapPostgresException(
            PostgresErrorCodes.UniqueViolation,
            constraintName);

        Assert.Equal(
            expected,
            EndpointPostgresExceptionHelpers.IsRoomSlugConflict(exception));
    }

    [Theory]
    [InlineData("PK_friendships", true)]
    [InlineData("IX_rooms_slug", false)]
    [InlineData("PK_unrelated", false)]
    public void FriendshipConflict_Requires_Expected_Unique_Constraint(
        string constraintName,
        bool expected)
    {
        var exception = WrapPostgresException(
            PostgresErrorCodes.UniqueViolation,
            constraintName);

        Assert.Equal(
            expected,
            EndpointPostgresExceptionHelpers.IsFriendshipConflict(exception));
    }

    [Fact]
    public void Expected_Constraint_With_Unrelated_SqlState_Propagates()
    {
        var exception = WrapPostgresException(
            PostgresErrorCodes.ForeignKeyViolation,
            "IX_rooms_slug");

        Assert.False(EndpointPostgresExceptionHelpers.IsRoomSlugConflict(exception));
    }

    [Fact]
    public void NonPostgres_DbUpdateException_Propagates()
    {
        var exception = new DbUpdateException(
            "unrelated provider failure",
            new InvalidOperationException());

        Assert.False(EndpointPostgresExceptionHelpers.IsRoomSlugConflict(exception));
        Assert.False(EndpointPostgresExceptionHelpers.IsFriendshipConflict(exception));
    }

    private static DbUpdateException WrapPostgresException(
        string sqlState,
        string constraintName) =>
        new(
            "database update failed",
            new PostgresException(
                "constraint violation",
                "ERROR",
                "ERROR",
                sqlState,
                constraintName: constraintName));
}
