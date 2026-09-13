using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationCreationService(
    PostgresOrganizationStore store,
    TimeProvider timeProvider)
{

    public Task<OrganizationCreationResult> CreateAsync(
        Guid ownerUserId,
        string name,
        CancellationToken cancellationToken = default)
    {

        var observedAt = timeProvider.GetUtcNow();
        var registration = OrganizationRegistration.Start(ownerUserId, name, observedAt);
        return store.AddAsync(registration, Guid.CreateVersion7(), cancellationToken);
    }
}
