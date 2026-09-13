using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresDeviceEntitlementCheckStore(
    LicensingDbContext dbContext)
{

    public async Task<DeviceEntitlementCheckResult> CheckAsync(
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            return await CheckWithinTransactionAsync(
                dbContext,
                userId,
                activationId,
                observedAt,
                ownedTransaction: null,
                cancellationToken);
        }

        return await dbContext.Database.CreateExecutionStrategy().ExecuteAsync(async () =>
        {
            await using var transaction = await dbContext.Database.BeginTransactionAsync(
                cancellationToken);
            return await CheckWithinTransactionAsync(
                dbContext,
                userId,
                activationId,
                observedAt,
                transaction,
                cancellationToken);
        });
    }

    private static async Task<DeviceEntitlementCheckResult> CheckWithinTransactionAsync(
        LicensingDbContext operationContext,
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt,
        IDbContextTransaction? ownedTransaction,
        CancellationToken cancellationToken)
    {
        if (!await EfTransactionSerialization.TryClaimUserAsync(
                operationContext,
                userId,
                cancellationToken))
        {
            return DeviceEntitlementCheckResult.DeviceNotActive();
        }

        var ownedActivation = await PostgresDeviceActivations
            .FindOwnedActiveAsync(
            operationContext,
            userId,
            activationId,
            cancellationToken);
        if (ownedActivation is null)
        {
            return DeviceEntitlementCheckResult.DeviceNotActive();
        }

        var activation = ownedActivation.Activation;
        var entitlement = await SeatEntitlementQuery
            .SerializeBillingAccountAndResolveForSeatAsync(
            operationContext,
            activation.SeatId,
            ownedActivation.BillingAccountId,
            observedAt,
            cancellationToken);
        if (!entitlement.IsEligible)
        {
            return DeviceEntitlementCheckResult.From(entitlement, activation);
        }

        if (activation.RecordSuccessfulEntitlementCheck(observedAt))
        {
            await operationContext.SaveChangesAsync(cancellationToken);
        }

        if (ownedTransaction is not null)
        {
            await ownedTransaction.CommitAsync(cancellationToken);
        }

        return DeviceEntitlementCheckResult.From(entitlement, activation);
    }
}
