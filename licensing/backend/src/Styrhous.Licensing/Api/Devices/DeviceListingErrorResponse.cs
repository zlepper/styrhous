using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public sealed record DeviceListingErrorResponse(string ReasonCode)
{
    internal static DeviceListingErrorResponse SeatNotFound()
    {
        return new(DeviceListingReasonCodes.SeatNotFound);
    }
}
