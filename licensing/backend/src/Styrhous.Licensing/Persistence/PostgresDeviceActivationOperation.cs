using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresDeviceActivationOperation
{
    public static async Task<DeviceActivationResult> ExecuteAsync(
        LicensingDbContext dbContext,
        Guid userId,
        Guid seatId,
        DesktopInstallation installation,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        if (!await EfTransactionSerialization.TryClaimUserAsync(
                dbContext,
                userId,
                cancellationToken))
        {
            return DeviceActivationResult.SeatNotEligible(correlationId);
        }

        var seat = await EfTransactionSerialization.FindOwnedSeatAsync(
            dbContext,
            seatId,
            userId,
            cancellationToken);
        if (seat is null)
        {
            return DeviceActivationResult.SeatNotEligible(correlationId);
        }

        var entitlement = await SeatEntitlementQuery
            .SerializeBillingAccountAndResolveForSeatAsync(
            dbContext,
            seatId,
            seat.BillingAccountId,
            observedAt,
            cancellationToken);
        if (!entitlement.IsEligible)
        {
            return DeviceActivationResult.SeatNotEligible(correlationId);
        }

        var active = await dbContext.DeviceActivations
            .AsNoTracking()
            .Where(activation => activation.SeatId == seatId && activation.RevokedAt == null)
            .OrderBy(activation => activation.ActivatedAt)
            .ThenBy(activation => activation.Id)
            .ToListAsync(cancellationToken);
        var decision = DeviceActivationPolicy.Decide(
            active,
            installation.InstallationId,
            seat.DeviceLimit,
            observedAt);
        if (decision.Kind == DeviceActivationDecisionKind.AlreadyActive)
        {
            return DeviceActivationResult.AlreadyActive(
                correlationId,
                decision.ExistingActivation!,
                ActiveDevice.Summarize(active));
        }

        if (decision.Kind == DeviceActivationDecisionKind.LimitReached)
        {
            return DeviceActivationResult.LimitReached(
                correlationId,
                ActiveDevice.Summarize(active));
        }

        var revoked = decision.StaleActivation;
        if (revoked is not null)
        {
            dbContext.DeviceActivations.Attach(revoked);
            revoked.RevokeAsStale(observedAt);
            await DesktopSessionRevocation.RevokeForActivationsAsync(
                dbContext,
                [revoked.Id],
                observedAt,
                retainSessionRecords: true,
                cancellationToken);
            dbContext.AuditRecords.Add(
                AuditRecord.Create(
                    correlationId,
                    userId,
                    AuditAction.DeviceRevokedStale,
                    AuditTargetType.DeviceActivation,
                    revoked.Id,
                    observedAt));
        }

        var activation = DeviceActivation.Activate(
            seatId,
            userId,
            installation,
            observedAt);
        dbContext.DeviceActivations.Add(activation);
        dbContext.AuditRecords.Add(
            AuditRecord.Create(
                correlationId,
                userId,
                AuditAction.DeviceActivated,
                AuditTargetType.DeviceActivation,
                activation.Id,
                observedAt));

        var resultingActive = active
            .Where(candidate => candidate.RevokedAt is null)
            .Append(activation)
            .ToArray();
        return DeviceActivationResult.Activated(
            correlationId,
            activation,
            revoked,
            ActiveDevice.Summarize(resultingActive));
    }
}
