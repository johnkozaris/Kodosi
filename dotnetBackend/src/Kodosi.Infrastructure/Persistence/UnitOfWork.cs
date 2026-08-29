using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Npgsql;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence;

public sealed class UnitOfWork(KodosiDbContext context) : IUnitOfWork
{
    private readonly KodosiDbContext _context = context;

    public async Task SaveChangesAsync(CancellationToken ct = default)
    {
        try
        {
            await _context.SaveChangesAsync(ct);
        }
        catch (DbUpdateException ex) when (IsHandleUniquenessViolation(ex))
        {
            throw new HandleConcurrentlyTakenException(ex);
        }
        catch (DbUpdateException ex) when (IsUserDeviceUniqueViolation(ex))
        {
            throw new DeviceAlreadyEnrolledException(ex);
        }
        catch (DbUpdateException ex) when (IsDeviceListGenerationCollision(ex))
        {
            throw new DeviceListGenerationCollisionException(ex);
        }
        catch (DbUpdateException ex) when (IsDeviceLinkUserCodeCollision(ex))
        {
            foreach (var entry in ex.Entries.Where(entry => entry.Entity is DeviceLinkRequest))
            {
                entry.State = EntityState.Detached;
            }
            throw new DeviceLinkUserCodeCollisionException(ex);
        }
        catch (DbUpdateException ex) when (IsRoomInvitationUniquenessViolation(ex))
        {
            throw new ConflictException(
                "The invitation ID is already in use or another pending invitation exists.",
                ex);
        }
        catch (DbUpdateConcurrencyException ex)
        {
            throw new ConcurrentModificationException(ex);
        }
    }

    public async Task<ITransactionScope> BeginTransactionAsync(CancellationToken ct = default)
    {
        var transaction = await _context.Database.BeginTransactionAsync(ct);
        return new EfTransactionScope(transaction);
    }

    private static bool IsHandleUniquenessViolation(DbUpdateException exception)
    {


        if (exception.InnerException is not PostgresException pg)
        {
            return false;
        }
        if (pg.SqlState != "23505")
        {
            return false;
        }
        return pg.ConstraintName?.Contains("handle", StringComparison.OrdinalIgnoreCase) == true;
    }

    private static bool IsDeviceListGenerationCollision(DbUpdateException exception) =>
        exception.InnerException is PostgresException
        {
            SqlState: "23505",
            ConstraintName: "PK_user_device_lists",
        };

    private static bool IsUserDeviceUniqueViolation(DbUpdateException exception) =>
        exception.InnerException is PostgresException { SqlState: "23505" } pg
        && pg.ConstraintName is "PK_user_devices" or "IX_user_devices_device_id";

    private static bool IsDeviceLinkUserCodeCollision(DbUpdateException exception) =>
        exception.InnerException is PostgresException
        {
            SqlState: "23505",
            ConstraintName: "IX_device_link_requests_user_code",
        };

    private static bool IsRoomInvitationUniquenessViolation(DbUpdateException exception)
    {
        if (exception.InnerException is not PostgresException { SqlState: "23505" } pg)
        {
            return false;
        }

        return pg.ConstraintName is "PK_room_invitations"
            or "UX_room_invitations_pending_room_invitee";
    }

    private sealed class EfTransactionScope(IDbContextTransaction transaction) : ITransactionScope
    {
        private readonly IDbContextTransaction _transaction = transaction;

        public Task CommitAsync(CancellationToken ct = default) => _transaction.CommitAsync(ct);

        public Task CreateSavepointAsync(string name, CancellationToken ct = default)
            => _transaction.CreateSavepointAsync(name, ct);

        public Task RollbackToSavepointAsync(string name, CancellationToken ct = default)
            => _transaction.RollbackToSavepointAsync(name, ct);

        public ValueTask DisposeAsync() => _transaction.DisposeAsync();
    }
}
