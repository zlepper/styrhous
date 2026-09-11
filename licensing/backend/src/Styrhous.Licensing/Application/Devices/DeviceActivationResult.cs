using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Application.Devices;

public sealed record DeviceActivationResult(
    DeviceActivationStatus Status,
    string ReasonCode,
    Guid CorrelationId,
    Guid? ActivationId,
    Guid? RevokedActivationId,
    IReadOnlyList<ActiveDevice> ActiveDevices)
{
    internal static DeviceActivationResult Activated(
        Guid correlationId,
        DeviceActivation activation,
        DeviceActivation? revokedActivation,
        IReadOnlyList<ActiveDevice> activeDevices)
    {
        return new(
            DeviceActivationStatus.Activated,
            revokedActivation is null
                ? DeviceActivationReasonCodes.Activated
                : DeviceActivationReasonCodes.StaleDeviceReplaced,
            correlationId,
            activation.Id,
            revokedActivation?.Id,
            activeDevices);
    }

    internal static DeviceActivationResult AlreadyActive(
        Guid correlationId,
        DeviceActivation activation,
        IReadOnlyList<ActiveDevice> activeDevices)
    {
        return new(
            DeviceActivationStatus.AlreadyActive,
            DeviceActivationReasonCodes.AlreadyActive,
            correlationId,
            activation.Id,
            RevokedActivationId: null,
            activeDevices);
    }

    internal static DeviceActivationResult LimitReached(
        Guid correlationId,
        IReadOnlyList<ActiveDevice> activeDevices)
    {
        return new(
            DeviceActivationStatus.DeviceLimitReached,
            DeviceActivationReasonCodes.DeviceLimitReached,
            correlationId,
            ActivationId: null,
            RevokedActivationId: null,
            activeDevices);
    }

    internal static DeviceActivationResult SeatNotEligible(Guid correlationId)
    {
        return new(
            DeviceActivationStatus.SeatNotEligible,
            DeviceActivationReasonCodes.SeatNotEligible,
            correlationId,
            ActivationId: null,
            RevokedActivationId: null,
            Array.Empty<ActiveDevice>());
    }
}
