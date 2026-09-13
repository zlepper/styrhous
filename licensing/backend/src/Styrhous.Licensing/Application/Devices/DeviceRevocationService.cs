using Styrhous.Licensing.Persistence;

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
            Guid.CreateVersion7(),
            cancellationToken);
    }
}
