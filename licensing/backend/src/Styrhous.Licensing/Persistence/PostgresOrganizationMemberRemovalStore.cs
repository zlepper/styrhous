using System.Data;
using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationMemberRemovalStore(
    IDbContextFactory<LicensingDbContext> contextFactory)
{
    private const int MaximumOptimisticAttempts = 3;

    public async Task<OrganizationMemberRemovalResult> RemoveAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < MaximumOptimisticAttempts; attempt++)
        {
            try
            {
                var result = await TryRemoveAsync(
                    actorUserId,
                    organizationId,
                    candidateMembershipId,
                    observedAt,
                    correlationId,
                    cancellationToken);
                if (result is not null)
                {
                    return result;
                }
            }
            catch (Exception exception) when (
                EfConcurrencyFailure.IsRetryable(exception)
                || IsConcurrentReferenceChange(exception))
            {
                if (attempt == MaximumOptimisticAttempts - 1)
                {
                    return Reject(OrganizationMemberRemovalStatus.ConcurrentModification);
                }

                continue;
            }
        }

        return Reject(OrganizationMemberRemovalStatus.ConcurrentModification);
    }

    private async Task<OrganizationMemberRemovalResult?> TryRemoveAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        await using var strategyContext = await contextFactory.CreateDbContextAsync(
            cancellationToken);
        return await strategyContext.Database.CreateExecutionStrategy()
            .ExecuteAsync<OrganizationMemberRemovalResult?>(
            async () =>
        {
            await using var dbContext = await contextFactory.CreateDbContextAsync(
                cancellationToken);
            await using var transaction = await dbContext.Database.BeginTransactionAsync(
                IsolationLevel.Serializable,
                cancellationToken);
            if (!await dbContext.UserAccounts
                    .AsNoTracking()
                    .AnyAsync(user => user.Id == actorUserId, cancellationToken))
            {
                throw new UserNotFoundException();
            }

            var organization = await dbContext.Organizations
                .AsNoTracking()
                .SingleOrDefaultAsync(
                    candidate => candidate.Id == organizationId,
                    cancellationToken);
            if (organization is null)
            {
                return Reject(OrganizationMemberRemovalStatus.OrganizationNotFound);
            }

            var memberships = await dbContext.OrganizationMemberships
                .AsNoTracking()
                .Where(membership => membership.OrganizationId == organizationId
                    && (membership.UserId == actorUserId
                        || (candidateMembershipId != null
                            && membership.Id == candidateMembershipId.Value)))
                .ToArrayAsync(cancellationToken);
            var actorMembership = memberships.SingleOrDefault(
                membership => membership.UserId == actorUserId);
            if (actorMembership is null)
            {
                return Reject(OrganizationMemberRemovalStatus.OrganizationNotFound);
            }

            var membership = candidateMembershipId is null
                ? null
                : memberships.SingleOrDefault(
                    candidate => candidate.Id == candidateMembershipId.Value);
            if (membership is null)
            {
                return Reject(OrganizationMemberRemovalStatus.MemberNotFound);
            }

            var decision = OrganizationMemberRemovalPolicy.Decide(
                actorUserId,
                actorMembership.Role,
                membership.UserId,
                membership.Role);
            if (decision != OrganizationMemberRemovalDecision.Allowed)
            {
                return Reject(
                    decision == OrganizationMemberRemovalDecision.OwnershipTransferRequired
                        ? OrganizationMemberRemovalStatus.OwnershipTransferRequired
                        : OrganizationMemberRemovalStatus.InsufficientPermission);
            }

            if (!await EfTransactionSerialization.TryClaimUserAsync(
                    dbContext,
                    membership.UserId,
                    cancellationToken))
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            var seat = await dbContext.Seats
                .AsNoTracking()
                .SingleOrDefaultAsync(
                    candidate => candidate.BillingAccountId == organization.BillingAccountId
                        && candidate.AssignedUserId == membership.UserId,
                    cancellationToken)
                ?? throw new InvalidOperationException(
                    "An organization membership must have an assigned organization seat.");
            if (!await dbContext.BillingAccounts
                    .AsNoTracking()
                    .AnyAsync(
                        account => account.Id == organization.BillingAccountId,
                        cancellationToken))
            {
                throw new InvalidOperationException(
                    "An organization must have an existing billing account.");
            }

            var deviceActivations = await dbContext.DeviceActivations
                .AsNoTracking()
                .Where(activation => activation.SeatId == seat.Id)
                .ToArrayAsync(cancellationToken);
            await DesktopSessionRevocation.RevokeForActivationsAsync(
                dbContext,
                deviceActivations.Select(activation => activation.Id).ToArray(),
                observedAt,
                retainSessionRecords: false,
                cancellationToken);
            var deletedActivationCount = await dbContext.DeviceActivations
                .Where(activation => activation.SeatId == seat.Id)
                .ExecuteDeleteAsync(cancellationToken);
            if (deletedActivationCount != deviceActivations.Length)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            var deletedSeatCount = await dbContext.Seats
                .Where(candidate => candidate.Id == seat.Id
                    && candidate.BillingAccountId == organization.BillingAccountId
                    && candidate.AssignedUserId == membership.UserId)
                .ExecuteDeleteAsync(cancellationToken);
            if (deletedSeatCount != 1)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            var deletedMembershipCount = await dbContext.OrganizationMemberships
                .Where(candidate => candidate.Id == membership.Id
                    && candidate.OrganizationId == organizationId
                    && candidate.UserId == membership.UserId
                    && candidate.Role == membership.Role)
                .ExecuteDeleteAsync(cancellationToken);
            if (deletedMembershipCount != 1)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            dbContext.AuditRecords.AddRange(
                OrganizationMemberRemovalAuditRecords.Create(
                    actorUserId,
                    membership,
                    seat,
                    deviceActivations,
                    correlationId,
                    observedAt));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationMemberRemovalResult.Removed(
                organizationId,
                membership.Id,
                membership.UserId,
                seat.Id,
                correlationId,
                observedAt);
        });
    }

    private static bool IsConcurrentReferenceChange(Exception exception)
    {
        return exception is PostgresException
        {
            SqlState: PostgresErrorCodes.ForeignKeyViolation
                or PostgresErrorCodes.RestrictViolation,
        }
        || exception is DbUpdateException
        {
            InnerException: PostgresException
            {
                SqlState: PostgresErrorCodes.ForeignKeyViolation
                    or PostgresErrorCodes.RestrictViolation,
            },
        };
    }

    private static OrganizationMemberRemovalResult.Rejection Reject(
        OrganizationMemberRemovalStatus status)
    {
        return OrganizationMemberRemovalResult.Rejected(status);
    }
}
