using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public sealed class CommercialSubscriptionProjectionService(
    PostgresCommercialSubscriptionProjectionStore store)
{

    public Task<CommercialSubscriptionProjectionResult> ApplyAsync(
        Guid billingAccountId,
        CommercialSubscriptionProjection projection,
        CancellationToken cancellationToken = default)
    {

        ArgumentNullException.ThrowIfNull(projection);
        return store.ApplyAsync(
            new AuthoritativeCommercialSubscription(billingAccountId, projection),
            cancellationToken);
    }

    public Task<CommercialSubscriptionProjectionResult> ApplyAsync(
        AuthoritativeCommercialSubscription subscription,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(subscription);

        return store.ApplyAsync(subscription, cancellationToken);
    }
}
