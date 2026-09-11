using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Persistence;

internal sealed record OwnedDeviceActivation(
    DeviceActivation Activation,
    Guid BillingAccountId);

internal static class PostgresDeviceActivations
{
    public static async Task<OwnedDeviceActivation?> FindOwnedActiveAsync(
        LicensingDbContext dbContext,
        Guid userId,
        Guid activationId,
        CancellationToken cancellationToken)
    {
        var activation = await dbContext.DeviceActivations.SingleOrDefaultAsync(
            candidate => candidate.Id == activationId
                && candidate.UserId == userId
                && candidate.RevokedAt == null,
            cancellationToken);
        if (activation is null)
        {
            return null;
        }

        var seat = await EfTransactionSerialization.FindOwnedSeatAsync(
            dbContext,
            activation.SeatId,
            userId,
            cancellationToken);
        return seat is null
            ? null
            : new OwnedDeviceActivation(activation, seat.BillingAccountId);
    }
}
