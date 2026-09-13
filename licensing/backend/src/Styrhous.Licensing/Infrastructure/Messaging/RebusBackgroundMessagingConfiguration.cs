using Rebus.Config;
using Rebus.Config.Outbox;
using Rebus.PostgreSql;
using Rebus.Retry.Simple;
using Rebus.Routing.TypeBased;
using Rebus.Serialization.Json;
using Rebus.Transport;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal static class RebusBackgroundMessagingConfiguration
{
    private const int MaximumDeliveryAttempts = 5;

    public static void Add(
        IServiceCollection services,
        string connectionString,
        BackgroundMessagingSettings settings)
    {
        ArgumentNullException.ThrowIfNull(services);
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionString);
        services.AddSingleton(settings);
        services.AddRebus(configure =>
        {
            configure.Transport(transport => transport.UsePostgreSql(
                connectionString,
                "RebusMessages",
                settings.QueueName));
            configure.Routing(routing => routing.TypeBased()
                .Map<OrganizationInvitationDeliveryMessage>(settings.QueueName)
                .Map<InfrastructureSmokeProbeMessage>(settings.QueueName)
                .Map<BillingWebhookProcessingMessage>(settings.QueueName));
            configure.Serialization(serializer => serializer.UseSystemTextJson());
            configure.Outbox(outbox => outbox.StoreInPostgreSql(
                connectionString,
                PostgresBackgroundWorkOutbox.TableName));
            configure.Options(options =>
            {
                options.RetryStrategy(
                    settings.ErrorQueueName,
                    MaximumDeliveryAttempts);
                options.SetNumberOfWorkers(1);
                options.SetMaxParallelism(settings.MaximumParallelism);
                options.Decorate<ITransport>(resolution =>
                    new NativeOutboxSendGuardTransport(resolution.Get<ITransport>()));
            });
            return configure;
        });
        services.AddRebusHandler<OrganizationInvitationDeliveryMessageHandler>();
        services.AddRebusHandler<InfrastructureSmokeProbeMessageHandler>();
        services.AddRebusHandler<BillingWebhookProcessingMessageHandler>();
    }
}
