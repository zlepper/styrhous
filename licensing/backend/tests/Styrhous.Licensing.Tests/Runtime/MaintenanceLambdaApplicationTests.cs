using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.TestHost;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Rebus.Config;
using Rebus.Handlers;
using Rebus.Serialization.Json;
using Rebus.ServiceProvider;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class MaintenanceLambdaApplicationTests
{
    [Test]
    public async Task MinimalMaintenanceConfigurationRecoversPendingWorkOverHttp()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var queueName = $"styrhous-maintenance-test-{Guid.CreateVersion7():N}";
        var occurredAt = DateTimeOffset.UtcNow.AddMinutes(-10);
        var outbox = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery,
            "protected-payload",
            occurredAt,
            DateTimeOffset.UtcNow.AddHours(1));
        await using (var context = database.CreateContext())
        {
            context.OutboxMessages.Add(outbox);
            await context.SaveChangesAsync();
        }
        using var receiver = BuildReceiver(queueName);
        await receiver.StartAsync();
        var configuration = JsonSerializer.Serialize(new
        {
            ConnectionStrings = new { Licensing = database.ConnectionString },
            Messaging = new
            {
                Transport = "RabbitMq",
                QueueName = queueName,
                ErrorQueueName = $"{queueName}-error",
                RabbitMq = new
                {
                    ConnectionString = RabbitMqTestConnection.Value,
                },
            },
        });
        try
        {
            await using var application = Program.BuildMaintenanceApplication(
                [],
                configuration,
                builder => builder.WebHost.UseTestServer());
            await application.StartAsync();
            using var client = application.GetTestClient();

            using var response = await client.PostAsync(
                "/internal/lambda/maintenance",
                content: null);
            var result = await response.Content.ReadFromJsonAsync<
                BackgroundWorkRecoveryResult>();
            await using var verificationContext = database.CreateContext();
            var persisted = await verificationContext.OutboxMessages
                .AsNoTracking()
                .SingleAsync(message => message.Id == outbox.Id);

            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(result, Is.Not.Null);
                Assert.That(result!.OutboxDrained, Is.True);
                Assert.That(persisted.NativeOutboxEnqueued, Is.True);
            });
        }
        finally
        {
            await receiver.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task SmokeProbeRequestSeedsAndPublishesDurableWorkerWork()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var queueName = $"styrhous-maintenance-smoke-{Guid.CreateVersion7():N}";
        var probeId = Guid.CreateVersion7();
        using var receiver = BuildReceiver(queueName);
        await receiver.StartAsync();
        var configuration = JsonSerializer.Serialize(new
        {
            ConnectionStrings = new { Licensing = database.ConnectionString },
            Messaging = new
            {
                Transport = "RabbitMq",
                QueueName = queueName,
                ErrorQueueName = $"{queueName}-error",
                RabbitMq = new { ConnectionString = RabbitMqTestConnection.Value },
            },
        });
        try
        {
            await using var application = Program.BuildMaintenanceApplication(
                [],
                configuration,
                builder => builder.WebHost.UseTestServer());
            await application.StartAsync();
            using var client = application.GetTestClient();

            using var response = await client.PostAsJsonAsync(
                "/internal/lambda/maintenance",
                new { smokeTestId = probeId });
            using var result = JsonDocument.Parse(
                await response.Content.ReadAsStringAsync());
            await using var verificationContext = database.CreateContext();
            var persisted = await verificationContext.OutboxMessages
                .AsNoTracking()
                .SingleAsync(message => message.SubjectId == probeId);

            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(
                    result.RootElement.GetProperty("outboxDrained").GetBoolean(),
                    Is.True);
                Assert.That(
                    result.RootElement.GetProperty("smokeProbe")
                        .GetProperty("probeId").GetGuid(),
                    Is.EqualTo(probeId));
                Assert.That(
                    persisted.MessageType,
                    Is.EqualTo(OutboxMessageTypes.InfrastructureSmokeProbe));
                Assert.That(persisted.NativeOutboxEnqueued, Is.True);
            });
        }
        finally
        {
            await receiver.StopAsync(CancellationToken.None);
        }
    }

    private static IHost BuildReceiver(string queueName)
    {
        var builder = Host.CreateApplicationBuilder();
        builder.Services.AddRebus(configure => configure
            .Transport(transport => transport.UseRabbitMq(
                RabbitMqTestConnection.Value,
                queueName))
            .Serialization(serializer => serializer.UseSystemTextJson()));
        builder.Services.AddRebusHandler<DiscardingInvitationHandler>();
        builder.Services.AddRebusHandler<DiscardingSmokeProbeHandler>();
        return builder.Build();
    }

    private sealed class DiscardingInvitationHandler
        : IHandleMessages<OrganizationInvitationDeliveryMessage>
    {
        public Task Handle(OrganizationInvitationDeliveryMessage message)
        {
            return Task.CompletedTask;
        }
    }

    private sealed class DiscardingSmokeProbeHandler
        : IHandleMessages<InfrastructureSmokeProbeMessage>
    {
        public Task Handle(InfrastructureSmokeProbeMessage message)
        {
            return Task.CompletedTask;
        }
    }
}
