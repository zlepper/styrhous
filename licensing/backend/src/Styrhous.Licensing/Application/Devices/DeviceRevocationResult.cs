namespace Styrhous.Licensing.Application.Devices;

public sealed record DeviceRevocationResult(
    DeviceRevocationStatus Status,
    string ReasonCode,
    Guid CorrelationId,
    Guid? ActivationId)
{
    internal static DeviceRevocationResult Revoked(
        Guid correlationId,
        Guid activationId)
    {
        return new(
            DeviceRevocationStatus.Revoked,
            DeviceRevocationReasonCodes.Revoked,
            correlationId,
            activationId);
    }

    internal static DeviceRevocationResult DeviceNotActive(Guid correlationId)
    {
        return new(
            DeviceRevocationStatus.DeviceNotActive,
            DeviceOperationReasonCodes.DeviceNotActive,
            correlationId,
            ActivationId: null);
    }
}
