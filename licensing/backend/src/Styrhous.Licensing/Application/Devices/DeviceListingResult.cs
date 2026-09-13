using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Application.Devices;

public sealed record DeviceListingResult(
    DeviceListingStatus Status,
    string ReasonCode,
    Guid? SeatId,
    int? DeviceLimit,
    IReadOnlyList<ActiveDevice> ActiveDevices)
{
    internal static DeviceListingResult Listed(
        Seat seat,
        IEnumerable<DeviceActivation> activeDevices)
    {
        return new(
            DeviceListingStatus.Listed,
            DeviceListingReasonCodes.Listed,
            seat.Id,
            seat.DeviceLimit,
            ActiveDevice.Summarize(activeDevices));
    }

    internal static DeviceListingResult SeatNotFound()
    {
        return new(
            DeviceListingStatus.SeatNotFound,
            DeviceListingReasonCodes.SeatNotFound,
            SeatId: null,
            DeviceLimit: null,
            Array.Empty<ActiveDevice>());
    }
}
