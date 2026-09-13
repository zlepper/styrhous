using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Domain.Auditing;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresDeviceRevocationStore(
    IDbContextFactory<LicensingDbContext> contextFactory)

{

    public async Task<DeviceRevocationResult> RevokeAsync(
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await EfConcurrencyRetry.ExecuteAsync(
            () => LicensingDbContextTransaction.ExecuteAsync(
                    contextFactory,
                    IsolationLevel.Serializable,
                    (operationContext, token) => TryRevokeAsync(
                    operationContext,
                    userId,
                    activationId,
                    observedAt,
                    correlationId,
                    token),
                    cancellationToken));
    }

    private static async Task<DeviceRevocationResult> TryRevokeAsync(
        LicensingDbContext dbContext,
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        if (!await EfTransactionSerialization.TryClaimUserAsync(
                dbContext,
                userId,
                cancellationToken))
        {
            return DeviceRevocationResult.DeviceNotActive(correlationId);
        }

        var ownedActivation = await PostgresDeviceActivations
            .FindOwnedActiveAsync(
            dbContext,
            userId,
            activationId,
            cancellationToken);
        if (ownedActivation is null)
        {
            return DeviceRevocationResult.DeviceNotActive(correlationId);
        }

        var activation = ownedActivation.Activation;
        activation.RevokeManually(observedAt);
        await DesktopSessionRevocation.RevokeForActivationsAsync(
            dbContext,
            [activation.Id],
            observedAt,
            retainSessionRecords: true,
            cancellationToken);
        dbContext.AuditRecords.Add(
            AuditRecord.Create(
                correlationId,
                userId,
                AuditAction.DeviceRevokedManual,
                AuditTargetType.DeviceActivation,
                activation.Id,
                activation.RevokedAt!.Value));
        await dbContext.SaveChangesAsync(cancellationToken);
        return DeviceRevocationResult.Revoked(correlationId, activation.Id);
    }
}
