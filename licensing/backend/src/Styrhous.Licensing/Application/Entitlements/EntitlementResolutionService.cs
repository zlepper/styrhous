using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Entitlements;

public sealed class EntitlementResolutionService(
    PostgresEntitlementStore store,
    TimeProvider timeProvider)
{

    public async Task<IReadOnlyList<SeatEntitlement>> ListForUserAsync(
        Guid userId,
        CancellationToken cancellationToken = default)
    {

        var sources = await store.ListForUserAsync(userId, cancellationToken);
        var observedAt = timeProvider.GetUtcNow();
        return sources
            .Select(source => SeatEntitlement.Resolve(source, observedAt))
            .ToArray();
    }
}
