using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Devices;

public sealed class DeviceRevocationService(
    PostgresDeviceRevocationStore store,
    TimeProvider timeProvider)
{

    public Task<DeviceRevocationResult> RevokeAsync(
        Guid userId,
        Guid activationId,
        CancellationToken cancellationToken = default)
    {

        return store.RevokeAsync(
            userId,
            activationId,
            timeProvider.GetUtcNow(),
            Uuid7.Create(),
            cancellationToken);
    }
}
