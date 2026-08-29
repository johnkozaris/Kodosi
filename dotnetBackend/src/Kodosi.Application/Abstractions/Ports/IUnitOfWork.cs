namespace Kodosi.Application;

public interface IUnitOfWork
{
    Task SaveChangesAsync(CancellationToken ct = default);

    Task<ITransactionScope> BeginTransactionAsync(CancellationToken ct = default);
}

public interface ITransactionScope : IAsyncDisposable
{
    Task CommitAsync(CancellationToken ct = default);
    Task CreateSavepointAsync(string name, CancellationToken ct = default);
    Task RollbackToSavepointAsync(string name, CancellationToken ct = default);
}
