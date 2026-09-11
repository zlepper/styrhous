using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public sealed record ActiveDeviceResponse(
    Guid ActivationId,
    Guid InstallationId,
    string DisplayName,
    string Platform,
    string Architecture,
    string StyrhousVersion,
    DateTimeOffset ActivatedAt,
    DateTimeOffset LastSeenAt)
{
    internal static ActiveDeviceResponse From(ActiveDevice device)
    {
        return new(
            device.ActivationId,
            device.InstallationId,
            device.DisplayName,
            device.Platform,
            device.Architecture,
            device.StyrhousVersion,
            device.ActivatedAt,
            device.LastSeenAt);
    }
}
