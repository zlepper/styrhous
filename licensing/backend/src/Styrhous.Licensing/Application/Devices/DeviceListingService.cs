using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Devices;

public sealed class DeviceListingService(PostgresDeviceListingStore store)
{

    public Task<DeviceListingResult> ListActiveAsync(
        Guid userId,
        Guid seatId,
        CancellationToken cancellationToken = default)
    {

        return store.ListActiveAsync(userId, seatId, cancellationToken);
    }
}
