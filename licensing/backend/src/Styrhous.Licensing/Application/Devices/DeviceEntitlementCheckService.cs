using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Devices;

public sealed class DeviceEntitlementCheckService(
    PostgresDeviceEntitlementCheckStore store,
    TimeProvider timeProvider)
{

    public Task<DeviceEntitlementCheckResult> CheckAsync(
        Guid userId,
        Guid activationId,
        CancellationToken cancellationToken = default)
    {
        return CheckAtAsync(
            userId,
            activationId,
            timeProvider.GetUtcNow(),
            cancellationToken);
    }

    public Task<DeviceEntitlementCheckResult> CheckAtAsync(
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken = default)
    {

        return store.CheckAsync(
            userId,
            activationId,
            observedAt,
            cancellationToken);
    }
}
