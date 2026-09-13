using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Persistence;

public sealed class NativeOutboxUpgrade(
    IDbContextFactory<LicensingDbContext> dbContextFactory,
    PostgresBackgroundWorkOutbox outbox,
    TimeProvider timeProvider)
{
    public async Task EnqueueLegacyWorkAsync(CancellationToken cancellationToken)
    {
        while (true)
        {
            if (await LicensingDbContextTransaction.ExecuteAsync(
                    dbContextFactory,
                    IsolationLevel.Serializable,
                    (dbContext, token) => EnqueueBatchAsync(dbContext, token),
                    cancellationToken))
            {
                return;
            }
        }
    }

    private async Task<bool> EnqueueBatchAsync(
        LicensingDbContext dbContext,
        CancellationToken cancellationToken)
    {
        const int batchSize = 100;
        var observedAt = timeProvider.GetUtcNow();
        var deliveries = await dbContext.OutboxMessages
            .Where(message => !message.NativeOutboxEnqueued
                && message.DeliveredAt == null && message.DiscardedAt == null
                && (message.NotAfter == null || message.NotAfter > observedAt))
            .OrderBy(message => message.Id).Take(batchSize).ToArrayAsync(cancellationToken);
        var webhooks = await dbContext.BillingWebhookEvents
            .Where(message => !message.NativeOutboxEnqueued && message.ProcessedAt == null)
            .OrderBy(message => message.Id).Take(batchSize).ToArrayAsync(cancellationToken);
        foreach (var message in deliveries)
        {
            var work = message.MessageType switch
            {
                OutboxMessageTypes.OrganizationInvitationDelivery =>
                    BackgroundWorkReference.OrganizationInvitationDelivery(message.Id, message.OccurredAt),
                OutboxMessageTypes.InfrastructureSmokeProbe =>
                    BackgroundWorkReference.InfrastructureSmokeProbe(message.Id, message.OccurredAt),
                _ => throw new InvalidOperationException("Unsupported legacy background-work type."),
            };
            await outbox.EnqueueAsync(dbContext, work, cancellationToken);
        }
        foreach (var webhook in webhooks)
        {
            await outbox.EnqueueAsync(dbContext,
                BackgroundWorkReference.BillingWebhook(webhook.Id, webhook.ReceivedAt), cancellationToken);
        }
        await dbContext.SaveChangesAsync(cancellationToken);
        return deliveries.Length < batchSize && webhooks.Length < batchSize;
    }
}
