using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresEntitlementStore(LicensingDbContext dbContext)
{

    public async Task<IReadOnlyList<SeatEntitlementSource>> ListForUserAsync(
        Guid userId,
        CancellationToken cancellationToken)
    {
        return await dbContext.Database.CreateExecutionStrategy().ExecuteAsync(async () =>
        {
            await using var transaction = await dbContext.Database.BeginTransactionAsync(
                IsolationLevel.RepeatableRead,
                cancellationToken);
            await PostgresReadChecks.EnsureUserExistsAsync(
                dbContext,
                userId,
                cancellationToken);

            var sources = (await SeatEntitlementQuery.LoadForUserAsync(
                    dbContext,
                    userId,
                    cancellationToken))
                .Select(item => item.Source)
                .ToArray();
            await transaction.CommitAsync(cancellationToken);
            return (IReadOnlyList<SeatEntitlementSource>)sources;
        });
    }
}
