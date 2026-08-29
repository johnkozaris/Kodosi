using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class UserRepository(KodosiDbContext context) : IUserRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
        => await _context.Users.FirstOrDefaultAsync(u => u.Id == id, ct);

    public async Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default)
    {
        var normalizedHandle = handle.Trim().ToLowerInvariant();
        return await _context.Users.FirstOrDefaultAsync(u => u.Handle == normalizedHandle, ct);
    }

    public async Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default)
    {
        var normalizedPrefix = prefix.Trim().ToLowerInvariant();
        var prefixedStem = normalizedPrefix + "-";
        return await _context.Users
            .Where(user => user.Handle == normalizedPrefix || user.Handle.StartsWith(prefixedStem))
            .Select(user => user.Handle)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default)
        => await _context.Users.Where(u => ids.Contains(u.Id)).ToListAsync(ct);

    public async Task AddAsync(User user, CancellationToken ct = default)
        => await _context.Users.AddAsync(user, ct);
}
