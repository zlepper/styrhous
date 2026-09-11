using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingProviderReadRevisionSource(
    IDbContextFactory<LicensingDbContext> dbContextFactory)

{

    public async Task<long> ReserveAsync(CancellationToken cancellationToken)
    {
        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            await using var dbContext = await dbContextFactory.CreateDbContextAsync(
                cancellationToken);
            try
            {
                return await LicensingDbContextTransaction.ExecuteAsync<long>(
                    dbContext,
                    async (transaction, token) =>
                {
                    var cursor = await dbContext.BillingProviderReadCursors.SingleAsync(
                        candidate => candidate.Id == BillingProviderReadCursor.SingletonId,
                        token);
                    var revision = cursor.Advance();
                    await dbContext.SaveChangesAsync(token);
                    await transaction.CommitAsync(token);
                    return revision;
                },
                    cancellationToken);
            }
            catch (DbUpdateConcurrencyException)
            {
            }
        }
    }
}
