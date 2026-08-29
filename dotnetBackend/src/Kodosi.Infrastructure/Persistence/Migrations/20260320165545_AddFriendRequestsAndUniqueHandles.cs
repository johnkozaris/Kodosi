using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddFriendRequestsAndUniqueHandles : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_friendships_user_high_id",
                table: "friendships");

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "accepted_at",
                table: "friendships",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "requestor_user_id",
                table: "friendships",
                type: "uuid",
                nullable: true);

            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM users
                        GROUP BY lower(btrim(handle))
                        HAVING COUNT(*) > 1
                    ) THEN
                        RAISE EXCEPTION 'Cannot enforce unique lowercase usernames because duplicate handles already exist after normalization.';
                    END IF;
                END
                $$;
                """);

            migrationBuilder.Sql(
                """
                UPDATE users
                SET handle = lower(btrim(handle));
                """);

            migrationBuilder.Sql(
                """
                UPDATE friendships
                SET
                    status = CASE
                        WHEN status = 'Active' THEN 'Accepted'
                        ELSE status
                    END,
                    requestor_user_id = user_low_id,
                    accepted_at = CASE
                        WHEN status = 'Active' THEN created_at
                        ELSE accepted_at
                    END;
                """);

            migrationBuilder.AlterColumn<Guid>(
                name: "requestor_user_id",
                table: "friendships",
                type: "uuid",
                nullable: false,
                oldClrType: typeof(Guid),
                oldType: "uuid",
                oldNullable: true);

            migrationBuilder.CreateIndex(
                name: "IX_users_handle",
                table: "users",
                column: "handle",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_friendships_status_requestor_user_id",
                table: "friendships",
                columns: new[] { "status", "requestor_user_id" });

            migrationBuilder.CreateIndex(
                name: "IX_friendships_status_user_high_id",
                table: "friendships",
                columns: new[] { "status", "user_high_id" });

            migrationBuilder.CreateIndex(
                name: "IX_friendships_status_user_low_id",
                table: "friendships",
                columns: new[] { "status", "user_low_id" });
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DELETE FROM friendships
                WHERE status = 'Pending';

                UPDATE friendships
                SET status = 'Active'
                WHERE status = 'Accepted';
                """);

            migrationBuilder.DropIndex(
                name: "IX_users_handle",
                table: "users");

            migrationBuilder.DropIndex(
                name: "IX_friendships_status_requestor_user_id",
                table: "friendships");

            migrationBuilder.DropIndex(
                name: "IX_friendships_status_user_high_id",
                table: "friendships");

            migrationBuilder.DropIndex(
                name: "IX_friendships_status_user_low_id",
                table: "friendships");

            migrationBuilder.DropColumn(
                name: "accepted_at",
                table: "friendships");

            migrationBuilder.DropColumn(
                name: "requestor_user_id",
                table: "friendships");

            migrationBuilder.CreateIndex(
                name: "IX_friendships_user_high_id",
                table: "friendships",
                column: "user_high_id");
        }
    }
}
