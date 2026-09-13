using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationCancellationStore(
    LicensingDbContext dbContext)
{

    public async Task<OrganizationInvitationCancellationStatus> CancelAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid invitationId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationInvitationCancellationStatus>(
            dbContext,
            async (transaction, token) =>
        {
            var authorization = await PostgresOrganizationInvitationAuthorization
                .SerializeAndAuthorizeAsync(
                    dbContext,
                    actorUserId,
                    organizationId,
                    cancellationToken);
            if (authorization.Status
                == OrganizationInvitationAuthorizationStatus.OrganizationNotFound)
            {
                return OrganizationInvitationCancellationStatus.OrganizationNotFound;
            }

            if (authorization.Status
                == OrganizationInvitationAuthorizationStatus.InsufficientPermission)
            {
                return OrganizationInvitationCancellationStatus.InsufficientPermission;
            }

            var invitation = await EfTransactionSerialization
                .FindOrganizationInvitationAsync(
                dbContext,
                organizationId,
                invitationId,
                cancellationToken);
            if (invitation is null
                || invitation.AcceptedAt is not null
                || invitation.CancelledAt is not null)
            {
                return OrganizationInvitationCancellationStatus.InvitationNotFound;
            }

            if (invitation.LastSentAt > observedAt)
            {
                return OrganizationInvitationCancellationStatus.Superseded;
            }

            if (!invitation.TryCancel(observedAt))
            {
                return OrganizationInvitationCancellationStatus.InvitationNotFound;
            }

            await PostgresOrganizationInvitationDeliveryOutbox.DiscardPendingAsync(
                dbContext,
                invitation.Id,
                OutboxDiscardReason.InvitationCancelled,
                observedAt,
                cancellationToken);

            dbContext.AuditRecords.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.OrganizationInvitationCancelled,
                    AuditTargetType.OrganizationInvitation,
                    invitation.Id,
                    observedAt));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationInvitationCancellationStatus.Cancelled;
        },
            cancellationToken);
    }
}
