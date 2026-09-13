using System.Data;
using System.Runtime.ExceptionServices;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Api.Desktop;

public sealed class DesktopProtocolTransaction(LicensingDbContext dbContext)
{
    private IDbContextTransaction? _transaction;

    public bool HasStarted => _transaction is not null;
    public bool IsRetryableFailure { get; private set; }

    public void MarkRetryableFailure()
    {
        IsRetryableFailure = true;
    }

    public async Task ExecuteAsync(
        Func<Task> operation,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(operation);

        // Run the explicit transaction through the configured Npgsql strategy.
        // A one-time desktop credential has protocol-specific conflict recovery,
        // so surface serialization failures after the transaction is rolled back
        // instead of replaying an OpenIddict request automatically.
        Exception? concurrencyFailure = null;
        await dbContext.Database.CreateExecutionStrategy().ExecuteAsync(async () =>
        {
            IsRetryableFailure = false;
            await using var transaction = await dbContext.Database.BeginTransactionAsync(
                IsolationLevel.Serializable,
                cancellationToken);
            _transaction = transaction;
            try
            {
                await operation();
            }
            catch (Exception exception) when (EfConcurrencyFailure.IsRetryable(exception))
            {
                concurrencyFailure = exception;
            }
            finally
            {
                _transaction = null;
            }
        });

        if (concurrencyFailure is not null)
        {
            ExceptionDispatchInfo.Capture(concurrencyFailure).Throw();
        }
    }

    public async Task EnsureStartedAsync(CancellationToken cancellationToken)
    {
        _transaction ??= await dbContext.Database.BeginTransactionAsync(
            IsolationLevel.Serializable,
            cancellationToken);
    }

    public Task CommitAsync(CancellationToken cancellationToken)
    {
        return _transaction?.CommitAsync(cancellationToken) ?? Task.CompletedTask;
    }

    public Task RollbackAsync(CancellationToken cancellationToken)
    {
        return _transaction?.RollbackAsync(cancellationToken) ?? Task.CompletedTask;
    }

    public async ValueTask DisposeAsync()
    {
        if (_transaction is not null)
        {
            await _transaction.DisposeAsync();
        }
    }
}
