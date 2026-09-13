using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingWebhookProcessingStore(
    IDbContextFactory<LicensingDbContext> dbContextFactory)

{

    public async Task<BillingWebhookProcessingClaimResult> TryAcquireAsync(
        Guid inboxEventId,
        Guid leaseId,
        DateTimeOffset acquiredAt,
        DateTimeOffset expiresAt,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var webhookEvent = await dbContext.BillingWebhookEvents.SingleOrDefaultAsync(
            candidate => candidate.Id == inboxEventId,
            cancellationToken);
        if (webhookEvent is null)
        {
            return Result(BillingWebhookProcessingClaimStatus.NotFound);
        }

        if (webhookEvent.ProcessedAt is not null)
        {
            return Result(BillingWebhookProcessingClaimStatus.AlreadyProcessed);
        }

        if (!webhookEvent.TryAcquireProcessingLease(leaseId, acquiredAt, expiresAt))
        {
            return Result(BillingWebhookProcessingClaimStatus.Busy);
        }

        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
        }
        catch (DbUpdateConcurrencyException)
        {
            return Result(BillingWebhookProcessingClaimStatus.Busy);
        }

        return new BillingWebhookProcessingClaimResult(
            BillingWebhookProcessingClaimStatus.Acquired,
            new BillingWebhookProcessingClaim(
                webhookEvent.Id,
                leaseId,
                webhookEvent.ExternalEventId,
                webhookEvent.Kind));
    }

    public async Task<bool> CompleteAsync(
        Guid inboxEventId,
        Guid leaseId,
        DateTimeOffset processedAt,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var webhookEvent = await dbContext.BillingWebhookEvents.SingleOrDefaultAsync(
            candidate => candidate.Id == inboxEventId,
            cancellationToken);
        if (webhookEvent is null
            || !webhookEvent.TryMarkProcessed(leaseId, processedAt))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    public async Task<bool> ReleaseAsync(
        Guid inboxEventId,
        Guid leaseId,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var webhookEvent = await dbContext.BillingWebhookEvents.SingleOrDefaultAsync(
            candidate => candidate.Id == inboxEventId,
            cancellationToken);
        if (webhookEvent is null
            || !webhookEvent.TryReleaseProcessingLease(leaseId))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    private static async Task<bool> TrySaveAsync(
        LicensingDbContext dbContext,
        CancellationToken cancellationToken)
    {
        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
            return true;
        }
        catch (DbUpdateConcurrencyException)
        {
            return false;
        }
    }

    private static BillingWebhookProcessingClaimResult Result(
        BillingWebhookProcessingClaimStatus status)
    {
        return new(status, Claim: null);
    }

}
