using Rebus.Config.Outbox;
using Styrhous.Licensing.Persistence;
using Amazon;
using Amazon.SQS;
using Rebus.Config;
using Rebus.Retry.Simple;
using Rebus.Routing.TypeBased;
using Rebus.Serialization.Json;
using Rebus.Transport;
using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal static class RebusBackgroundMessagingConfiguration
{
    private const int MaximumDeliveryAttempts = 5;

    public static void Add(
        IServiceCollection services,
        IConfiguration configuration,
        bool receiveMessages)
    {
        ArgumentNullException.ThrowIfNull(services);
        ArgumentNullException.ThrowIfNull(configuration);
        var settings = BackgroundMessagingSettings.From(configuration);
        services.AddSingleton(settings);
        services.AddRebus(configure =>
        {
            configure.Transport(transport => ConfigureTransport(
                transport,
                settings,
                receiveMessages));
            configure.Routing(routing => routing.TypeBased()
                .Map<OrganizationInvitationDeliveryMessage>(settings.QueueName)
                .Map<InfrastructureSmokeProbeMessage>(settings.QueueName)
                .Map<BillingWebhookProcessingMessage>(settings.QueueName));
            configure.Serialization(serializer => serializer.UseSystemTextJson());
            configure.Options(options =>
            {
                options.RetryStrategy(
                    settings.ErrorQueueName,
                    MaximumDeliveryAttempts);
                if (receiveMessages)
                {
                    options.SetNumberOfWorkers(1);
                    options.SetMaxParallelism(settings.MaximumParallelism);
                }
            });
            if (!receiveMessages)
            {
                configure.Outbox(outbox => outbox.StoreInPostgreSql(
                    configuration.GetConnectionString("Licensing")
                        ?? throw new InvalidOperationException("ConnectionStrings:Licensing is required."),
                    PostgresBackgroundWorkOutbox.TableName));
                configure.Options(options => options.Decorate<ITransport>(resolution =>
                    new NativeOutboxSendGuardTransport(resolution.Get<ITransport>())));
            }
            return configure;
        });
        if (receiveMessages)
        {
            services.AddRebusHandler<OrganizationInvitationDeliveryMessageHandler>();
            services.AddRebusHandler<InfrastructureSmokeProbeMessageHandler>();
            services.AddRebusHandler<BillingWebhookProcessingMessageHandler>();
        }
    }

    private static void ConfigureTransport(
        StandardConfigurer<ITransport> transport,
        BackgroundMessagingSettings settings,
        bool receiveMessages)
    {
        switch (settings.Transport)
        {
            case BackgroundMessagingTransport.RabbitMq:
                ConfigureRabbitMq(transport, settings, receiveMessages);
                return;
            case BackgroundMessagingTransport.AmazonSqs:
                ConfigureAmazonSqs(transport, settings, receiveMessages);
                return;
            default:
                throw new ArgumentOutOfRangeException(
                    nameof(settings),
                    settings.Transport,
                    "The messaging transport is not supported.");
        }
    }

    private static void ConfigureRabbitMq(
        StandardConfigurer<ITransport> transport,
        BackgroundMessagingSettings settings,
        bool receiveMessages)
    {
        if (receiveMessages)
        {
            transport.UseRabbitMq(
                settings.RabbitMqConnectionString!,
                settings.QueueName);
        }
        else
        {
            transport.UseRabbitMqAsOneWayClient(
                settings.RabbitMqConnectionString!);
        }
    }

    private static void ConfigureAmazonSqs(
        StandardConfigurer<ITransport> transport,
        BackgroundMessagingSettings settings,
        bool receiveMessages)
    {
        var clientConfiguration = new AmazonSQSConfig
        {
            RegionEndpoint = RegionEndpoint.GetBySystemName(
                settings.AmazonSqsRegion),
        };
        var transportOptions = new AmazonSQSTransportOptions
        {
            CreateQueues = settings.CreateAmazonSqsQueues,
            ReceiveWaitTimeSeconds = 20,
        };
        if (receiveMessages)
        {
            transport.UseAmazonSQS(
                settings.QueueName,
                clientConfiguration,
                transportOptions);
        }
        else
        {
            transport.UseAmazonSQSAsOneWayClient(
                clientConfiguration,
                transportOptions);
        }
    }
}
