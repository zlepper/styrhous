using System.Data;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Styrhous.Licensing.Application.Desktop;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresDesktopDeviceAuthorizationStore(
    LicensingDbContext dbContext)
{

    public async Task<DesktopDeviceAuthorizationApproval> GetApprovalAsync(
        Guid userId,
        DesktopInstallation installation,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<DesktopDeviceAuthorizationApproval>(
            dbContext,
            IsolationLevel.RepeatableRead,
            async (transaction, token) =>
            {
                await PostgresReadChecks.EnsureUserExistsAsync(
                    dbContext,
                    userId,
                    token);
                var entitlements = (await SeatEntitlementQuery.LoadForUserAsync(
                        dbContext,
                        userId,
                        cancellationToken))
                    .Select(materialization => SeatEntitlement.Resolve(
                        materialization.Source,
                        observedAt))
                    .Where(entitlement => entitlement.IsEligible)
                    .ToArray();
                var eligibleSeatIds = entitlements.Select(entitlement => entitlement.SeatId).ToArray();
                var seatMetadata = await (
                        from seat in dbContext.Seats.AsNoTracking()
                        join organization in dbContext.Organizations.AsNoTracking()
                            on seat.BillingAccountId equals organization.BillingAccountId
                            into matchingOrganizations
                        from organization in matchingOrganizations.DefaultIfEmpty()
                        where eligibleSeatIds.Contains(seat.Id)
                        select new
                        {
                            seat.Id,
                            seat.DeviceLimit,
                            OrganizationName = organization == null ? null : organization.Name,
                        })
                    .ToDictionaryAsync(item => item.Id, cancellationToken);
                var active = await dbContext.DeviceActivations
                    .AsNoTracking()
                    .Where(activation => eligibleSeatIds.Contains(activation.SeatId)
                        && activation.RevokedAt == null)
                    .ToArrayAsync(cancellationToken);
                var activeBySeat = active.ToLookup(activation => activation.SeatId);

                var seats = entitlements
                    .Where(entitlement => seatMetadata.ContainsKey(entitlement.SeatId))
                    .Select(entitlement =>
                    {
                        var metadata = seatMetadata[entitlement.SeatId];
                        var devices = activeBySeat[entitlement.SeatId].ToArray();
                        var decision = DeviceActivationPolicy.Decide(
                            devices,
                            installation.InstallationId,
                            metadata.DeviceLimit,
                            observedAt);
                        return new DesktopDeviceAuthorizationSeat(
                            entitlement.SeatId,
                            entitlement.BillingAccountId,
                            metadata.OrganizationName ?? "Personal seat",
                            entitlement.State,
                            entitlement.ReasonCode,
                            metadata.DeviceLimit,
                            decision.Kind is not DeviceActivationDecisionKind.LimitReached,
                            ActiveDevice.Summarize(devices));
                    })
                    .ToArray();
                await transaction.CommitAsync(token);
                return new DesktopDeviceAuthorizationApproval(
                    DesktopDeviceAuthorizationReasonCodes.AwaitingApproval,
                    installation,
                    seats,
                    seats.Length is 1 ? seats[0].SeatId : null);
            },
            cancellationToken);
    }

    public async Task<DeviceActivationResult> ApproveAsync(
        Guid userId,
        Guid seatId,
        Guid authorizationId,
        DesktopInstallation installation,
        DateTimeOffset observedAt,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            return await ApproveWithinTransactionAsync(
                userId,
                seatId,
                authorizationId,
                installation,
                observedAt,
                correlationId,
                ownedTransaction: null,
                cancellationToken);
        }

        return await LicensingDbContextTransaction.ExecuteAsync<DeviceActivationResult>(
            dbContext,
            IsolationLevel.Serializable,
            (transaction, token) => ApproveWithinTransactionAsync(
                userId,
                seatId,
                authorizationId,
                installation,
                observedAt,
                correlationId,
                transaction,
                token),
            cancellationToken);
    }

    private async Task<DeviceActivationResult> ApproveWithinTransactionAsync(
        Guid userId,
        Guid seatId,
        Guid authorizationId,
        DesktopInstallation installation,
        DateTimeOffset observedAt,
        Guid correlationId,
        IDbContextTransaction? ownedTransaction,
        CancellationToken cancellationToken)
    {
        var result = await PostgresDeviceActivationOperation.ExecuteAsync(
            dbContext,
            userId,
            seatId,
            installation,
            observedAt,
            correlationId,
            cancellationToken);
        if (result.Status is DeviceActivationStatus.Activated
            or DeviceActivationStatus.AlreadyActive)
        {
            var activationId = result.ActivationId
                ?? throw new InvalidOperationException(
                    "An approved device authorization must have an activation.");
            var existing = await dbContext.DesktopDeviceSessions
                .SingleOrDefaultAsync(
                    session => session.AuthorizationId == authorizationId,
                    cancellationToken);
            if (existing is null)
            {
                dbContext.DesktopDeviceSessions.Add(
                    DesktopDeviceSession.Start(
                        activationId,
                        authorizationId,
                        observedAt));
            }
            else if (existing.ActivationId != activationId)
            {
                throw new InvalidOperationException(
                    "A desktop authorization cannot activate more than one device.");
            }
        }

        await dbContext.SaveChangesAsync(cancellationToken);
        if (ownedTransaction is not null)
        {
            await ownedTransaction.CommitAsync(cancellationToken);
        }

        return result;
    }
}
