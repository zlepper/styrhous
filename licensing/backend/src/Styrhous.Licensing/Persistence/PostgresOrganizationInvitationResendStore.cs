using Styrhous.Licensing.Infrastructure.Messaging;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationResendStore(
    LicensingDbContext dbContext,
    DataProtectionOrganizationInvitationDeliveryProtector deliveryProtector,
    PostgresBackgroundWorkOutbox outbox)

{

    public async Task<OrganizationInvitationResendStoreResult> ResendAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid invitationId,
        OrganizationInvitationSecret secret,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationInvitationResendStoreResult>(
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
                return Rejected(OrganizationInvitationResendStatus.OrganizationNotFound);
            }

            if (authorization.Status
                == OrganizationInvitationAuthorizationStatus.InsufficientPermission)
            {
                return Rejected(OrganizationInvitationResendStatus.InsufficientPermission);
            }

            var organization = authorization.Organization;
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
                return Rejected(OrganizationInvitationResendStatus.InvitationNotFound);
            }

            if (invitation.LastSentAt > observedAt)
            {
                return Rejected(OrganizationInvitationResendStatus.Superseded);
            }

            int? preservedSeatCapacity = invitation.AssignProductSeat
                && observedAt < invitation.ExpiresAt
                ? invitation.ReservedSeatCapacity
                : null;
            var eligibility = await PostgresOrganizationInvitationEligibility.CheckAsync(
                dbContext,
                organization,
                invitation.NormalizedEmail,
                observedAt,
                invitation.AssignProductSeat,
                invitation.Id,
                preservedSeatCapacity,
                cancellationToken);
            if (eligibility.Status != OrganizationInvitationEligibilityStatus.Eligible)
            {
                return Rejected(eligibility.Status switch
                {
                    OrganizationInvitationEligibilityStatus.AlreadyMember =>
                        OrganizationInvitationResendStatus.AlreadyMember,
                    OrganizationInvitationEligibilityStatus.InvitationAlreadyPending =>
                        OrganizationInvitationResendStatus.InvitationAlreadyPending,
                    OrganizationInvitationEligibilityStatus.NoActiveSeatCapacity =>
                        OrganizationInvitationResendStatus.NoActiveSeatCapacity,
                    OrganizationInvitationEligibilityStatus.SeatCapacityReached =>
                        OrganizationInvitationResendStatus.SeatCapacityReached,
                    _ => throw new InvalidOperationException(
                        $"Unsupported invitation eligibility status: {eligibility}."),
                });
            }

            await PostgresOrganizationInvitationCapacity.ReserveEligibleInvitationAsync(
                dbContext,
                organization,
                invitation,
                eligibility.ReservedSeatCapacity,
                observedAt,
                invitation.Id,
                cancellationToken);
            if (!invitation.TryResend(secret.Hash, observedAt))
            {
                return Rejected(OrganizationInvitationResendStatus.InvitationNotFound);
            }

            await PostgresOrganizationInvitationDeliveryOutbox.DiscardPendingAsync(
                dbContext,
                invitation.Id,
                OutboxDiscardReason.Superseded,
                observedAt,
                cancellationToken);

            dbContext.AuditRecords.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.OrganizationInvitationResent,
                    AuditTargetType.OrganizationInvitation,
                    invitation.Id,
                    observedAt));
            var outboxMessage = PostgresOrganizationInvitationDeliveryOutbox.Enqueue(
                dbContext,
                deliveryProtector,
                OrganizationInvitationDeliveryKind.Resent,
                invitation,
                secret,
                correlationId,
                observedAt);

            await outbox.EnqueueAsync(
                dbContext,
                BackgroundWorkReference.OrganizationInvitationDelivery(outboxMessage.Id, outboxMessage.OccurredAt),
                cancellationToken);
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return new OrganizationInvitationResendStoreResult.Success(
                BackgroundWorkReference.OrganizationInvitationDelivery(
                    outboxMessage.Id,
                    outboxMessage.OccurredAt));
        },
            cancellationToken);
    }

    private static OrganizationInvitationResendStoreResult.Rejection Rejected(
        OrganizationInvitationResendStatus status)
    {
        return OrganizationInvitationResendStoreResult.Rejection.From(status);
    }
}
