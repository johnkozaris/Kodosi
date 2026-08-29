using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomRosterTransitionRepository(KodosiDbContext context)
    : IRoomRosterTransitionRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(RoomRosterTransition transition, CancellationToken ct = default)
        => await _context.RoomRosterTransitions.AddAsync(transition, ct);

    public async Task<IReadOnlyList<RoomRosterTransition>> GetPageAfterAsync(
        RoomId roomId,
        long afterGeneration,
        int limit,
        CancellationToken ct = default)
    {
        return await _context.RoomRosterTransitions
            .AsNoTracking()
            .Where(transition =>
                transition.RoomId == roomId
                && transition.Generation > afterGeneration)
            .OrderBy(transition => transition.Generation)
            .Take(limit)
            .ToListAsync(ct);
    }
}
