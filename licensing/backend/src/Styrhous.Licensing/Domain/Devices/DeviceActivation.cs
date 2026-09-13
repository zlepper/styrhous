
namespace Styrhous.Licensing.Domain.Devices;

public enum DeviceRevocationReason
{
    StaleDeviceReplaced,
    Manual,
    EntitlementLost,
    MembershipRemoved,
}

public sealed class DeviceActivation
{
    public static readonly TimeSpan LastSeenWriteInterval = TimeSpan.FromHours(1);

    public static readonly TimeSpan StaleAfter = TimeSpan.FromDays(7);

    private DeviceActivation()
    {
    }

    private DeviceActivation(
        Guid id,
        Guid seatId,
        Guid userId,
        DesktopInstallation installation,
        DateTimeOffset activatedAt)
    {
        Id = id;
        SeatId = seatId;
        UserId = userId;
        InstallationId = installation.InstallationId;
        DisplayName = installation.DisplayName;
        Platform = installation.Platform;
        Architecture = installation.Architecture;
        StyrhousVersion = installation.StyrhousVersion;
        ActivatedAt = activatedAt;
        LastSeenAt = activatedAt;
    }

    public Guid Id { get; private set; }

    public Guid SeatId { get; private set; }

    public Guid UserId { get; private set; }

    public Guid InstallationId { get; private set; }

    public string DisplayName { get; private set; } = string.Empty;

    public string Platform { get; private set; } = string.Empty;

    public string Architecture { get; private set; } = string.Empty;

    public string StyrhousVersion { get; private set; } = string.Empty;

    public DateTimeOffset ActivatedAt { get; private set; }

    public DateTimeOffset LastSeenAt { get; private set; }

    public DateTimeOffset? RevokedAt { get; private set; }

    public DeviceRevocationReason? RevocationReason { get; private set; }

    internal static DeviceActivation Activate(
        Guid seatId,
        Guid userId,
        DesktopInstallation installation,
        DateTimeOffset observedAt)
    {

        ArgumentNullException.ThrowIfNull(installation);
        return new DeviceActivation(
            Guid.CreateVersion7(),
            seatId,
            userId,
            installation,
            observedAt.ToUniversalTime());
    }

    internal void RevokeAsStale(DateTimeOffset observedAt)
    {
        var utcObservedAt = observedAt.ToUniversalTime();
        if (LastSeenAt >= utcObservedAt - StaleAfter)
        {
            throw new InvalidOperationException("Only a stale device activation can be replaced.");
        }

        Revoke(utcObservedAt, DeviceRevocationReason.StaleDeviceReplaced);
    }

    internal void RevokeManually(DateTimeOffset observedAt)
    {
        Revoke(observedAt.ToUniversalTime(), DeviceRevocationReason.Manual);
    }

    internal bool RecordSuccessfulEntitlementCheck(DateTimeOffset observedAt)
    {
        if (RevokedAt is not null)
        {
            throw new InvalidOperationException("A revoked device cannot record an entitlement check.");
        }

        var utcObservedAt = observedAt.ToUniversalTime();
        if (utcObservedAt < LastSeenAt + LastSeenWriteInterval)
        {
            return false;
        }

        LastSeenAt = utcObservedAt;
        return true;
    }

    private void Revoke(DateTimeOffset observedAt, DeviceRevocationReason reason)
    {
        if (RevokedAt is not null)
        {
            throw new InvalidOperationException("The device activation is already revoked.");
        }

        RevokedAt = observedAt < LastSeenAt ? LastSeenAt : observedAt;
        RevocationReason = reason;
    }
}
