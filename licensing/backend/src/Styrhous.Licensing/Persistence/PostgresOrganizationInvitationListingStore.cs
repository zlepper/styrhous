using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationListingStore(LicensingDbContext dbContext)

{

    public async Task<OrganizationInvitationListingResult> ListAsync(
        Guid actorUserId,
        Guid organizationId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        await PostgresReadChecks.EnsureUserExistsAsync(
            dbContext,
            actorUserId,
            cancellationToken);

        var utcObservedAt = observedAt.ToUniversalTime();
        var rows = await (
                from membership in dbContext.OrganizationMemberships.AsNoTracking()
                join candidateInvitation in PostgresOrganizationInvitationQueries
                        .Active(dbContext, utcObservedAt)
                        .AsNoTracking()
                    on new
                    {
                        membership.OrganizationId,
                        CanList = membership.Role == OrganizationRole.Owner
                            || membership.Role == OrganizationRole.Admin,
                    }
                    equals new
                    {
                        candidateInvitation.OrganizationId,
                        CanList = true,
                    }
                    into activeInvitations
                from invitation in activeInvitations.DefaultIfEmpty()
                where membership.OrganizationId == organizationId
                    && membership.UserId == actorUserId
                orderby invitation.CreatedAt, invitation.Id
                select new
                {
                    ActorRole = membership.Role,
                    InvitationId = (Guid?)invitation.Id,
                    CreatedByUserId = (Guid?)invitation.CreatedByUserId,
                    invitation.Email,
                    InvitationRole = (OrganizationRole?)invitation.Role,
                    AssignProductSeat = (bool?)invitation.AssignProductSeat,
                    CreatedAt = (DateTimeOffset?)invitation.CreatedAt,
                    LastSentAt = (DateTimeOffset?)invitation.LastSentAt,
                    ExpiresAt = (DateTimeOffset?)invitation.ExpiresAt,
                })
            .ToArrayAsync(cancellationToken);
        if (rows.Length == 0)
        {
            return OrganizationInvitationListingResult.Rejected(
                OrganizationInvitationListingStatus.OrganizationNotFound);
        }

        if (rows[0].ActorRole is not OrganizationRole.Owner and not OrganizationRole.Admin)
        {
            return OrganizationInvitationListingResult.Rejected(
                OrganizationInvitationListingStatus.InsufficientPermission);
        }

        var invitations = rows
            .Where(row => row.InvitationId is not null)
            .Select(row => new OrganizationInvitationSummary(
                row.InvitationId!.Value,
                row.CreatedByUserId!.Value,
                row.Email,
                row.InvitationRole!.Value,
                row.AssignProductSeat!.Value,
                row.CreatedAt!.Value,
                row.LastSentAt!.Value,
                row.ExpiresAt!.Value))
            .ToArray();
        return OrganizationInvitationListingResult.Listed(
            organizationId,
            invitations);
    }
}
