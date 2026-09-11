using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationSeatAssignmentStore(
    IDbContextFactory<LicensingDbContext> contextFactory)
{
    private const int MaximumOptimisticAttempts = 3;

    public async Task<OrganizationSeatAssignmentResult> SetAssignedAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        bool assigned,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < MaximumOptimisticAttempts; attempt++)
        {
            try
            {
                var result = await TrySetAssignedAsync(
                    actorUserId,
                    organizationId,
                    candidateMembershipId,
                    assigned,
                    observedAt,
                    correlationId,
                    cancellationToken);
                if (result is not null)
                {
                    return result;
                }
            }
            catch (Exception exception) when (EfConcurrencyFailure.IsRetryable(exception))
            {
                if (attempt == MaximumOptimisticAttempts - 1)
                {
                    return Reject(OrganizationSeatAssignmentStatus.ConcurrentModification);
                }
            }
        }

        return Reject(OrganizationSeatAssignmentStatus.ConcurrentModification);
    }

    private async Task<OrganizationSeatAssignmentResult?> TrySetAssignedAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        bool assigned,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationSeatAssignmentResult?>(
            dbContext,
            async (transaction, token) =>
        {
            var actor = await PostgresOrganizationActorResolver.SerializeAndResolveAsync(
                dbContext,
                actorUserId,
                organizationId,
                cancellationToken);
            if (actor is null)
            {
                return Reject(OrganizationSeatAssignmentStatus.OrganizationNotFound);
            }

            if (!OrganizationSeatAssignmentPolicy.CanManage(actor.Role))
            {
                return Reject(OrganizationSeatAssignmentStatus.InsufficientPermission);
            }

            var membershipCandidate = candidateMembershipId is null
                ? null
                : await dbContext.OrganizationMemberships
                    .AsNoTracking()
                    .SingleOrDefaultAsync(
                        candidate => candidate.Id == candidateMembershipId.Value
                            && candidate.OrganizationId == organizationId,
                        cancellationToken);
            if (membershipCandidate is null)
            {
                return Reject(OrganizationSeatAssignmentStatus.MemberNotFound);
            }

            if (membershipCandidate.UserId != actorUserId
                && !await EfTransactionSerialization.TryClaimUserAsync(
                    dbContext,
                    membershipCandidate.UserId,
                    cancellationToken))
            {
                throw new UserNotFoundException();
            }

            var membership = await dbContext.OrganizationMemberships
                .AsNoTracking()
                .SingleOrDefaultAsync(
                    candidate => candidate.Id == membershipCandidate.Id
                        && candidate.OrganizationId == organizationId
                        && candidate.UserId == membershipCandidate.UserId,
                    cancellationToken);
            if (membership is null)
            {
                return Reject(OrganizationSeatAssignmentStatus.MemberNotFound);
            }

            var organization = actor.Organization;
            var seat = await dbContext.Seats
                .AsNoTracking()
                .SingleOrDefaultAsync(
                    candidate => candidate.BillingAccountId == organization.BillingAccountId
                        && candidate.AssignedUserId == membership.UserId,
                    cancellationToken)
                ?? throw new InvalidOperationException(
                    "An organization membership must have an assigned seat identity.");
            var decision = OrganizationSeatAssignmentPolicy.Decide(
                actor.Role,
                seat.ProductAccessEnabled,
                assigned);
            if (decision != OrganizationSeatAssignmentDecision.Allowed)
            {
                return Reject(
                    decision == OrganizationSeatAssignmentDecision.Unchanged
                        ? OrganizationSeatAssignmentStatus.Unchanged
                        : OrganizationSeatAssignmentStatus.InsufficientPermission);
            }

            if (assigned)
            {
                var seatCapacity = await PostgresOrganizationSeatCapacity
                    .SerializeBillingAccountAndResolveActiveAsync(
                        dbContext,
                        organization.BillingAccountId,
                        observedAt,
                        cancellationToken);
                if (seatCapacity is null)
                {
                    return Reject(OrganizationSeatAssignmentStatus.NoActiveSeatCapacity);
                }

                if (!await PostgresOrganizationInvitationCapacity.HasAvailableSeatAsync(
                        dbContext,
                        organization,
                        seatCapacity.Value,
                        observedAt,
                        excludedInvitationId: null,
                        cancellationToken))
                {
                    return Reject(OrganizationSeatAssignmentStatus.SeatCapacityReached);
                }

                await PostgresOrganizationInvitationCapacity.PromoteActiveReservationsAsync(
                    dbContext,
                    organization,
                    seatCapacity.Value,
                    observedAt,
                    excludedInvitationId: null,
                    cancellationToken);
            }
            else if (!await EfTransactionSerialization.TryClaimBillingAccountAsync(
                    dbContext,
                    organization.BillingAccountId,
                    cancellationToken))
            {
                throw new InvalidOperationException(
                    "An organization must have an existing billing account.");
            }

            var updatedCount = await dbContext.Seats
                .Where(candidate => candidate.Id == seat.Id
                    && candidate.BillingAccountId == organization.BillingAccountId
                    && candidate.AssignedUserId == membership.UserId
                    && candidate.ProductAccessEnabled == !assigned
                    && dbContext.OrganizationMemberships.Any(actorMembership =>
                        actorMembership.OrganizationId == organizationId
                        && actorMembership.UserId == actorUserId
                        && (actorMembership.Role == OrganizationRole.Owner
                            || actorMembership.Role == OrganizationRole.Admin))
                    && dbContext.OrganizationMemberships.Any(targetMembership =>
                        targetMembership.Id == membership.Id
                        && targetMembership.OrganizationId == organizationId
                        && targetMembership.UserId == membership.UserId))
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        candidate => candidate.ProductAccessEnabled,
                        assigned),
                    cancellationToken);
            if (updatedCount != 1)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            dbContext.AuditRecords.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    assigned ? AuditAction.SeatAssigned : AuditAction.SeatUnassigned,
                    AuditTargetType.Seat,
                    seat.Id,
                    observedAt));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationSeatAssignmentResult.Changed(
                organizationId,
                membership.Id,
                membership.UserId,
                seat.Id,
                assigned,
                seat.DeviceLimit,
                correlationId,
                observedAt);
        },
            cancellationToken);
    }

    private static OrganizationSeatAssignmentResult.Rejection Reject(
        OrganizationSeatAssignmentStatus status)
    {
        return OrganizationSeatAssignmentResult.Rejected(status);
    }
}
