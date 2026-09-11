using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingSeatQuantityStore(
    IDbContextFactory<LicensingDbContext> contextFactory)

{

    public async Task<BillingSeatQuantityPreparationResult> PrepareAsync(
        Guid actorUserId,
        Guid billingAccountId,
        int seatQuantity,
        Guid? retryOperationId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            BillingSeatQuantityPreparationResult>(
            dbContext,
            async (transaction, token) =>
            {
                await PostgresReadChecks.EnsureUserExistsAsync(
                    dbContext,
                    actorUserId,
                    cancellationToken);
                if (!await EfTransactionSerialization.TryClaimBillingAccountAsync(
                        dbContext,
                        billingAccountId,
                        cancellationToken))
                {
                    return Reject(BillingSeatQuantityPreparationStatus.BillingAccountNotFound);
                }

                var account = await AccountForActor(dbContext, actorUserId, billingAccountId)
                    .SingleAsync(cancellationToken);
                var authorization = BillingAccountManagementPolicy.Authorize(
                    actorUserId,
                    account.Kind,
                    account.PersonalOwnerUserId,
                    account.OrganizationRole);
                if (authorization != BillingAccountManagementAuthorization.Authorized)
                {
                    return Reject(authorization switch
                    {
                        BillingAccountManagementAuthorization.BillingAccountNotFound =>
                            BillingSeatQuantityPreparationStatus.BillingAccountNotFound,
                        BillingAccountManagementAuthorization.InsufficientPermission =>
                            BillingSeatQuantityPreparationStatus.InsufficientPermission,
                        _ => throw new InvalidOperationException(
                            "The billing-management authorization result is unsupported."),
                    });
                }

                if (account.ExternalSubscriptionId is null)
                {
                    return Reject(BillingSeatQuantityPreparationStatus.SubscriptionNotFound);
                }

                var requiredSeatQuantity = await PostgresBillingSeatRequirement.ResolveAsync(
                    dbContext,
                    account.Kind,
                    account.OrganizationId,
                    account.Id,
                    observedAt,
                    cancellationToken);
                if (account.Kind == BillingAccountKind.Personal && seatQuantity != 1)
                {
                    return Reject(
                        BillingSeatQuantityPreparationStatus.PersonalSeatQuantityInvalid,
                        requiredSeatQuantity);
                }

                BillingOperation? retryOperation = null;
                if (retryOperationId is Guid retryId)
                {
                    retryOperation = await dbContext.BillingOperations.SingleOrDefaultAsync(
                        operation => operation.Id == retryId
                            && operation.BillingAccountId == billingAccountId,
                        cancellationToken);
                    if (retryOperation is null || !Matches(retryOperation, seatQuantity))
                    {
                        return Reject(BillingSeatQuantityPreparationStatus.BillingOperationNotFound);
                    }

                    if (retryOperation.Status is BillingOperationStatus.Completed
                        or BillingOperationStatus.Failed)
                    {
                        await transaction.CommitAsync(cancellationToken);
                        return BillingSeatQuantityPreparationResult.Terminal(
                            retryOperation,
                            account.SeatQuantity!.Value,
                            requiredSeatQuantity);
                    }
                }

                var liveOperation = await dbContext.BillingOperations.SingleOrDefaultAsync(
                    operation => operation.BillingAccountId == billingAccountId
                        && (operation.Status == BillingOperationStatus.Pending
                            || operation.Status == BillingOperationStatus.ProviderSessionCreated),
                    cancellationToken);
                if (retryOperation is not null
                    && (liveOperation is null || liveOperation.Id != retryOperation.Id))
                {
                    return Reject(BillingSeatQuantityPreparationStatus.BillingOperationNotFound);
                }

                if (liveOperation is not null)
                {
                    if (liveOperation.Kind != BillingOperationKind.SeatQuantityChange)
                    {
                        return Reject(BillingSeatQuantityPreparationStatus.SubscriptionInactive);
                    }

                    if (liveOperation.SeatQuantity < requiredSeatQuantity)
                    {
                        return Reject(
                            BillingSeatQuantityPreparationStatus.SeatQuantityTooSmall,
                            requiredSeatQuantity);
                    }

                    if (retryOperationId is not null
                        || Matches(liveOperation, seatQuantity))
                    {
                        await transaction.CommitAsync(cancellationToken);
                        return BillingSeatQuantityPreparationResult.Prepared(
                            liveOperation,
                            account.ExternalSubscriptionId,
                            requiredSeatQuantity);
                    }

                    await transaction.CommitAsync(cancellationToken);
                    return BillingSeatQuantityPreparationResult.InProgress(
                        liveOperation,
                        requiredSeatQuantity);
                }

                if (!CommercialEntitlement.Resolve(
                        account.SubscriptionStatus!.Value,
                        account.CancelAtPeriodEnd!.Value,
                        account.CurrentPeriodStartedAt!.Value,
                        account.CurrentPeriodEndsAt!.Value,
                        observedAt)
                    .IsEligible)
                {
                    return Reject(BillingSeatQuantityPreparationStatus.SubscriptionInactive);
                }

                var currentSeatQuantity = account.SeatQuantity!.Value;
                if (seatQuantity == currentSeatQuantity)
                {
                    return Reject(BillingSeatQuantityPreparationStatus.SeatQuantityUnchanged);
                }

                if (seatQuantity < requiredSeatQuantity)
                {
                    return Reject(
                        BillingSeatQuantityPreparationStatus.SeatQuantityTooSmall,
                        requiredSeatQuantity);
                }

                var operation = BillingOperation.StartSeatQuantityChange(
                    billingAccountId,
                    actorUserId,
                    currentSeatQuantity,
                    seatQuantity,
                    observedAt);
                dbContext.BillingOperations.Add(operation);
                dbContext.AuditRecords.Add(
                    AuditRecord.Create(
                        operation.Id,
                        actorUserId,
                        AuditAction.BillingSeatQuantityChangeStarted,
                        AuditTargetType.BillingOperation,
                        operation.Id,
                        observedAt));
                await dbContext.SaveChangesAsync(cancellationToken);
                await transaction.CommitAsync(cancellationToken);
                return BillingSeatQuantityPreparationResult.Prepared(
                    operation,
                    account.ExternalSubscriptionId,
                    requiredSeatQuantity);
            },
            cancellationToken);
    }

    public Task<BillingSeatQuantityResolutionResult?> RejectProviderMutationAsync(
        Guid operationId,
        DateTimeOffset rejectedAt,
        CancellationToken cancellationToken)
    {
        return PostgresBillingOperationMutation
            .WithBillingAccountLockAsync<BillingSeatQuantityResolutionResult?>(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                var authoritativeSubscription =
                    await LoadAuthoritativeSubscriptionAsync(operationContext, operation, token);
                if (operation.Status == BillingOperationStatus.Pending)
                {
                    var resolvedAt = LatestOf(
                        operation.CreatedAt,
                        authoritativeSubscription.ProjectedAt,
                        rejectedAt);
                    switch (ClassifyAuthoritativeState(
                        operation,
                        authoritativeSubscription))
                    {
                        case AuthoritativeSeatQuantityState.Target:
                            _ = operation.TryCompleteSeatQuantityChange(resolvedAt);
                            break;
                        case AuthoritativeSeatQuantityState.Previous:
                            _ = operation.TryFailSeatQuantityChange(
                                SeatQuantityChangeOutcome.ProviderRejected,
                                resolvedAt);
                            break;
                        case AuthoritativeSeatQuantityState.Superseding:
                            _ = operation.TryFailSeatQuantityChange(
                                SeatQuantityChangeOutcome.Superseded,
                                resolvedAt);
                            break;
                        default:
                            throw new InvalidOperationException(
                                "The authoritative seat-quantity state is invalid.");
                    }
                }
                else if (operation.Status is not BillingOperationStatus.Completed
                    and not BillingOperationStatus.Failed)
                {
                    throw new InvalidOperationException(
                        "The seat-quantity operation has an invalid rejection status.");
                }

                return new BillingSeatQuantityResolutionResult(
                    operation.Status,
                    operation.RequireSeatQuantityOutcome(),
                    authoritativeSubscription.SeatQuantity);
            },
            missingResult: null,
            cancellationToken);
    }

    public async Task<BillingSeatQuantityMutationPreparationResult?>
        ApplyObservationAndPrepareProviderMutationAsync(
        Guid operationId,
        AuthoritativeCommercialSubscription subscription,
        CancellationToken cancellationToken)
    {
        return await PostgresBillingOperationMutation
            .WithBillingAccountLockAsync<BillingSeatQuantityMutationPreparationResult?>(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                var authoritativeSubscription =
                    await ApplyAuthoritativeSubscriptionAsync(
                        operationContext,
                        operation,
                        subscription,
                        token);
                ResolvePendingObservation(
                    operation,
                    authoritativeSubscription);
                EnsureResolvableStatus(operation);

                return new BillingSeatQuantityMutationPreparationResult(
                    operation.Status,
                    operation.SeatQuantityOutcome,
                    operation.ProviderMutationReplayStartedAt,
                    authoritativeSubscription.ProjectedAt,
                    authoritativeSubscription.SeatQuantity);
            },
            missingResult: null,
            cancellationToken);
    }

    public async Task<BillingSeatQuantityResolutionResult?>
        ApplySubscriptionAndResolveAsync(
        Guid operationId,
        AuthoritativeCommercialSubscription subscription,
        DateTimeOffset resolvedAt,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(subscription);

        return await PostgresBillingOperationMutation
            .WithBillingAccountLockAsync<BillingSeatQuantityResolutionResult?>(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                var authoritativeSubscription =
                    await ApplyAuthoritativeSubscriptionAsync(
                        operationContext,
                        operation,
                        subscription,
                        token);
                ResolvePending(
                    operation,
                    authoritativeSubscription,
                    resolvedAt,
                    startReplayForPrevious: false);
                EnsureResolvableStatus(operation);

                return new BillingSeatQuantityResolutionResult(
                    operation.Status,
                    operation.SeatQuantityOutcome,
                    authoritativeSubscription.SeatQuantity);
            },
            missingResult: null,
            cancellationToken);
    }

    public async Task<BillingSeatQuantityResolutionResult?>
        ApplyProviderMutationIfObservationCurrentAsync(
        Guid operationId,
        AuthoritativeCommercialSubscription authorizedObservation,
        Func<CancellationToken, Task<AuthoritativeCommercialSubscription>>
            applyProviderMutation,
        DateTimeOffset resolvedAt,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(authorizedObservation);
        ArgumentNullException.ThrowIfNull(applyProviderMutation);

        return await PostgresBillingOperationMutation
            .WithBillingAccountLockAsync<BillingSeatQuantityResolutionResult?>(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                if (operation.BillingAccountId != authorizedObservation.BillingAccountId)
                {
                    throw new InvalidOperationException(
                        "The seat-quantity mutation belongs to another billing account.");
                }

                var authoritativeSubscription =
                    await LoadAuthoritativeSubscriptionAsync(operationContext, operation, token);
                if (operation.Status == BillingOperationStatus.Pending
                    && authoritativeSubscription.MatchesProviderSnapshot(
                        authorizedObservation.Projection,
                        authorizedObservation.ProviderReadRevision,
                        authorizedObservation.ProviderSnapshotKind))
                {
                    var appliedSubscription = await applyProviderMutation(token);
                    authoritativeSubscription =
                        await ApplyAuthoritativeSubscriptionAsync(
                        operationContext,
                            operation,
                            appliedSubscription,
                            token);
                }

                ResolvePending(
                    operation,
                    authoritativeSubscription,
                    resolvedAt,
                    startReplayForPrevious: false);
                EnsureResolvableStatus(operation);
                return new BillingSeatQuantityResolutionResult(
                    operation.Status,
                    operation.SeatQuantityOutcome,
                    authoritativeSubscription.SeatQuantity);
            },
            missingResult: null,
            cancellationToken);
    }

    private static async Task<CommercialSubscription> ApplyAuthoritativeSubscriptionAsync(
        LicensingDbContext dbContext,
        BillingOperation operation,
        AuthoritativeCommercialSubscription subscription,
        CancellationToken cancellationToken)
    {
        if (operation.BillingAccountId != subscription.BillingAccountId)
        {
            throw new InvalidOperationException(
                "The authoritative subscription belongs to another billing account.");
        }

        await PostgresCommercialSubscriptionProjection.ApplyClaimedAsync(
            dbContext,
            operation.BillingAccountId,
            subscription.Projection,
            subscription.ProviderReadRevision,
            subscription.ProviderSnapshotKind,
            cancellationToken);
        return dbContext.CommercialSubscriptions.Local.Single(
            candidate => candidate.BillingAccountId == operation.BillingAccountId);
    }

    private static async Task<CommercialSubscription> LoadAuthoritativeSubscriptionAsync(
        LicensingDbContext dbContext,
        BillingOperation operation,
        CancellationToken cancellationToken)
    {
        var subscription = dbContext.CommercialSubscriptions.Local.SingleOrDefault(
            candidate => candidate.BillingAccountId == operation.BillingAccountId);
        if (subscription is not null)
        {
            await dbContext.Entry(subscription).ReloadAsync(cancellationToken);
            if (dbContext.Entry(subscription).State == EntityState.Detached)
            {
                subscription = null;
            }
        }

        return subscription ?? await dbContext.CommercialSubscriptions.SingleAsync(
            candidate => candidate.BillingAccountId == operation.BillingAccountId,
            cancellationToken);
    }

    private static void ResolvePendingObservation(
        BillingOperation operation,
        CommercialSubscription subscription)
    {
        if (operation.Status != BillingOperationStatus.Pending)
        {
            return;
        }

        ResolvePending(
            operation,
            subscription,
            subscription.ProjectedAt,
            startReplayForPrevious: true);
    }

    private static void ResolvePending(
        BillingOperation operation,
        CommercialSubscription subscription,
        DateTimeOffset resolvedAt,
        bool startReplayForPrevious)
    {
        if (operation.Status != BillingOperationStatus.Pending)
        {
            return;
        }

        var effectiveResolvedAt = LatestOf(
            operation.CreatedAt,
            subscription.ProjectedAt,
            resolvedAt);
        switch (ClassifyAuthoritativeState(operation, subscription))
        {
            case AuthoritativeSeatQuantityState.Target:
                _ = operation.TryCompleteSeatQuantityChange(effectiveResolvedAt);
                break;
            case AuthoritativeSeatQuantityState.Previous when startReplayForPrevious:
                _ = operation.TryStartSeatQuantityProviderMutationReplay(
                    subscription.ProjectedAt);
                break;
            case AuthoritativeSeatQuantityState.Previous:
                break;
            case AuthoritativeSeatQuantityState.Superseding:
                _ = operation.TryFailSeatQuantityChange(
                    SeatQuantityChangeOutcome.Superseded,
                    effectiveResolvedAt);
                break;
            default:
                throw new InvalidOperationException(
                    "The authoritative seat-quantity state is invalid.");
        }
    }

    private static AuthoritativeSeatQuantityState ClassifyAuthoritativeState(
        BillingOperation operation,
        CommercialSubscription subscription)
    {
        if (!IsEligible(subscription))
        {
            return AuthoritativeSeatQuantityState.Superseding;
        }

        if (subscription.SeatQuantity == operation.SeatQuantity)
        {
            return AuthoritativeSeatQuantityState.Target;
        }

        return subscription.SeatQuantity == operation.RequirePreviousSeatQuantity()
            ? AuthoritativeSeatQuantityState.Previous
            : AuthoritativeSeatQuantityState.Superseding;
    }

    private static bool IsEligible(CommercialSubscription subscription)
    {
        return CommercialEntitlement.Resolve(
                subscription.Status,
                subscription.CancelAtPeriodEnd,
                subscription.CurrentPeriodStartedAt,
                subscription.CurrentPeriodEndsAt,
                subscription.ProjectedAt)
            .IsEligible;
    }

    private static DateTimeOffset LatestOf(
        DateTimeOffset first,
        DateTimeOffset second,
        DateTimeOffset third)
    {
        return first >= second
            ? first >= third ? first : third
            : second >= third ? second : third;
    }

    private static DateTimeOffset LaterOf(
        DateTimeOffset first,
        DateTimeOffset second)
    {
        return first >= second ? first : second;
    }

    private static void EnsureResolvableStatus(BillingOperation operation)
    {
        if (operation.Status is not BillingOperationStatus.Pending
            and not BillingOperationStatus.Completed
            and not BillingOperationStatus.Failed)
        {
            throw new InvalidOperationException(
                "The seat-quantity operation has an invalid resolution status.");
        }
    }

    private static IQueryable<BillingSeatQuantityAccount> AccountForActor(
        LicensingDbContext dbContext,
        Guid actorUserId,
        Guid billingAccountId)
    {
        return from account in dbContext.BillingAccounts.AsNoTracking()
               join organization in dbContext.Organizations.AsNoTracking()
                   on account.Id equals organization.BillingAccountId into organizations
               from organization in organizations.DefaultIfEmpty()
               join membership in dbContext.OrganizationMemberships
                       .AsNoTracking()
                       .Where(candidate => candidate.UserId == actorUserId)
                   on organization.Id equals membership.OrganizationId into memberships
               from membership in memberships.DefaultIfEmpty()
               join subscription in dbContext.CommercialSubscriptions.AsNoTracking()
                   on account.Id equals subscription.BillingAccountId into subscriptions
               from subscription in subscriptions.DefaultIfEmpty()
               where account.Id == billingAccountId
               select new BillingSeatQuantityAccount(
                   account.Id,
                   account.Kind,
                   account.PersonalOwnerUserId,
                   organization == null ? null : organization.Id,
                   membership == null ? null : membership.Role,
                   subscription == null ? null : subscription.ExternalSubscriptionId,
                   subscription == null ? null : subscription.Status,
                   subscription == null ? null : subscription.SeatQuantity,
                   subscription == null ? null : subscription.CancelAtPeriodEnd,
                   subscription == null ? null : subscription.CurrentPeriodStartedAt,
                   subscription == null ? null : subscription.CurrentPeriodEndsAt);
    }

    private static bool Matches(
        BillingOperation operation,
        int seatQuantity)
    {
        return operation.Kind == BillingOperationKind.SeatQuantityChange
        && operation.SeatQuantity == seatQuantity;
    }

    private static BillingSeatQuantityPreparationResult Reject(
        BillingSeatQuantityPreparationStatus status,
        int? requiredSeatQuantity = null)
    {
        return BillingSeatQuantityPreparationResult.Rejected(status, requiredSeatQuantity);
    }

    private sealed record BillingSeatQuantityAccount(
        Guid Id,
        BillingAccountKind Kind,
        Guid? PersonalOwnerUserId,
        Guid? OrganizationId,
        OrganizationRole? OrganizationRole,
        string? ExternalSubscriptionId,
        CommercialSubscriptionStatus? SubscriptionStatus,
        int? SeatQuantity,
        bool? CancelAtPeriodEnd,
        DateTimeOffset? CurrentPeriodStartedAt,
        DateTimeOffset? CurrentPeriodEndsAt);

    private enum AuthoritativeSeatQuantityState
    {
        Target,
        Previous,
        Superseding,
    }
}
