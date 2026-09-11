using Rebus.Bus;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class RebusBackgroundMessagingConfigurationTests
{
    [Test]
    public void SenderOnlyModeDoesNotRegisterReceiverHandlers()
    {
        var services = new ServiceCollection();

        RebusBackgroundMessagingConfiguration.Add(
            services,
            Configuration(),
            receiveMessages: false);

        Assert.Multiple(() =>
        {
            Assert.That(
                services.Any(descriptor =>
                    descriptor.ServiceType == typeof(IBus)),
                Is.True);
            Assert.That(HasHandler<OrganizationInvitationDeliveryMessageHandler>(services), Is.False);
            Assert.That(HasHandler<InfrastructureSmokeProbeMessageHandler>(services), Is.False);
            Assert.That(HasHandler<BillingWebhookProcessingMessageHandler>(services), Is.False);
        });
    }

    [Test]
    public void ReceiverModeRegistersBothDurableWorkHandlers()
    {
        var services = new ServiceCollection();

        RebusBackgroundMessagingConfiguration.Add(
            services,
            Configuration(),
            receiveMessages: true);

        Assert.Multiple(() =>
        {
            Assert.That(HasHandler<OrganizationInvitationDeliveryMessageHandler>(services), Is.True);
            Assert.That(HasHandler<InfrastructureSmokeProbeMessageHandler>(services), Is.True);
            Assert.That(HasHandler<BillingWebhookProcessingMessageHandler>(services), Is.True);
        });
    }

    private static bool HasHandler<THandler>(IEnumerable<ServiceDescriptor> services)
    {
        return services.Any(descriptor => descriptor.ImplementationType == typeof(THandler));
    }

    private static IConfiguration Configuration()
    {
        return new ConfigurationBuilder()
            .AddInMemoryCollection(
                new Dictionary<string, string?>
                {
                    ["Messaging:Transport"] = "RabbitMq",
                    ["Messaging:QueueName"] = "styrhous-licensing",
                    ["Messaging:RabbitMq:ConnectionString"] = "amqp://localhost",
                })
            .Build();
    }
}
