using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationListingService(PostgresOrganizationListingStore store)
{

    public Task<IReadOnlyList<OrganizationSummary>> ListForUserAsync(
        Guid userId,
        CancellationToken cancellationToken = default)
    {

        return store.ListForUserAsync(userId, cancellationToken);
    }
}
