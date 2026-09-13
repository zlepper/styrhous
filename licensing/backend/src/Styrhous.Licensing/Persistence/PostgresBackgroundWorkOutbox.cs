using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Npgsql;
using Rebus.Messages;
using Rebus.PostgreSql;
using Rebus.PostgreSql.Outbox;
using Rebus.Transport;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBackgroundWorkOutbox(string queueName)
{
    internal const string TableName = "RebusOutbox";

    public async Task EnqueueAsync(
        LicensingDbContext dbContext,
        BackgroundWorkReference work,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var transaction = dbContext.Database.CurrentTransaction
            ?? throw new InvalidOperationException("Background work must join the business transaction.");
        using var connection = new DbConnectionWrapper(
            (NpgsqlConnection)dbContext.Database.GetDbConnection(),
            (NpgsqlTransaction)transaction.GetDbTransaction(),
            managedExternally: true);
        var storage = new PostgreSqlOutboxStorage(
            _ => connection,
            Rebus.PostgreSql.TableName.Parse(TableName));
        var message = MessageFor(work);
        var headers = new Dictionary<string, string>
        {
            [Headers.MessageId] = Guid.CreateVersion7().ToString(),
            [Headers.Type] = message.GetType().AssemblyQualifiedName!,
            [Headers.ContentType] = "application/json;charset=utf-8",
            [Headers.CorrelationId] = work.WorkId.ToString(),
        };
        var envelope = new TransportMessage(
            headers,
            JsonSerializer.SerializeToUtf8Bytes(message, message.GetType()));
        await storage.Save([new OutgoingTransportMessage(envelope, queueName)], connection);
        if (work.Kind == BackgroundWorkKind.BillingWebhook)
        {
            dbContext.BillingWebhookEvents.Local.Single(message => message.Id == work.WorkId)
                .NativeOutboxEnqueued = true;
        }
        else
        {
            dbContext.OutboxMessages.Local.Single(message => message.Id == work.WorkId)
                .NativeOutboxEnqueued = true;
        }
    }

    private static object MessageFor(BackgroundWorkReference work)
    {
        return work.Kind switch
        {
            BackgroundWorkKind.OrganizationInvitationDelivery =>
                new OrganizationInvitationDeliveryMessage(work.WorkId),
            BackgroundWorkKind.InfrastructureSmokeProbe =>
                new InfrastructureSmokeProbeMessage(work.WorkId),
            BackgroundWorkKind.BillingWebhook =>
                new BillingWebhookProcessingMessage(work.WorkId),
            _ => throw new ArgumentOutOfRangeException(
                nameof(work),
                work.Kind,
                "The background-work kind is not supported."),
        };
    }
}
