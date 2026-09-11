using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Application.Devices;

public sealed record DeviceEntitlementCheckResult(
    DeviceEntitlementCheckStatus Status,
    string ReasonCode,
    SeatEntitlement? Entitlement,
    ActiveDevice? Device)
{
    internal static DeviceEntitlementCheckResult From(
        SeatEntitlement entitlement,
        DeviceActivation activation)
    {
        return new(
            entitlement.IsEligible
                ? DeviceEntitlementCheckStatus.Eligible
                : DeviceEntitlementCheckStatus.Ineligible,
            entitlement.ReasonCode,
            entitlement,
            ActiveDevice.From(activation));
    }

    internal static DeviceEntitlementCheckResult DeviceNotActive()
    {
        return new(
            DeviceEntitlementCheckStatus.DeviceNotActive,
            DeviceOperationReasonCodes.DeviceNotActive,
            Entitlement: null,
            Device: null);
    }
}
