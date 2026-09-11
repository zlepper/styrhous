using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Devices;

public sealed class DesktopDeviceSession
{
    private DesktopDeviceSession()
    {
    }

    private DesktopDeviceSession(
        Guid id,
        Guid activationId,
        Guid authorizationId,
        DateTimeOffset createdAt)
    {
        Id = id;
        ActivationId = activationId;
        AuthorizationId = authorizationId;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid ActivationId { get; private set; }

    public Guid AuthorizationId { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }

    public DateTimeOffset? RevokedAt { get; private set; }

    internal static DesktopDeviceSession Start(
        Guid activationId,
        Guid authorizationId,
        DateTimeOffset createdAt)
    {

        return new DesktopDeviceSession(
            Uuid7.Create(),
            activationId,
            authorizationId,
            createdAt.ToUniversalTime());
    }

    internal void Revoke(DateTimeOffset observedAt)
    {
        if (RevokedAt is null)
        {
            RevokedAt = observedAt.ToUniversalTime();
        }
    }
}
