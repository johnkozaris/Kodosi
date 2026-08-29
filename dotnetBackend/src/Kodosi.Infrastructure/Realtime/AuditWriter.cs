using System.Collections.Concurrent;
using System.Diagnostics.Metrics;
using System.Threading.Channels;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace Kodosi.Infrastructure.Realtime;

public sealed class AuditWriter : BackgroundService, IAuditWriter
{

    private static readonly Meter MetricsMeter = new("Kodosi.Host");
    private static readonly Counter<long> EnqueueTimeoutCounter = MetricsMeter.CreateCounter<long>(
        "Kodosi.audit.enqueue_timeouts",
        description: "AuditWriter.AppendAsync calls that hit the enqueue timeout (queue saturated).");
    private static readonly Histogram<int> FlushBatchHistogram = MetricsMeter.CreateHistogram<int>(
        "Kodosi.audit.flush_batch_size",
        description: "Per-flush batch size observed by the AuditWriter background loop.");

    private readonly Channel<AppendAuditCommand> _channel;
    private readonly IServiceScopeFactory _scopeFactory;
    private readonly ILogger<AuditWriter> _logger;
    private readonly TimeProvider _timeProvider;
    private readonly TimeSpan _shutdownDrainDeadline;
    private readonly CancellationTokenSource _forceStop = new();
    private readonly Meter _observableMeter = new("Kodosi.Host");
    private readonly ConcurrentDictionary<AppendAuditCommand, byte> _accepted =
        new(ReferenceEqualityComparer.Instance);
    private int _queueDepth;

    private const int MaxQueueSize = 4096;
    private const int BatchSize = 50;
    private static readonly TimeSpan EnqueueTimeout = TimeSpan.FromSeconds(1);
    private static readonly TimeSpan DefaultShutdownDrainDeadline = TimeSpan.FromSeconds(30);

