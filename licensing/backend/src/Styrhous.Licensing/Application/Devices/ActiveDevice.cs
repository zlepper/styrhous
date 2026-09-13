using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Application.Devices;

public sealed record ActiveDevice(
    Guid ActivationId,
    Guid InstallationId,
    string DisplayName,
    string Platform,
    string Architecture,
    string StyrhousVersion,
    DateTimeOffset ActivatedAt,
    DateTimeOffset LastSeenAt)
{
    internal static ActiveDevice From(DeviceActivation activation)
    {
        return new(
            activation.Id,
            activation.InstallationId,
            activation.DisplayName,
            activation.Platform,
            activation.Architecture,
            activation.StyrhousVersion,
            activation.ActivatedAt,
            activation.LastSeenAt);
    }

    internal static ActiveDevice[] Summarize(
        IEnumerable<DeviceActivation> activations)
    {
        return activations
            .Where(activation => activation.RevokedAt is null)
            .OrderByDescending(activation => activation.LastSeenAt)
            .ThenBy(activation => activation.Id)
            .Select(From)
            .ToArray();
    }
}
