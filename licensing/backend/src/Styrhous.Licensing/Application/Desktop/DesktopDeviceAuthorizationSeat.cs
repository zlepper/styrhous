using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Application.Desktop;

public sealed record DesktopDeviceAuthorizationSeat(
    Guid SeatId,
    Guid BillingAccountId,
    string Name,
    EntitlementState EntitlementState,
    string EntitlementReasonCode,
    int DeviceLimit,
    bool CanActivate,
    IReadOnlyList<ActiveDevice> ActiveDevices);
