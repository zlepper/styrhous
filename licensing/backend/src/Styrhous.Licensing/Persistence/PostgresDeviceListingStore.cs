using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresDeviceListingStore(
    IDbContextFactory<LicensingDbContext> contextFactory)

{

    public async Task<DeviceListingResult> ListActiveAsync(
        Guid userId,
        Guid seatId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync(
            contextFactory,
            IsolationLevel.RepeatableRead,
            (dbContext, token) => ListWithinTransactionAsync(
                dbContext,
                userId,
                seatId,
                token),
            cancellationToken);
    }

    private static async Task<DeviceListingResult> ListWithinTransactionAsync(
        LicensingDbContext dbContext,
        Guid userId,
        Guid seatId,
        CancellationToken cancellationToken)
    {
        var seat = await dbContext.Seats
            .AsNoTracking()
            .SingleOrDefaultAsync(
                candidate => candidate.Id == seatId && candidate.AssignedUserId == userId,
                cancellationToken);
        if (seat is null)
        {
            return DeviceListingResult.SeatNotFound();
        }

        var activeDevices = await dbContext.DeviceActivations
            .AsNoTracking()
            .Where(
                activation => activation.SeatId == seatId
                    && activation.UserId == userId
                    && activation.RevokedAt == null)
            .ToArrayAsync(cancellationToken);
        return DeviceListingResult.Listed(seat, activeDevices);
    }
}
