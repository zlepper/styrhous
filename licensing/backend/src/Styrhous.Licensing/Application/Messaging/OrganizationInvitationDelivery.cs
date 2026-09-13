using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Messaging;

public enum OrganizationInvitationDeliveryKind
{
    Created,
    Resent,
}

public sealed class OrganizationInvitationDelivery
{
    private const string RedactedValue = "[REDACTED INVITATION DELIVERY]";

    private OrganizationInvitationDelivery(
        OrganizationInvitationDeliveryKind kind,
        Guid invitationId,
        Guid organizationId,
        string email,
        OrganizationRole role,
        OrganizationInvitationSecret secret,
        DateTimeOffset expiresAt)
    {
        Kind = kind;
        InvitationId = invitationId;
        OrganizationId = organizationId;
        Email = email;
        Role = role;
        Secret = secret;
        ExpiresAt = expiresAt;
    }

    public OrganizationInvitationDeliveryKind Kind { get; }

    public Guid InvitationId { get; }

    public Guid OrganizationId { get; }

    public string Email { get; }

    public OrganizationRole Role { get; }

    public OrganizationInvitationSecret Secret { get; }

    public DateTimeOffset ExpiresAt { get; }

    internal static OrganizationInvitationDelivery From(
        OrganizationInvitationDeliveryKind kind,
        OrganizationInvitation invitation,
        OrganizationInvitationSecret secret)
    {
        ArgumentNullException.ThrowIfNull(invitation);
        ArgumentNullException.ThrowIfNull(secret);
        if (!secret.MatchesHash(invitation.SecretHash))
        {
            throw new ArgumentException(
                "The delivery secret must match the invitation secret hash.",
                nameof(secret));
        }

        return Restore(
            kind,
            invitation.Id,
            invitation.OrganizationId,
            invitation.Email,
            invitation.Role,
            secret.Reveal(),
            invitation.ExpiresAt);
    }

    internal static OrganizationInvitationDelivery Restore(
        OrganizationInvitationDeliveryKind kind,
        Guid invitationId,
        Guid organizationId,
        string email,
        OrganizationRole role,
        string secret,
        DateTimeOffset expiresAt)
    {
        return new OrganizationInvitationDelivery(
            kind,
            invitationId,
            organizationId,
            email,
            role,
            OrganizationInvitationSecret.FromTrustedPayload(secret),
            expiresAt);
    }

    public override string ToString()
    {
        return RedactedValue;
    }
}
