using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Application.Desktop;

public enum DesktopDeviceAuthorizationDecisionStatus
{
    Approved,
    SeatNotEligible,
    DeviceLimitReached,
}

public sealed record DesktopDeviceAuthorizationDecisionResult(
    DesktopDeviceAuthorizationDecisionStatus Status,
    string ReasonCode,
    Guid CorrelationId,
    Guid? ActivationId,
    Guid? RevokedActivationId,
    IReadOnlyList<ActiveDevice> ActiveDevices)
{
    internal static DesktopDeviceAuthorizationDecisionResult From(
        DeviceActivationResult activation)
    {
        return activation.Status switch
        {
            DeviceActivationStatus.Activated or DeviceActivationStatus.AlreadyActive =>
                new DesktopDeviceAuthorizationDecisionResult(
                    DesktopDeviceAuthorizationDecisionStatus.Approved,
                    DesktopDeviceAuthorizationReasonCodes.Approved,
                    activation.CorrelationId,
                    activation.ActivationId,
                    activation.RevokedActivationId,
                    activation.ActiveDevices),
            DeviceActivationStatus.DeviceLimitReached =>
                new DesktopDeviceAuthorizationDecisionResult(
                    DesktopDeviceAuthorizationDecisionStatus.DeviceLimitReached,
                    DesktopDeviceAuthorizationReasonCodes.DeviceLimitReached,
                    activation.CorrelationId,
                    ActivationId: null,
                    RevokedActivationId: null,
                    activation.ActiveDevices),
            DeviceActivationStatus.SeatNotEligible =>
                new DesktopDeviceAuthorizationDecisionResult(
                    DesktopDeviceAuthorizationDecisionStatus.SeatNotEligible,
                    DesktopDeviceAuthorizationReasonCodes.SeatNotEligible,
                    activation.CorrelationId,
                    ActivationId: null,
                    RevokedActivationId: null,
                    Array.Empty<ActiveDevice>()),
            _ => throw new ArgumentOutOfRangeException(
                nameof(activation),
                activation.Status,
                "The device activation status is not supported."),
        };
    }
}
