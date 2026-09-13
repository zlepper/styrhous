using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresCommercialSubscriptionProjectionStore(
    LicensingDbContext dbContext)

{

    public async Task<CommercialSubscriptionProjectionResult> ApplyAsync(
        AuthoritativeCommercialSubscription subscription,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(subscription);
        return await LicensingDbContextTransaction.ExecuteAsync<
            CommercialSubscriptionProjectionResult>(
            dbContext,
            async (transaction, token) =>
        {
            if (!await EfTransactionSerialization.TryClaimBillingAccountAsync(
                    dbContext,
                    subscription.BillingAccountId,
                    cancellationToken))
            {
                return new CommercialSubscriptionProjectionResult(
                    CommercialSubscriptionProjectionStatus.BillingAccountNotFound,
                    SubscriptionId: null);
            }

            var result = await PostgresCommercialSubscriptionProjection.ApplyClaimedAsync(
                dbContext,
                subscription.BillingAccountId,
                subscription.Projection,
                subscription.ProviderReadRevision,
                subscription.ProviderSnapshotKind,
                cancellationToken,
                subscription.BillingOperationId);
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return result;
        },
            cancellationToken);
    }

}
