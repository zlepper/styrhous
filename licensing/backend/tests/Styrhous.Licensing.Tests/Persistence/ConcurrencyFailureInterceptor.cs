using System.Collections.Concurrent;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class ConcurrencyFailureInterceptor(
    Func<int, bool> shouldFail) : SaveChangesInterceptor
{
    private int _attemptCount;
    private readonly ConcurrentDictionary<Guid, byte> _contexts = new();

    public int ContextCount => _contexts.Count;

    public int AttemptCount => Volatile.Read(ref _attemptCount);

    public override ValueTask<InterceptionResult<int>> SavingChangesAsync(
        DbContextEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        _contexts.TryAdd(eventData.Context!.ContextId.InstanceId, 0);
        var attempt = Interlocked.Increment(ref _attemptCount);
        if (shouldFail(attempt))
        {
            throw new DbUpdateConcurrencyException(
                $"Injected optimistic concurrency failure on save attempt {attempt}.");
        }

        return base.SavingChangesAsync(eventData, result, cancellationToken);
    }
}
