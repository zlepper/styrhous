using Styrhous.Licensing.Application.Messaging;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingWebhookInboxStore(
    IDbContextFactory<LicensingDbContext> dbContextFactory,
    PostgresBackgroundWorkOutbox outbox)

{

    public async Task<BillingWebhookInboxStoreResult> ReceiveAsync(
        VerifiedBillingWebhookEvent webhookEvent,
        DateTimeOffset receivedAt,
        BillingWebhookInboxDisposition disposition,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(webhookEvent);
        if (!Enum.IsDefined(disposition))
        {
            throw new ArgumentOutOfRangeException(nameof(disposition));
        }

        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<BillingWebhookInboxStoreResult>(
            dbContext,
            async (transaction, token) =>
            {
                var existingId = await FindExistingIdAsync(
                    dbContext,
                    webhookEvent.ExternalEventId,
                    token);
                if (existingId is not null)
                {
                    return Duplicate(existingId.Value);
                }

                var inboxEvent = BillingWebhookEvent.Receive(
                    webhookEvent.ExternalEventId,
                    webhookEvent.EventType,
                    webhookEvent.Kind,
                    webhookEvent.OccurredAt,
                    receivedAt);
                if (disposition == BillingWebhookInboxDisposition.Ignored)
                {
                    inboxEvent.TryMarkProcessed(receivedAt);
                }

                dbContext.BillingWebhookEvents.Add(inboxEvent);
                try
                {

                    if (disposition == BillingWebhookInboxDisposition.PendingProcessing)
                    {
                        await outbox.EnqueueAsync(
                            dbContext,
                            BackgroundWorkReference.BillingWebhook(inboxEvent.Id, receivedAt),
                            token);
                    }

                    await dbContext.SaveChangesAsync(token);
                    await transaction.CommitAsync(token);
                    return new BillingWebhookInboxStoreResult(
                        BillingWebhookInboxStoreStatus.Received,
                        inboxEvent.Id);
                }
                catch (DbUpdateException exception) when (exception.IsUniqueViolation())
                {
                    await transaction.RollbackAsync(token);
                    await using var verificationContext =
                        await dbContextFactory.CreateDbContextAsync(token);
                    existingId = await FindExistingIdAsync(
                        verificationContext,
                        webhookEvent.ExternalEventId,
                        token);
                    if (existingId is not null)
                    {
                        return Duplicate(existingId.Value);
                    }

                    throw;
                }
            },
            cancellationToken);
    }

    private static Task<Guid?> FindExistingIdAsync(
        LicensingDbContext dbContext,
        string externalEventId,
        CancellationToken cancellationToken)
    {
        return dbContext.BillingWebhookEvents
            .AsNoTracking()
            .Where(webhookEvent => webhookEvent.ExternalEventId == externalEventId)
            .Select(webhookEvent => (Guid?)webhookEvent.Id)
            .SingleOrDefaultAsync(cancellationToken);
    }

    private static BillingWebhookInboxStoreResult Duplicate(Guid existingId)
    {
        return new(BillingWebhookInboxStoreStatus.Duplicate, existingId);
    }
}
