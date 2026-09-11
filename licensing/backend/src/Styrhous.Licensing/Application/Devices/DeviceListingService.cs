using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

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
