using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddIdentityLifecycleRevision : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid>(
                name: "identity_incarnation_id",
                table: "users",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "identity_revision",
                table: "users",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<long>(
                name: "identity_revision",
                table: "identity_reset_audit",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.CreateTable(
                name: "identity_exposures",
                columns: table => new
                {
                    identity_owner_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    recipient_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    first_exposed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    last_exposed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_identity_exposures", x => new { x.identity_owner_user_id, x.recipient_user_id });
                });

            migrationBuilder.CreateIndex(
                name: "IX_identity_exposures_recipient_user_id",
                table: "identity_exposures",
                column: "recipient_user_id");

            migrationBuilder.Sql(
                """
                WITH ordered_resets AS (
                    SELECT
                        id,
                        ROW_NUMBER() OVER (
                            PARTITION BY user_id
                            ORDER BY reset_at, id
                        ) AS revision
                    FROM identity_reset_audit
                )
                UPDATE identity_reset_audit AS reset
                SET identity_revision = ordered.revision
                FROM ordered_resets AS ordered
                WHERE reset.id = ordered.id;

                WITH reset_revisions AS (
                    SELECT user_id, COALESCE(MAX(identity_revision), 0) AS revision
                    FROM identity_reset_audit
                    GROUP BY user_id
                ),
                enrolled_user_ids AS (
                    SELECT DISTINCT user_id
                    FROM user_device_lists
                ),
                enrolled_users AS (
                    SELECT
                        user_id,
                        LPAD(
                            TO_HEX((EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint),
                            12,
                            '0') AS timestamp_hex,
                        REPLACE(gen_random_uuid()::text, '-', '') AS random_hex
                    FROM enrolled_user_ids
                )
                UPDATE users AS target
                SET
                    identity_revision = COALESCE(reset_revisions.revision, 0) + 1,
                    identity_incarnation_id = (
                        SUBSTRING(enrolled.timestamp_hex FROM 1 FOR 8) || '-' ||
                        SUBSTRING(enrolled.timestamp_hex FROM 9 FOR 4) || '-' ||
                        '7' || SUBSTRING(enrolled.random_hex FROM 1 FOR 3) || '-' ||
                        '8' || SUBSTRING(enrolled.random_hex FROM 4 FOR 3) || '-' ||
                        SUBSTRING(enrolled.random_hex FROM 7 FOR 12)
                    )::uuid
                FROM enrolled_users AS enrolled
                LEFT JOIN reset_revisions
                    ON reset_revisions.user_id = enrolled.user_id
                WHERE target.id = enrolled.user_id;

                UPDATE users AS target
                SET identity_revision = reset_revisions.revision
                FROM (
                    SELECT user_id, MAX(identity_revision) AS revision
                    FROM identity_reset_audit
                    GROUP BY user_id
                ) AS reset_revisions
                WHERE target.id = reset_revisions.user_id
                  AND NOT EXISTS (
                      SELECT 1
                      FROM user_device_lists AS list
                      WHERE list.user_id = target.id
                  );

                INSERT INTO identity_exposures (
                    identity_owner_user_id,
                    recipient_user_id,
                    first_exposed_at,
                    last_exposed_at
                )
                SELECT DISTINCT ON (owner_id, recipient_id)
                    owner_id,
                    recipient_id,
                    exposed_at,
                    exposed_at
                FROM (
                    SELECT
                        friendship.actor_user_id AS owner_id,
                        friendship.other_user_id AS recipient_id,
                        friendship.occurred_at AS exposed_at
                    FROM friendship_audit AS friendship
                    WHERE friendship.action = 'RequestAccepted'

                    UNION ALL

                    SELECT
                        friendship.other_user_id,
                        friendship.actor_user_id,
                        friendship.occurred_at
                    FROM friendship_audit AS friendship
                    WHERE friendship.action = 'RequestAccepted'

                    UNION ALL

                    SELECT
                        mine.user_id,
                        peer.user_id,
                        GREATEST(mine.created_at, peer.created_at)
                    FROM room_members AS mine
                    INNER JOIN room_members AS peer
                        ON peer.room_id = mine.room_id
                       AND peer.user_id <> mine.user_id
                       AND (mine.revoked_at IS NULL OR peer.created_at <= mine.revoked_at)
                       AND (peer.revoked_at IS NULL OR mine.created_at <= peer.revoked_at)

                    UNION ALL

                    SELECT
                        session.owner_user_id,
                        access_override.actor_user_id,
                        access_override.created_at
                    FROM session_access_overrides AS access_override
                    INNER JOIN sessions AS session
                        ON session.id = access_override.session_id
                    WHERE session.owner_user_id <> access_override.actor_user_id

                    UNION ALL

                    SELECT
                        access_override.actor_user_id,
                        session.owner_user_id,
                        access_override.created_at
                    FROM session_access_overrides AS access_override
                    INNER JOIN sessions AS session
                        ON session.id = access_override.session_id
                    WHERE session.owner_user_id <> access_override.actor_user_id
                ) AS historical(owner_id, recipient_id, exposed_at)
                WHERE owner_id <> recipient_id
                ORDER BY owner_id, recipient_id, exposed_at
                ON CONFLICT (identity_owner_user_id, recipient_user_id) DO NOTHING;
                """);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "identity_exposures");

            migrationBuilder.DropColumn(
                name: "identity_incarnation_id",
                table: "users");

            migrationBuilder.DropColumn(
                name: "identity_revision",
                table: "users");

            migrationBuilder.DropColumn(
                name: "identity_revision",
                table: "identity_reset_audit");
        }
    }
}
