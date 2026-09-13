using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresOrganizationInvitationQueries
{
    public static IQueryable<OrganizationInvitation> Active(
        LicensingDbContext dbContext,
        DateTimeOffset observedAt,
        Guid? excludedInvitationId = null)
    {
        var invitations = dbContext.OrganizationInvitations.Where(
            invitation => invitation.AcceptedAt == null
                && invitation.CancelledAt == null
                && invitation.ExpiresAt > observedAt);
        return excludedInvitationId is Guid excludedId
            ? invitations.Where(invitation => invitation.Id != excludedId)
            : invitations;
    }
}
