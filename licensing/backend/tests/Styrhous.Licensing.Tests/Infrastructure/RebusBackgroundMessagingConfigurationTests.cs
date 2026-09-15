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
    public void MonolithRegistersSenderAndAllReceiverHandlers()
    {
        var services = new ServiceCollection();

        RebusBackgroundMessagingConfiguration.Add(
            services,
            "Host=localhost;Database=licensing;Username=styrhous;Password=test",
            Settings());

        Assert.Multiple(() =>
        {
            Assert.That(
                services.Any(descriptor =>
                    descriptor.ServiceType == typeof(IBus)),
                Is.True);
            Assert.That(HasHandler<OrganizationInvitationDeliveryMessageHandler>(services), Is.True);
            Assert.That(HasHandler<InfrastructureSmokeProbeMessageHandler>(services), Is.True);
            Assert.That(HasHandler<BillingWebhookProcessingMessageHandler>(services), Is.True);
        });
    }

    private static bool HasHandler<THandler>(IEnumerable<ServiceDescriptor> services)
    {
        return services.Any(descriptor => descriptor.ImplementationType == typeof(THandler));
    }

    private static BackgroundMessagingSettings Settings()
    {
        return BackgroundMessagingSettings.From(new ConfigurationBuilder()
            .AddInMemoryCollection(
                new Dictionary<string, string?>
                {
                    ["Messaging:QueueName"] = "styrhous-licensing",
                })
            .Build());
    }
}
