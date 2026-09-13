using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public sealed record DeviceRevocationResponse(
    string ReasonCode,
    Guid ActivationId,
    Guid CorrelationId)
{
    internal static DeviceRevocationResponse From(DeviceRevocationResult result)
    {
        if (result.Status != DeviceRevocationStatus.Revoked
            || result.ActivationId is null)
        {
            throw new ArgumentException(
                "A successful device revocation result is required.",
                nameof(result));
        }

        return new DeviceRevocationResponse(
            result.ReasonCode,
            result.ActivationId.Value,
            result.CorrelationId);
    }
}
