using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public sealed record DeviceListResponse(
    string ReasonCode,
    Guid SeatId,
    int DeviceLimit,
    IReadOnlyList<ActiveDeviceResponse> ActiveDevices)
{
    internal static DeviceListResponse From(DeviceListingResult result)
    {
        if (result.Status != DeviceListingStatus.Listed
            || result.SeatId is null
            || result.DeviceLimit is null)
        {
            throw new ArgumentException(
                "A successful device listing result is required.",
                nameof(result));
        }

        return new DeviceListResponse(
            result.ReasonCode,
            result.SeatId.Value,
            result.DeviceLimit.Value,
            result.ActiveDevices.Select(ActiveDeviceResponse.From).ToArray());
    }
}
