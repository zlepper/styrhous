using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public sealed record DeviceRevocationErrorResponse(string ReasonCode)
{
    internal static DeviceRevocationErrorResponse DeviceNotActive()
    {
        return new(DeviceOperationReasonCodes.DeviceNotActive);
    }
}
