using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Billing;

public sealed class BillingAccountListingService(
    PostgresBillingAccountListingStore store,
    TimeProvider timeProvider)
{

    public Task<IReadOnlyList<BillingAccountSummary>> ListForUserAsync(
        Guid userId,
        CancellationToken cancellationToken = default)
    {

        return store.ListForUserAsync(
            userId,
            timeProvider.GetUtcNow(),
            cancellationToken);
    }
}
