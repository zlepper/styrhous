using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

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
