using System.Data;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Npgsql;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionIncarnationRepository(KodosiDbContext context)
    : ISessionIncarnationRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<SessionIncarnationRecord?> GetByIdempotencyKeyAsync(
        SessionId sessionId,
        Guid idempotencyKey,
        CancellationToken ct = default)
    {
        await using var command = CreateCommand(
            """
            SELECT generation, incarnation_id, protocol_version, created_at
            FROM session_incarnations
            WHERE session_id = @session_id
              AND idempotency_key = @idempotency_key
            """);
        command.Parameters.AddWithValue("session_id", sessionId.Value);
        command.Parameters.AddWithValue("idempotency_key", idempotencyKey);
        await using var reader = await command.ExecuteReaderAsync(
            CommandBehavior.SingleRow,
            ct);
        if (!await reader.ReadAsync(ct))
        {
            return null;
        }

        return new SessionIncarnationRecord(
            sessionId,
            reader.GetInt64(0),
            reader.GetGuid(1),
            reader.GetInt32(2),
            idempotencyKey,
            reader.GetFieldValue<DateTimeOffset>(3));
    }

    public async Task AddAsync(
        SessionIncarnationRecord incarnation,
        CancellationToken ct = default)
    {
        await using var command = CreateCommand(
            """
            INSERT INTO session_incarnations (
                session_id,
                generation,
                incarnation_id,
                protocol_version,
                idempotency_key,
                created_at
            ) VALUES (
                @session_id,
                @generation,
                @incarnation_id,
                @protocol_version,
                @idempotency_key,
                @created_at
            )
            """);
        command.Parameters.AddWithValue("session_id", incarnation.SessionId.Value);
        command.Parameters.AddWithValue("generation", incarnation.Generation);
        command.Parameters.AddWithValue("incarnation_id", incarnation.IncarnationId);
        command.Parameters.AddWithValue("protocol_version", incarnation.ProtocolVersion);
        command.Parameters.AddWithValue("idempotency_key", incarnation.IdempotencyKey);
        command.Parameters.AddWithValue("created_at", incarnation.CreatedAt);
        _ = await command.ExecuteNonQueryAsync(ct);
    }

    private NpgsqlCommand CreateCommand(string sql)
    {
        var transaction = _context.Database.CurrentTransaction
            ?? throw new InvalidOperationException(
                "Session incarnation history requires an active database transaction.");
        var connection = (NpgsqlConnection)_context.Database.GetDbConnection();
        if (connection.State != ConnectionState.Open)
        {
            throw new InvalidOperationException(
                "Session incarnation history requires the transaction connection to be open.");
        }

        return new NpgsqlCommand(
            sql,
            connection,
            (NpgsqlTransaction)transaction.GetDbTransaction());
    }
}
