using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal static class EfTransactionSerialization
{
    public static async Task<bool> TryClaimBillingAccountAsync(
        LicensingDbContext dbContext,
        Guid billingAccountId,
        CancellationToken cancellationToken)
    {
        RequireTransaction(dbContext);
        return await dbContext.BillingAccounts
            .Where(account => account.Id == billingAccountId)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    account => account.ConcurrencyVersion,
                    account => account.ConcurrencyVersion + 1),
                cancellationToken) == 1;
    }

    public static async Task<bool> TryClaimUserAsync(
        LicensingDbContext dbContext,
        Guid userId,
        CancellationToken cancellationToken)
    {
        RequireTransaction(dbContext);
        return await dbContext.UserAccounts
            .Where(user => user.Id == userId)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    user => user.ConcurrencyVersion,
                    user => user.ConcurrencyVersion + 1),
                cancellationToken) == 1;
    }

    public static async Task<Seat?> FindOwnedSeatAsync(
        LicensingDbContext dbContext,
        Guid seatId,
        Guid userId,
        CancellationToken cancellationToken)
    {
        RequireTransaction(dbContext);
        return await dbContext.Seats
            .AsNoTracking()
            .SingleOrDefaultAsync(
                seat => seat.Id == seatId && seat.AssignedUserId == userId,
                cancellationToken);
    }

    public static async Task<OrganizationInvitation?>
        FindOrganizationInvitationAsync(
            LicensingDbContext dbContext,
            Guid organizationId,
            Guid invitationId,
            CancellationToken cancellationToken)
    {
        RequireTransaction(dbContext);
        var tracked = dbContext.OrganizationInvitations.Local.SingleOrDefault(
            invitation => invitation.Id == invitationId
                && invitation.OrganizationId == organizationId);
        if (tracked is not null)
        {
            await dbContext.Entry(tracked).ReloadAsync(cancellationToken);
            return dbContext.Entry(tracked).State == EntityState.Detached
                ? null
                : tracked;
        }

        return await dbContext.OrganizationInvitations.SingleOrDefaultAsync(
            invitation => invitation.Id == invitationId
                && invitation.OrganizationId == organizationId,
            cancellationToken);
    }

    public static async Task<Organization?> FindOrganizationAndClaimAsync(
        LicensingDbContext dbContext,
        Guid organizationId,
        CancellationToken cancellationToken)
    {
        RequireTransaction(dbContext);
        var claimedCount = await dbContext.Organizations
            .Where(organization => organization.Id == organizationId)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    organization => organization.ConcurrencyVersion,
                    organization => organization.ConcurrencyVersion + 1),
                cancellationToken);
        return claimedCount == 1
            ? await dbContext.Organizations
                .AsNoTracking()
                .SingleAsync(
                    organization => organization.Id == organizationId,
                    cancellationToken)
            : null;
    }

    private static void RequireTransaction(LicensingDbContext dbContext)
    {
        if (dbContext.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "EF transaction serialization requires an active transaction.");
        }
    }
}
