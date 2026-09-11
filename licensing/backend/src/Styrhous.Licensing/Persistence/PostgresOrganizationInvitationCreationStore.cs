using Styrhous.Licensing.Infrastructure.Messaging;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationCreationStore(
    LicensingDbContext dbContext,
    DataProtectionOrganizationInvitationDeliveryProtector deliveryProtector,
    PostgresBackgroundWorkOutbox outbox)

{

    public async Task<OrganizationInvitationCreationStoreResult> CreateAsync(
        OrganizationInvitation invitation,
        OrganizationInvitationSecret secret,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationInvitationCreationStoreResult>(
            dbContext,
            async (transaction, token) =>
        {
            var authorization = await PostgresOrganizationInvitationAuthorization
                .SerializeAndAuthorizeAsync(
                    dbContext,
                    invitation.CreatedByUserId,
                    invitation.OrganizationId,
                    cancellationToken);
            if (authorization.Status
                == OrganizationInvitationAuthorizationStatus.OrganizationNotFound)
            {
                return Rejected(OrganizationInvitationCreationStatus.OrganizationNotFound);
            }

            if (authorization.Status
                == OrganizationInvitationAuthorizationStatus.InsufficientPermission)
            {
                return Rejected(OrganizationInvitationCreationStatus.InsufficientPermission);
            }

            var organization = authorization.Organization;
            var eligibility = await PostgresOrganizationInvitationEligibility.CheckAsync(
                dbContext,
                organization,
                invitation.NormalizedEmail,
                invitation.CreatedAt,
                invitation.AssignProductSeat,
                excludedInvitationId: null,
                preservedSeatCapacity: null,
                cancellationToken);
            if (eligibility.Status != OrganizationInvitationEligibilityStatus.Eligible)
            {
                return Rejected(eligibility.Status switch
                {
                    OrganizationInvitationEligibilityStatus.AlreadyMember =>
                        OrganizationInvitationCreationStatus.AlreadyMember,
                    OrganizationInvitationEligibilityStatus.InvitationAlreadyPending =>
                        OrganizationInvitationCreationStatus.InvitationAlreadyPending,
                    OrganizationInvitationEligibilityStatus.NoActiveSeatCapacity =>
                        OrganizationInvitationCreationStatus.NoActiveSeatCapacity,
                    OrganizationInvitationEligibilityStatus.SeatCapacityReached =>
                        OrganizationInvitationCreationStatus.SeatCapacityReached,
                    _ => throw new InvalidOperationException(
                        $"Unsupported invitation eligibility status: {eligibility}."),
                });
            }

            await PostgresOrganizationInvitationCapacity.ReserveEligibleInvitationAsync(
                dbContext,
                organization,
                invitation,
                eligibility.ReservedSeatCapacity,
                invitation.CreatedAt,
                excludedInvitationId: null,
                cancellationToken);
            dbContext.OrganizationInvitations.Add(invitation);
            dbContext.AuditRecords.Add(
                AuditRecord.Create(
                    correlationId,
                    invitation.CreatedByUserId,
                    AuditAction.OrganizationInvitationCreated,
                    AuditTargetType.OrganizationInvitation,
                    invitation.Id,
                    invitation.CreatedAt));
            var outboxMessage = PostgresOrganizationInvitationDeliveryOutbox.Enqueue(
                dbContext,
                deliveryProtector,
                OrganizationInvitationDeliveryKind.Created,
                invitation,
                secret,
                correlationId,
                invitation.CreatedAt);

            await outbox.EnqueueAsync(
                dbContext,
                BackgroundWorkReference.OrganizationInvitationDelivery(outboxMessage.Id, outboxMessage.OccurredAt),
                cancellationToken);
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return new OrganizationInvitationCreationStoreResult.Success(
                BackgroundWorkReference.OrganizationInvitationDelivery(
                    outboxMessage.Id,
                    outboxMessage.OccurredAt));
        },
            cancellationToken);
    }

    private static OrganizationInvitationCreationStoreResult.Rejection Rejected(
        OrganizationInvitationCreationStatus status)
    {
        return OrganizationInvitationCreationStoreResult.Rejection.From(status);
    }
}
