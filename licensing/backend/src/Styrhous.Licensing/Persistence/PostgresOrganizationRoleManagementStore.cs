using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationRoleManagementStore(
    IDbContextFactory<LicensingDbContext> contextFactory)
{
    private const int MaximumOptimisticAttempts = 3;

    public async Task<OrganizationRoleManagementResult> ChangeRoleAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        OrganizationRole requestedRole,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < MaximumOptimisticAttempts; attempt++)
        {
            var result = await TryChangeRoleAsync(
                actorUserId,
                organizationId,
                candidateMembershipId,
                requestedRole,
                observedAt,
                correlationId,
                cancellationToken);
            if (result is not null)
            {
                return result;
            }
        }

        return Reject(OrganizationRoleManagementStatus.ConcurrentModification);
    }

    public async Task<OrganizationRoleManagementResult> TransferOwnershipAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < MaximumOptimisticAttempts; attempt++)
        {
            var result = await TryTransferOwnershipAsync(
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

        return Reject(OrganizationRoleManagementStatus.ConcurrentModification);
    }

    private async Task<OrganizationRoleManagementResult?> TryChangeRoleAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        OrganizationRole requestedRole,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationRoleManagementResult?>(
            dbContext,
            async (transaction, token) =>
        {
            var context = await LoadContextAsync(
                dbContext,
                actorUserId,
                organizationId,
                candidateMembershipId,
                cancellationToken);
            if (context.Rejection is not null)
            {
                return context.Rejection;
            }

            var actor = context.Actor!;
            var target = context.Target!;
            var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
                actor.Role,
                target.Role,
                requestedRole);
            if (decision != OrganizationRoleManagementDecision.Allowed)
            {
                return Reject(ToStatus(decision));
            }

            var updatedCount = await dbContext.OrganizationMemberships
                .Where(membership => membership.Id == target.Id
                    && membership.OrganizationId == organizationId
                    && membership.Role == target.Role
                    && dbContext.OrganizationMemberships.Any(actorMembership =>
                        actorMembership.OrganizationId == organizationId
                        && actorMembership.UserId == actorUserId
                        && actorMembership.Role == OrganizationRole.Owner))
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        membership => membership.Role,
                        requestedRole),
                    cancellationToken);
            if (updatedCount == 0)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            dbContext.AuditRecords.Add(
                OrganizationRoleManagementAuditRecords.RoleChanged(
                    actorUserId,
                    target.Id,
                    correlationId,
                    observedAt));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationRoleManagementResult.Changed(
                organizationId,
                target.Id,
                target.UserId,
                target.Role,
                requestedRole,
                correlationId,
                observedAt);
        },
            cancellationToken);
    }

    private async Task<OrganizationRoleManagementResult?> TryTransferOwnershipAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? candidateMembershipId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationRoleManagementResult?>(
            dbContext,
            async (transaction, token) =>
        {
            var context = await LoadContextAsync(
                dbContext,
                actorUserId,
                organizationId,
                candidateMembershipId,
                cancellationToken);
            if (context.Rejection is not null)
            {
                return context.Rejection;
            }

            var actor = context.Actor!;
            var target = context.Target!;
            var decision = OrganizationRoleManagementPolicy.DecideOwnershipTransfer(
                actorUserId,
                actor.Role,
                target.UserId,
                target.Role);
            if (decision != OrganizationRoleManagementDecision.Allowed)
            {
                return Reject(ToStatus(decision));
            }

            var demotedCount = await dbContext.OrganizationMemberships
                .Where(membership => membership.Id == actor.Id
                    && membership.OrganizationId == organizationId
                    && membership.Role == OrganizationRole.Owner)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        membership => membership.Role,
                        OrganizationRole.Admin),
                    cancellationToken);
            if (demotedCount == 0)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            var promotedCount = await dbContext.OrganizationMemberships
                .Where(membership => membership.Id == target.Id
                    && membership.OrganizationId == organizationId
                    && membership.Role == target.Role)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        membership => membership.Role,
                        OrganizationRole.Owner),
                    cancellationToken);
            if (promotedCount == 0)
            {
                await transaction.RollbackAsync(cancellationToken);
                return null;
            }

            dbContext.AuditRecords.AddRange(
                OrganizationRoleManagementAuditRecords.OwnershipTransferred(
                    actorUserId,
                    actor.Id,
                    target.Id,
                    correlationId,
                    observedAt));
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationRoleManagementResult.Transferred(
                organizationId,
                actor.Id,
                actor.UserId,
                target.Id,
                target.UserId,
                correlationId,
                observedAt);
        },
            cancellationToken);
    }

    private static async Task<RoleManagementContext> LoadContextAsync(
        LicensingDbContext dbContext,
        Guid actorUserId,
        Guid organizationId,
        Guid? membershipId,
        CancellationToken cancellationToken)
    {
        if (!await dbContext.UserAccounts
                .AsNoTracking()
                .AnyAsync(user => user.Id == actorUserId, cancellationToken))
        {
            throw new UserNotFoundException();
        }

        if (!await dbContext.Organizations
                .AsNoTracking()
                .AnyAsync(organization => organization.Id == organizationId, cancellationToken))
        {
            return RoleManagementContext.Rejected(
                OrganizationRoleManagementStatus.OrganizationNotFound);
        }

        var memberships = await dbContext.OrganizationMemberships
            .AsNoTracking()
            .Where(membership => membership.OrganizationId == organizationId
                && (membership.UserId == actorUserId
                    || (membershipId != null && membership.Id == membershipId.Value)))
            .Select(membership => new MembershipSnapshot(
                membership.Id,
                membership.UserId,
                membership.Role))
            .ToArrayAsync(cancellationToken);
        var actor = memberships.SingleOrDefault(
            membership => membership.UserId == actorUserId);
        if (actor is null)
        {
            return RoleManagementContext.Rejected(
                OrganizationRoleManagementStatus.OrganizationNotFound);
        }

        var target = membershipId is null
            ? null
            : memberships.SingleOrDefault(
                membership => membership.Id == membershipId.Value);
        return target is null
            ? RoleManagementContext.Rejected(OrganizationRoleManagementStatus.MemberNotFound)
            : RoleManagementContext.Loaded(actor, target);
    }

    private static OrganizationRoleManagementStatus ToStatus(
        OrganizationRoleManagementDecision decision)
    {
        return decision switch
        {
            OrganizationRoleManagementDecision.InsufficientPermission =>
                OrganizationRoleManagementStatus.InsufficientPermission,
            OrganizationRoleManagementDecision.InvalidOwnershipTarget =>
                OrganizationRoleManagementStatus.InvalidOwnershipTarget,
            OrganizationRoleManagementDecision.OwnershipTransferRequired =>
                OrganizationRoleManagementStatus.OwnershipTransferRequired,
            OrganizationRoleManagementDecision.RoleUnchanged =>
                OrganizationRoleManagementStatus.RoleUnchanged,
            OrganizationRoleManagementDecision.InvalidRole =>
                throw new InvalidOperationException("The service accepted an invalid role."),
            OrganizationRoleManagementDecision.Allowed =>
                throw new InvalidOperationException("An allowed role change cannot be rejected."),
            _ => throw new InvalidOperationException(
                $"Unsupported role-management decision: {decision}."),
        };
    }

    private static OrganizationRoleManagementResult.Rejection Reject(
        OrganizationRoleManagementStatus status)
    {
        return OrganizationRoleManagementResult.Rejected(status);
    }

    private sealed record MembershipSnapshot(
        Guid Id,
        Guid UserId,
        OrganizationRole Role);

    private sealed record RoleManagementContext(
        MembershipSnapshot? Actor,
        MembershipSnapshot? Target,
        OrganizationRoleManagementResult.Rejection? Rejection)
    {
        public static RoleManagementContext Loaded(
            MembershipSnapshot actor,
            MembershipSnapshot target)
        {
            return new(actor, target, Rejection: null);
        }

        public static RoleManagementContext Rejected(
            OrganizationRoleManagementStatus status)
        {
            return new(Actor: null, Target: null, Reject(status));
        }
    }
}