    public AuditWriter(
        IServiceScopeFactory scopeFactory,
        ILogger<AuditWriter> logger,
        TimeProvider? timeProvider = null,
        TimeSpan? shutdownDrainDeadline = null)
    {
        _scopeFactory = scopeFactory;
        _logger = logger;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _shutdownDrainDeadline = shutdownDrainDeadline ?? DefaultShutdownDrainDeadline;
        if (_shutdownDrainDeadline <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(shutdownDrainDeadline));
        }
        _channel = Channel.CreateBounded<AppendAuditCommand>(new BoundedChannelOptions(MaxQueueSize)
        {
            FullMode = BoundedChannelFullMode.Wait,
            SingleReader = true,
        });
        _observableMeter.CreateObservableGauge(
            "Kodosi.audit.queue_depth",
            () => Volatile.Read(ref _queueDepth),
            description: "AuditWriter channel depth (pending-append count).");
    }

    public async Task<AuditAppendOutcome> AppendAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditKind kind,
        string payload,
        InputAuditStatus status,
        CancellationToken ct = default)
    {

        var sha256 = Convert.ToHexStringLower(
            System.Security.Cryptography.SHA256.HashData(
                System.Text.Encoding.UTF8.GetBytes(payload)));

        var completion = new TaskCompletionSource<AuditAppendOutcome>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var command = new AppendAuditCommand(
            sessionId,
            userId,
            clientCommandId,
            kind,
            sha256,
            System.Text.Encoding.UTF8.GetByteCount(payload),
            status,
            completion);

        if (!await EnqueueCommandAsync(command, ct))
        {
            return AuditAppendOutcome.Failed;
        }

        return await completion.Task.WaitAsync(ct);
    }

    public async Task<bool> RecordDuplicateAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditKind kind,
        string payload,
        CancellationToken ct = default)
    {
        if (await TryUpdateDuplicateAsync(sessionId, userId, clientCommandId, ct))
        {
            return true;
        }

        var appendOutcome = await AppendAsync(
            sessionId,
            userId,
            clientCommandId,
            kind,
            payload,
            InputAuditStatus.Duplicate,
            ct);
        if (appendOutcome == AuditAppendOutcome.Appended)
        {
            return true;
        }

        return appendOutcome == AuditAppendOutcome.AlreadyExists
            && await TryUpdateDuplicateAsync(sessionId, userId, clientCommandId, ct);
    }

    public async Task<bool> UpdateStatusAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditStatus status,
        CancellationToken ct = default)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repo = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
            var entry = await repo.GetByClientCommandAsync(
                sessionId,
                userId,
                clientCommandId,
                ct);
            if (entry is null)
            {
                _logger.LogWarning(
                    "Audit entry missing for status update {ActionId}",
                    clientCommandId);
                return false;
            }

            ApplyStatus(entry, status);
            await unitOfWork.SaveChangesAsync(ct);
            return true;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to persist audit status update {ActionId}", clientCommandId);
            return false;
        }
    }

    public async Task<bool> TryRearmAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditStatus initialStatus,
        CancellationToken ct = default)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
            var entry = await repository.GetByClientCommandAsync(
                sessionId,
                userId,
                clientCommandId,
                ct);
            if (entry is null || !entry.TryRearm(initialStatus))
            {
                return false;
            }

            await unitOfWork.SaveChangesAsync(ct);
            return true;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to rearm audit action {ActionId}", clientCommandId);
            return false;
        }
    }

    public async Task<InputAuditStatus?> GetStatusAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        CancellationToken ct = default)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            var entry = await repository.GetByClientCommandAsync(
                sessionId,
                userId,
                clientCommandId,
                ct);
            return entry?.Status;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to read audit status {ActionId}", clientCommandId);
            return null;
        }
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        var batch = new List<AppendAuditCommand>(BatchSize);
        using var stoppingRegistration = stoppingToken.Register(
            static state => ((ChannelWriter<AppendAuditCommand>)state!).TryComplete(),
            _channel.Writer);

        try
        {
            await foreach (var first in _channel.Reader.ReadAllAsync())
            {
                batch.Clear();
                batch.Add(first);
                Interlocked.Decrement(ref _queueDepth);

                while (batch.Count < BatchSize && _channel.Reader.TryRead(out var item))
                {
                    batch.Add(item);
                    Interlocked.Decrement(ref _queueDepth);
                }

                FlushBatchHistogram.Record(batch.Count);
                try
                {
                    await FlushBatchAsync(batch, _forceStop.Token);
                }
                catch (OperationCanceledException) when (_forceStop.IsCancellationRequested)
                {
                    CompleteBatch(batch, AuditAppendOutcome.Failed);
                    DrainAsFailures();
                    return;
                }
            }
        }
        catch (Exception ex)
        {
            _channel.Writer.TryComplete(ex);
            _logger.LogError(ex, "Audit writer terminated unexpectedly");
            CompleteBatch(batch, AuditAppendOutcome.Failed);
            DrainAsFailures();
        }
        finally
        {
            FailAllAccepted();
            Interlocked.Exchange(ref _queueDepth, 0);
        }
    }

    public override async Task StopAsync(CancellationToken cancellationToken)
    {
        _channel.Writer.TryComplete();
        var executeTask = ExecuteTask;
        if (executeTask is null)
        {
            FailAllAccepted();
            return;
        }

        try
        {
            await executeTask.WaitAsync(
                _shutdownDrainDeadline,
                _timeProvider,
                CancellationToken.None);
        }
        catch (TimeoutException)
        {
            _logger.LogCritical(
                "Audit shutdown drain exceeded {Deadline}; failing all unresolved appends",
                _shutdownDrainDeadline);
            await _forceStop.CancelAsync();
            FailAllAccepted();
            await executeTask;
        }
    }

    public override void Dispose()
    {
        _observableMeter.Dispose();
        _forceStop.Dispose();
        base.Dispose();
    }

    private async Task<bool> EnqueueCommandAsync(AppendAuditCommand command, CancellationToken ct)
    {
        using var enqueueCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        enqueueCts.CancelAfter(EnqueueTimeout);

        try
        {
            if (!_accepted.TryAdd(command, 0))
            {
                throw new InvalidOperationException("Audit command was already queued.");
            }
            Interlocked.Increment(ref _queueDepth);
            await _channel.Writer.WriteAsync(command, enqueueCts.Token);
            return true;
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            RemoveUnaccepted(command);
            EnqueueTimeoutCounter.Add(1);
            _logger.LogWarning(
                "Audit queue saturated; failed to enqueue command for {ActionId}",
                command.ClientCommandId);
            command.Completion.TrySetResult(AuditAppendOutcome.Failed);
            return false;
        }
        catch (OperationCanceledException)
        {
            RemoveUnaccepted(command);
            command.Completion.TrySetCanceled(ct);
            throw;
        }
        catch (ChannelClosedException)
        {
            RemoveUnaccepted(command);
            _logger.LogWarning(
                "Audit queue closed; failed to enqueue command for {ActionId}",
                command.ClientCommandId);
            command.Completion.TrySetResult(AuditAppendOutcome.Failed);
            return false;
        }
    }

    private async Task FlushBatchAsync(List<AppendAuditCommand> batch, CancellationToken ct)
    {
        var commandsByKey = GroupCommandsByKey(batch);
        IReadOnlySet<InputAuditLookupKey> existingKeys;
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repo = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();

            existingKeys = await repo.GetExistingClientCommandsAsync(commandsByKey.Keys, ct);

            var newEntries = new List<InputAuditEntry>(commandsByKey.Count - existingKeys.Count);
            foreach (var (key, commands) in commandsByKey)
            {
                if (existingKeys.Contains(key))
                {
                    continue;
                }

                newEntries.Add(CreateEntry(commands[0]));
            }

            if (newEntries.Count > 0)
            {
                await repo.AddRangeAsync(newEntries, ct);
                await unitOfWork.SaveChangesAsync(ct);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to persist audit append batch of {Count} items", batch.Count);
            await FlushBatchIndividuallyAsync(batch, ct);
            return;
        }

        foreach (var (key, commands) in commandsByKey)
        {
            var outcome = existingKeys.Contains(key)
                ? AuditAppendOutcome.AlreadyExists
                : AuditAppendOutcome.Appended;
            for (var index = 0; index < commands.Count; index++)
            {
                CompleteCommand(
                    commands[index],
                    index == 0 ? outcome : AuditAppendOutcome.AlreadyExists);
            }
        }
    }

    private async Task FlushBatchIndividuallyAsync(
        List<AppendAuditCommand> batch,
        CancellationToken ct)
    {
        foreach (var command in batch)
        {
            CompleteCommand(command, await PersistAppendAsync(command, ct));
        }
    }

    private void DrainAsFailures()
    {
        while (_channel.Reader.TryRead(out var command))
        {
            Interlocked.Decrement(ref _queueDepth);
            CompleteCommand(command, AuditAppendOutcome.Failed);
        }
    }

    private void CompleteBatch(
        IEnumerable<AppendAuditCommand> commands,
        AuditAppendOutcome outcome)
    {
        foreach (var command in commands)
        {
            CompleteCommand(command, outcome);
        }
    }

    private void CompleteCommand(AppendAuditCommand command, AuditAppendOutcome outcome)
    {
        _accepted.TryRemove(command, out _);
        command.Completion.TrySetResult(outcome);
    }

    private void RemoveUnaccepted(AppendAuditCommand command)
    {
        if (_accepted.TryRemove(command, out _))
        {
            Interlocked.Decrement(ref _queueDepth);
        }
    }

    private void FailAllAccepted()
    {
        foreach (var command in _accepted.Keys)
        {
            CompleteCommand(command, AuditAppendOutcome.Failed);
        }
    }

    private async Task<AuditAppendOutcome> PersistAppendAsync(
        AppendAuditCommand command,
        CancellationToken ct)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repo = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
            var existing = await repo.GetByClientCommandAsync(
                command.SessionId,
                command.UserId,
                command.ClientCommandId,
                ct);
            if (existing is not null)
            {
                return AuditAppendOutcome.AlreadyExists;
            }

            await repo.AddAsync(CreateEntry(command), ct);
            await unitOfWork.SaveChangesAsync(ct);
            return AuditAppendOutcome.Appended;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to persist audit append {ActionId}", command.ClientCommandId);
            return AuditAppendOutcome.Failed;
        }
    }

    private async Task<bool> TryUpdateDuplicateAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        CancellationToken ct)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<IInputAuditRepository>();
            return await repository.TryRecordDuplicateAsync(
                sessionId,
                userId,
                clientCommandId,
                _timeProvider.GetUtcNow(),
                ct);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to record duplicate audit action {ActionId}", clientCommandId);
            return false;
        }
    }

    private static Dictionary<InputAuditLookupKey, List<AppendAuditCommand>> GroupCommandsByKey(
        List<AppendAuditCommand> batch)
    {
        var commandsByKey = new Dictionary<InputAuditLookupKey, List<AppendAuditCommand>>();
        foreach (var command in batch)
        {
            var key = new InputAuditLookupKey(
                command.SessionId,
                command.UserId,
                command.ClientCommandId);
            if (!commandsByKey.TryGetValue(key, out var commands))
            {
                commands = new List<AppendAuditCommand>();
                commandsByKey.Add(key, commands);
            }

            commands.Add(command);
        }

        return commandsByKey;
    }

    private static InputAuditEntry CreateEntry(AppendAuditCommand command)
    {
        var entry = InputAuditEntry.Create(
            command.SessionId,
            command.UserId,
            command.ClientCommandId,
            command.Kind,
            command.Sha256,
            command.PayloadBytesLen);
        ApplyStatus(entry, command.Status);
        return entry;
    }

    private static void ApplyStatus(InputAuditEntry entry, InputAuditStatus status)
    {
        switch (status)
        {
            case InputAuditStatus.Rejected:
                entry.MarkRejected();
                break;
            case InputAuditStatus.Dispatched:
                entry.MarkDispatched();
                break;
            case InputAuditStatus.Failed:
                entry.MarkFailed();
                break;
            case InputAuditStatus.Duplicate:
                entry.MarkDuplicate();
                break;
        }
    }

    private sealed record AppendAuditCommand(
        SessionId SessionId,
        UserId UserId,
        string ClientCommandId,
        InputAuditKind Kind,
        string Sha256,
        int PayloadBytesLen,
        InputAuditStatus Status,
        TaskCompletionSource<AuditAppendOutcome> Completion)
        ;
}
