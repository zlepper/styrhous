using Styrhous.Licensing.Domain.Accounts;

namespace Styrhous.Licensing.Domain.Organizations;

public sealed class OrganizationInvitationAcceptance
{
    private OrganizationInvitationAcceptance(
        OrganizationInvitation invitation,
        OrganizationMembership membership,
        Seat seat,
        DateTimeOffset observedAt)
    {
        Invitation = invitation;
        Membership = membership;
        Seat = seat;
        ObservedAt = observedAt;
    }

    public OrganizationInvitation Invitation { get; }

    public OrganizationMembership Membership { get; }

    public Seat Seat { get; }

    public DateTimeOffset ObservedAt { get; }

    public static OrganizationInvitationAcceptance? TryComplete(
        Organization organization,
        OrganizationInvitation invitation,
        Guid userId,
        DateTimeOffset observedAt)
    {
        ArgumentNullException.ThrowIfNull(organization);
        ArgumentNullException.ThrowIfNull(invitation);
        if (organization.Id != invitation.OrganizationId)
        {
            throw new ArgumentException(
                "The invitation does not belong to the organization.",
                nameof(invitation));
        }

        var utcObservedAt = observedAt.ToUniversalTime();
        if (!invitation.TryAccept(userId, utcObservedAt))
        {
            return null;
        }

        var membership = OrganizationMembership.AcceptInvitation(
            organization.Id,
            userId,
            invitation.Role,
            utcObservedAt);
        var seat = Seat.Assign(
            organization.BillingAccountId,
            userId,
            utcObservedAt,
            invitation.AssignProductSeat);
        return new OrganizationInvitationAcceptance(
            invitation,
            membership,
            seat,
            utcObservedAt);
    }
}
