using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingCheckoutStore(
    IDbContextFactory<LicensingDbContext> contextFactory)

{

    public async Task<BillingCheckoutPreparationResult> PrepareAsync(
        Guid actorUserId,
        Guid billingAccountId,
        BillingCadence cadence,
        int seatQuantity,
        Guid? retryOperationId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            BillingCheckoutPreparationResult>(
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
                    return Reject(BillingCheckoutPreparationStatus.BillingAccountNotFound);
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
                            BillingCheckoutPreparationStatus.BillingAccountNotFound,
                        BillingAccountManagementAuthorization.InsufficientPermission =>
                            BillingCheckoutPreparationStatus.InsufficientPermission,
                        _ => throw new InvalidOperationException(
                            "The billing-management authorization result is unsupported."),
                    });
                }

                var currentSubscription = await dbContext.CommercialSubscriptions.AsNoTracking().SingleOrDefaultAsync(
                        subscription => subscription.BillingAccountId == billingAccountId,
                        cancellationToken);
                if (!BillingCheckoutEligibility.AllowsPurchase(currentSubscription?.Status))
                {
                    return Reject(
                        BillingCheckoutPreparationStatus.SubscriptionAlreadyExists);
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
                        BillingCheckoutPreparationStatus.PersonalSeatQuantityInvalid,
                        requiredSeatQuantity);
                }

                var liveOperation = await dbContext.BillingOperations
                    .SingleOrDefaultAsync(
                        operation => operation.BillingAccountId == billingAccountId
                            && (operation.Status == BillingOperationStatus.Pending
                                || operation.Status
                                    == BillingOperationStatus.ProviderSessionCreated),
                        cancellationToken);

                if (retryOperationId is Guid retryId
                    && (liveOperation is null
                        || liveOperation.Id != retryId
                        || !Matches(
                            liveOperation,
                            actorUserId,
                            cadence,
                            seatQuantity)))
                {
                    await transaction.CommitAsync(cancellationToken);
                    return Reject(
                        BillingCheckoutPreparationStatus.BillingOperationNotFound);
                }

                if (liveOperation is not null
                    && observedAt >= liveOperation.RequireCheckoutExpiry())
                {
                    await transaction.CommitAsync(cancellationToken);
                    return BillingCheckoutPreparationResult.ReconciliationRequired(
                        liveOperation,
                        requiredSeatQuantity);
                }

                if (liveOperation is not null
                    && liveOperation.SeatQuantity < requiredSeatQuantity)
                {
                    await transaction.CommitAsync(cancellationToken);
                    return BillingCheckoutPreparationResult.CapacityChanged(
                        liveOperation,
                        requiredSeatQuantity);
                }

                if (seatQuantity < requiredSeatQuantity)
                {
                    await transaction.CommitAsync(cancellationToken);
                    return Reject(
                        BillingCheckoutPreparationStatus.SeatQuantityTooSmall,
                        requiredSeatQuantity);
                }

                if (retryOperationId is Guid operationId)
                {
                    await transaction.CommitAsync(cancellationToken);
                    return BillingCheckoutPreparationResult.Prepared(
                        liveOperation
                            ?? throw new InvalidOperationException(
                                $"Retry operation {operationId} was not loaded."),
                        requiredSeatQuantity);
                }

                if (liveOperation is not null)
                {
                    await transaction.CommitAsync(cancellationToken);
                    return Matches(liveOperation, actorUserId, cadence, seatQuantity)
                        ? BillingCheckoutPreparationResult.Prepared(
                            liveOperation,
                            requiredSeatQuantity)
                        : BillingCheckoutPreparationResult.InProgress(
                            liveOperation,
                            requiredSeatQuantity);
                }

                var operation = BillingOperation.StartInitialCheckout(
                    billingAccountId,
                    actorUserId,
                    cadence,
                    seatQuantity,
                    observedAt,
                    currentSubscription?.Snapshot());
                dbContext.BillingOperations.Add(operation);
                dbContext.AuditRecords.Add(
                    AuditRecord.Create(
                        operation.Id,
                        actorUserId,
                        AuditAction.BillingCheckoutStarted,
                        AuditTargetType.BillingOperation,
                        operation.Id,
                        observedAt));
                await dbContext.SaveChangesAsync(cancellationToken);
                await transaction.CommitAsync(cancellationToken);
                return BillingCheckoutPreparationResult.Prepared(
                    operation,
                    requiredSeatQuantity);
            },
            cancellationToken);
    }

    public Task<bool> RecordProviderSessionAsync(
        Guid operationId,
        string externalSessionId,
        DateTimeOffset recordedAt,
        CancellationToken cancellationToken)
    {
        return PostgresBillingOperationMutation.WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            operation =>
            {
                _ = operation.TryRecordProviderSession(
                    externalSessionId,
                    recordedAt);
            },
            cancellationToken);
    }

    public Task<bool> FailProviderSessionCreationAsync(
        Guid operationId,
        DateTimeOffset failedAt,
        CancellationToken cancellationToken)
    {
        return PostgresBillingOperationMutation.WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            operation =>
            {
                _ = operation.TryFailProviderSessionCreation(failedAt);
            },
            cancellationToken);
    }

    public Task<bool> ExpireProviderSessionAsync(
        Guid operationId,
        string externalSessionId,
        DateTimeOffset expiredAt,
        CancellationToken cancellationToken)
    {
        return PostgresBillingOperationMutation.WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            operation =>
            {
                _ = operation.TryExpireProviderSession(
                    externalSessionId,
                    expiredAt);
            },
            cancellationToken);
    }

    public async Task<bool> ApplySubscriptionAndCompleteProviderSessionAsync(
        Guid operationId,
        AuthoritativeCommercialSubscription subscription,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(subscription);
        return await PostgresBillingOperationMutation.WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                if (operation.BillingAccountId != subscription.BillingAccountId)
                {
                    throw new InvalidOperationException(
                        "The completed Checkout operation belongs to another billing account.");
                }

                await PostgresCommercialSubscriptionProjection.ApplyClaimedAsync(
                    operationContext,
                    operation.BillingAccountId,
                    subscription.Projection,
                    subscription.ProviderReadRevision,
                    subscription.ProviderSnapshotKind,
                    token,
                    operation.Id);
            },
            cancellationToken);
    }

    private static IQueryable<BillingCheckoutAccount> AccountForActor(
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
               where account.Id == billingAccountId
               select new BillingCheckoutAccount(
                   account.Id,
                   account.Kind,
                   account.PersonalOwnerUserId,
                   organization == null ? null : organization.Id,
                   membership == null ? null : membership.Role);
    }

    private static bool Matches(
        BillingOperation operation,
        Guid actorUserId,
        BillingCadence cadence,
        int seatQuantity)
    {
        return operation.ActorUserId == actorUserId
        && operation.Kind == BillingOperationKind.InitialCheckout
        && operation.Cadence == cadence
        && operation.SeatQuantity == seatQuantity;
    }

    private static BillingCheckoutPreparationResult Reject(
        BillingCheckoutPreparationStatus status,
        int? requiredSeatQuantity = null)
    {
        return BillingCheckoutPreparationResult.Rejected(status, requiredSeatQuantity);
    }

    private sealed record BillingCheckoutAccount(
        Guid Id,
        BillingAccountKind Kind,
        Guid? PersonalOwnerUserId,
        Guid? OrganizationId,
        OrganizationRole? OrganizationRole);
}
