using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationAcceptanceStore(
    LicensingDbContext dbContext,
    TimeProvider timeProvider)
{

    public async Task<OrganizationInvitationAcceptanceResult> AcceptAsync(
        Guid actorUserId,
        string secretHash,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationInvitationAcceptanceResult>(
            dbContext,
            async (transaction, token) =>
        {
            if (!await EfTransactionSerialization.TryClaimUserAsync(
                    dbContext,
                    actorUserId,
                    cancellationToken))
            {
                throw new UserNotFoundException();
            }

            var reference = await dbContext.OrganizationInvitations
                .AsNoTracking()
                .Where(invitation => invitation.SecretHash == secretHash)
                .Select(invitation => new
                {
                    invitation.Id,
                    invitation.OrganizationId,
                })
                .SingleOrDefaultAsync(cancellationToken);
            if (reference is null)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.InvitationNotFound);
            }

            var organization = await EfTransactionSerialization.FindOrganizationAndClaimAsync(
                dbContext,
                reference.OrganizationId,
                cancellationToken);
            if (organization is null)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.InvitationNotFound);
            }

            var invitation = await EfTransactionSerialization
                .FindOrganizationInvitationAsync(
                dbContext,
                reference.OrganizationId,
                reference.Id,
                cancellationToken);
            var acceptedAt = timeProvider.GetUtcNow().ToUniversalTime();
            if (invitation is null
                || invitation.SecretHash != secretHash
                || invitation.AcceptedAt is not null
                || invitation.CancelledAt is not null
                || acceptedAt < invitation.LastSentAt
                || acceptedAt >= invitation.ExpiresAt)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.InvitationNotFound);
            }

            var hasBillingAccount = await EfTransactionSerialization.TryClaimBillingAccountAsync(
                dbContext,
                organization.BillingAccountId,
                cancellationToken);
            if (!hasBillingAccount)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.InvitationNotFound);
            }

            var preservedSeatCapacity = 0;
            if (invitation.AssignProductSeat)
            {
                preservedSeatCapacity = await PostgresOrganizationInvitationCapacity
                    .ResolvePreservedCapacityAsync(
                        dbContext,
                        organization,
                        invitation.ReservedSeatCapacity,
                        acceptedAt,
                        invitation.Id,
                        cancellationToken);
                if (!await PostgresOrganizationInvitationCapacity.HasAvailableSeatAsync(
                        dbContext,
                        organization,
                        preservedSeatCapacity,
                        acceptedAt,
                        invitation.Id,
                        cancellationToken))
                {
                    return OrganizationInvitationAcceptanceResult.Rejected(
                        OrganizationInvitationAcceptanceStatus.InvitationNotFound);
                }
            }

            var emailMatches = await dbContext.VerifiedEmailClaims.AnyAsync(
                claim => claim.UserId == actorUserId
                    && claim.NormalizedEmail == invitation.NormalizedEmail,
                cancellationToken);
            if (!emailMatches)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.EmailMismatch);
            }

            var alreadyMember = await dbContext.OrganizationMemberships.AnyAsync(
                membership => membership.OrganizationId == organization.Id
                    && membership.UserId == actorUserId,
                cancellationToken);
            var alreadyHasSeat = await dbContext.Seats.AnyAsync(
                seat => seat.BillingAccountId == organization.BillingAccountId
                    && seat.AssignedUserId == actorUserId,
                cancellationToken);
            if (alreadyMember || alreadyHasSeat)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.AlreadyMember);
            }

            var acceptance = OrganizationInvitationAcceptance.TryComplete(
                organization,
                invitation,
                actorUserId,
                acceptedAt);
            if (acceptance is null)
            {
                return OrganizationInvitationAcceptanceResult.Rejected(
                    OrganizationInvitationAcceptanceStatus.InvitationNotFound);
            }

            if (invitation.AssignProductSeat)
            {
                await PostgresOrganizationInvitationCapacity.PromoteActiveReservationsAsync(
                    dbContext,
                    organization,
                    preservedSeatCapacity,
                    acceptedAt,
                    invitation.Id,
                    cancellationToken);
            }

            await PostgresOrganizationInvitationDeliveryOutbox.DiscardPendingAsync(
                dbContext,
                invitation.Id,
                OutboxDiscardReason.InvitationAccepted,
                acceptedAt,
                cancellationToken);

            dbContext.AddRange(acceptance.Membership, acceptance.Seat);
            dbContext.AuditRecords.AddRange(
                OrganizationInvitationAcceptanceAuditRecords.Create(
                    acceptance,
                    correlationId));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationInvitationAcceptanceResult.Accepted(
                acceptance,
                correlationId);
        },
            cancellationToken);
    }
}
