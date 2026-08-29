using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class RepairCurrentKeyGeneration : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                UPDATE sessions AS session
                SET current_key_generation = latest.max_generation
                FROM (
                    SELECT session_id, MAX(key_generation) AS max_generation
                    FROM session_key_blobs
                    GROUP BY session_id
                ) AS latest
                WHERE session.id = latest.session_id
                  AND session.current_key_generation < latest.max_generation;
                """);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
        }
    }
}
