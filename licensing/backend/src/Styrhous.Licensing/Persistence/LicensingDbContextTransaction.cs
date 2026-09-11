using System.Data;
using Microsoft.EntityFrameworkCore;

namespace Styrhous.Licensing.Persistence;

internal static class LicensingDbContextTransaction
{
    public static async Task<T> ExecuteAsync<T>(
        IDbContextFactory<LicensingDbContext> contextFactory,
        IsolationLevel isolationLevel,
        Func<LicensingDbContext, CancellationToken, Task<T>> operation,
        CancellationToken cancellationToken)
    {
        await using var strategyContext = await contextFactory.CreateDbContextAsync(
            cancellationToken);
        var strategy = strategyContext.Database.CreateExecutionStrategy();
        return await strategy.ExecuteAsync(async () =>
        {
            await using var operationContext = await contextFactory.CreateDbContextAsync(
                cancellationToken);
            await using var transaction = await operationContext.Database
                .BeginTransactionAsync(isolationLevel, cancellationToken);
            var result = await operation(operationContext, cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return result;
        });
    }
}
