
namespace Styrhous.Licensing.Domain.Devices;

internal enum DeviceActivationDecisionKind
{
    Activate,
    AlreadyActive,
    ReplaceStale,
    LimitReached,
}

internal sealed record DeviceActivationDecision(
    DeviceActivationDecisionKind Kind,
    DeviceActivation? ExistingActivation = null,
    DeviceActivation? StaleActivation = null);

internal static class DeviceActivationPolicy
{
    public static DeviceActivationDecision Decide(
        IReadOnlyCollection<DeviceActivation> active,
        Guid installationId,
        int deviceLimit,
        DateTimeOffset observedAt)
    {
        ArgumentNullException.ThrowIfNull(active);

        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(deviceLimit);

        if (active.Any(activation => activation.RevokedAt is not null))
        {
            throw new ArgumentException("Only active device activations may be evaluated.", nameof(active));
        }

        var existing = active.SingleOrDefault(
            activation => activation.InstallationId == installationId);
        if (existing is not null)
        {
            return new DeviceActivationDecision(
                DeviceActivationDecisionKind.AlreadyActive,
                ExistingActivation: existing);
        }

        if (active.Count < deviceLimit)
        {
            return new DeviceActivationDecision(DeviceActivationDecisionKind.Activate);
        }

        if (active.Count > deviceLimit)
        {
            return new DeviceActivationDecision(DeviceActivationDecisionKind.LimitReached);
        }

        var staleBefore = observedAt.ToUniversalTime() - DeviceActivation.StaleAfter;
        var stale = active
            .Where(activation => activation.LastSeenAt < staleBefore)
            .OrderBy(activation => activation.LastSeenAt)
            .ThenBy(activation => activation.ActivatedAt)
            .ThenBy(activation => activation.Id)
            .FirstOrDefault();
        return stale is null
            ? new DeviceActivationDecision(DeviceActivationDecisionKind.LimitReached)
            : new DeviceActivationDecision(
                DeviceActivationDecisionKind.ReplaceStale,
                StaleActivation: stale);
    }
}
