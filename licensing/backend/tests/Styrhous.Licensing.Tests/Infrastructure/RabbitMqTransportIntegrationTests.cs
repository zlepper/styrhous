using Rebus.Bus;
using Microsoft.Extensions.DependencyInjection.Extensions;
using System.Collections.Concurrent;
using System.Text.Json;
using Rebus.Config.Outbox;
using Rebus.Transport;
using Rebus.Messages;
using Styrhous.Licensing.Infrastructure.Organizations;
using System.Net;
using System.Net.Http.Json;
using System.Text;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Rebus.Activation;
using Rebus.Config;
using Rebus.Handlers;
using Rebus.Serialization.Json;
using Stripe;
using Styrhous.Licensing.Api.Billing;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Api;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[NonParallelizable]
public sealed class RabbitMqTransportIntegrationTests
{
    private static readonly DateTimeOffset ObservedAt =
        LicensingPersistenceScenario.SignupTime.AddDays(3);

    [Test]
    public async Task ApiNativeOutboxForwardsCommittedOutboxIdentifier()
    {
        await using var database = await CreateTestDatabaseAsync();
        var owner = await LicensingPersistenceScenario.SignUpAsync(
            database,
            "rabbit-api-owner",
            "rabbit-api-owner@example.com");
        var organization = await LicensingPersistenceScenario.CreateOrganizationAsync(
            database,
            owner.UserId,
            "Rabbit API",
            ObservedAt.AddDays(-1));
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var capture = new InvitationHintCapture();
        using var receiver = BuildHintReceiver(queueName, capture);
        await receiver.StartAsync();
        try
        {
            await using var factory = new LicensingWebApplicationFactory(
                database,
                ObservedAt,
                rabbitMqMessaging: new RabbitMqApiMessaging(
                    queueName,
                    RabbitMqTestConnection.Value));
            using var client = factory.CreateApiClient(owner.UserId);
            var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
            using var request = new HttpRequestMessage(
                HttpMethod.Post,
                $"/api/organizations/{organization.OrganizationId}/invitations")
            {
                Content = JsonContent.Create(
                    new { email = "rabbit-api-invitee@example.com", role = "member" }),
            };
            AntiforgeryTestClient.AddToken(request, antiforgeryToken);

            using var response = await client.SendAsync(request);

            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
            var publishedWorkId = await capture.WorkId.Task.WaitAsync(
                TimeSpan.FromSeconds(10));
            await using var context = database.CreateContext();
            var outbox = await context.OutboxMessages
                .AsNoTracking()
                .SingleAsync(message => message.Id == publishedWorkId);
            Assert.Multiple(() =>
            {
                Assert.That(outbox.Id.Version, Is.EqualTo(7));
                Assert.That(outbox.NativeOutboxEnqueued, Is.True);
                Assert.That(outbox.DeliveredAt, Is.Null);
            });
        }
        finally
        {
            await receiver.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task ApiNativeOutboxForwardsCommittedBillingIdentifier()
    {
        await using var database = await CreateTestDatabaseAsync();
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var capture = new BillingHintCapture();
        using var receiver = BuildBillingHintReceiver(queueName, capture);
        await receiver.StartAsync();
        try
        {
            await using var factory = new LicensingWebApplicationFactory(
                database,
                ObservedAt,
                rabbitMqMessaging: new RabbitMqApiMessaging(
                    queueName,
                    RabbitMqTestConnection.Value));
            using var client = factory.CreateApiClient();
            var payload = $"{{\"id\":\"evt_rabbit_api\",\"object\":\"event\","
                + $"\"created\":{ObservedAt.AddMinutes(-1).ToUnixTimeSeconds()},"
                + $"\"type\":\"{StripeBillingWebhookEventTypes.InvoicePaid}\","
                + "\"data\":{\"object\":{}}}";
            using var request = new HttpRequestMessage(
                HttpMethod.Post,
                "/api/webhooks/stripe")
            {
                Content = new StringContent(payload, Encoding.UTF8, "application/json"),
            };
            request.Headers.Add(
                BillingWebhookEndpoints.SignatureHeaderName,
                EventUtility.GenerateSignatureHeader(
                    payload,
                    LicensingWebApplicationFactory.StripeWebhookSecret,
                    ObservedAt.ToUnixTimeSeconds()));

            using var response = await client.SendAsync(request);

            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            var publishedWorkId = await capture.WorkId.Task.WaitAsync(
                TimeSpan.FromSeconds(10));
            await using var context = database.CreateContext();
            var webhookEvent = await context.BillingWebhookEvents
                .AsNoTracking()
                .SingleAsync(candidate => candidate.Id == publishedWorkId);
            Assert.Multiple(() =>
            {
                Assert.That(webhookEvent.Id.Version, Is.EqualTo(7));
                Assert.That(webhookEvent.Kind, Is.EqualTo(BillingWebhookEventKind.InvoicePaid));
                Assert.That(webhookEvent.NativeOutboxEnqueued, Is.True);
                Assert.That(webhookEvent.ProcessedAt, Is.Null);
            });
        }
        finally
        {
            await receiver.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task ConfiguredReceiverDispatchesRabbitMqHintThroughEfStore()
    {
        await using var database = await CreateTestDatabaseAsync();
        var scenario = await CreateInvitationOutboxAsync(database, "dispatch");
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var emailSender = new AttemptingInvitationEmailSender(failuresBeforeSuccess: 0);
        using var host = BuildReceiver(database, queueName, emailSender);
        await host.StartAsync();
        try
        {
            await host.Services.GetRequiredService<IBus>().Send(new OrganizationInvitationDeliveryMessage(scenario.OutboxMessageId));

            Assert.That(
                await emailSender.Delivered.Task.WaitAsync(TimeSpan.FromSeconds(10)),
                Is.EqualTo(scenario.OutboxMessageId));
            var persisted = await WaitForOutboxAsync(
                database,
                scenario.OutboxMessageId,
                message => message.DeliveredAt is not null);
            Assert.Multiple(() =>
            {
                Assert.That(persisted.DeliveredAt, Is.EqualTo(ObservedAt));
                Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(1));
            });
        }
        finally
        {
            await host.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task TransientHandlerFailuresAreRetriedUntilDeliverySucceeds()
    {
        await using var database = await CreateTestDatabaseAsync();
        var scenario = await CreateInvitationOutboxAsync(database, "retry");
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var emailSender = new AttemptingInvitationEmailSender(failuresBeforeSuccess: 2);
        using var host = BuildReceiver(database, queueName, emailSender);
        await host.StartAsync();
        try
        {
            await host.Services.GetRequiredService<IBus>().Send(new OrganizationInvitationDeliveryMessage(scenario.OutboxMessageId));

            Assert.That(
                await emailSender.Delivered.Task.WaitAsync(TimeSpan.FromSeconds(10)),
                Is.EqualTo(scenario.OutboxMessageId));
            var persisted = await WaitForOutboxAsync(
                database,
                scenario.OutboxMessageId,
                message => message.DeliveredAt is not null);
            Assert.Multiple(() =>
            {
                Assert.That(emailSender.AttemptCount, Is.EqualTo(3));
                Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(3));
                Assert.That(persisted.ProcessingLeaseId, Is.Null);
            });
        }
        finally
        {
            await host.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task PermanentHandlerFailureMovesHintToErrorQueueAfterFiveAttempts()
    {
        await using var database = await CreateTestDatabaseAsync();
        var scenario = await CreateInvitationOutboxAsync(database, "dead-letter");
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var errorQueueName = $"{queueName}-error";
        var emailSender = new AttemptingInvitationEmailSender(int.MaxValue);
        var deadLettered = new TaskCompletionSource<Guid>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        using var errorActivator = new BuiltinHandlerActivator();
        errorActivator.Handle<OrganizationInvitationDeliveryMessage>(message =>
        {
            deadLettered.TrySetResult(message.WorkId);
            return Task.CompletedTask;
        });
        using var errorReceiver = Configure.With(errorActivator)
            .Transport(transport => transport.UseRabbitMq(
                RabbitMqTestConnection.Value,
                errorQueueName))
            .Serialization(serializer => serializer.UseSystemTextJson())
            .Start();
        using var host = BuildReceiver(database, queueName, emailSender);
        await host.StartAsync();
        try
        {
            await host.Services.GetRequiredService<IBus>().Send(new OrganizationInvitationDeliveryMessage(scenario.OutboxMessageId));

            Assert.That(
                await deadLettered.Task.WaitAsync(TimeSpan.FromSeconds(10)),
                Is.EqualTo(scenario.OutboxMessageId));
            var persisted = await WaitForOutboxAsync(
                database,
                scenario.OutboxMessageId,
                message => message.ProcessingAttemptCount == 5);
            Assert.Multiple(() =>
            {
                Assert.That(emailSender.AttemptCount, Is.EqualTo(5));
                Assert.That(persisted.DeliveredAt, Is.Null);
                Assert.That(persisted.ProcessingLeaseId, Is.Null);
                Assert.That(persisted.ProcessingLeaseExpiresAt, Is.Null);
            });
        }
        finally
        {
            await host.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task MaintenancePublishesWhileWorkerIsScaledToZeroAndWorkerLaterDelivers()
    {
        await using var database = await CreateTestDatabaseAsync();
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var scenario = await CreateInvitationOutboxAsync(database, "maintenance", queueName);
        var provisioningScenario = await CreateInvitationOutboxAsync(
            database,
            "maintenance-provisioning");
        var workerObservedAt = DateTimeOffset.FromUnixTimeSeconds(
            DateTimeOffset.UtcNow.ToUnixTimeSeconds());
        await MakeRecoverableAtCurrentTimeAsync(
            database,
            scenario,
            workerObservedAt);
        var provisioningEmailSender = new AttemptingInvitationEmailSender(
            failuresBeforeSuccess: 0);
        using (var queueProvisioner = BuildReceiver(
            database,
            queueName,
            provisioningEmailSender))
        {
            await queueProvisioner.StartAsync();
            await queueProvisioner.Services
                .GetRequiredService<IBus>().Send(new OrganizationInvitationDeliveryMessage(provisioningScenario.OutboxMessageId));
            await provisioningEmailSender.Delivered.Task.WaitAsync(
                TimeSpan.FromSeconds(10));
            await WaitForOutboxAsync(
                database,
                provisioningScenario.OutboxMessageId,
                message => message.DeliveredAt is not null);
            await queueProvisioner.StopAsync(CancellationToken.None);
        }

        var recovery = await Program.RunMaintenanceAsync(
            MaintenanceArguments(database.ConnectionString, queueName));

        Assert.That(
            recovery,
            Is.EqualTo(new BackgroundWorkRecoveryResult(OutboxDrained: true)));
        var emailSender = new AttemptingInvitationEmailSender(failuresBeforeSuccess: 0);
        using var worker = BuildReceiver(
            database,
            queueName,
            emailSender,
            workerObservedAt);
        await worker.StartAsync();
        try
        {
            Assert.That(
                await emailSender.Delivered.Task.WaitAsync(TimeSpan.FromSeconds(10)),
                Is.EqualTo(scenario.OutboxMessageId));
            var persisted = await WaitForOutboxAsync(
                database,
                scenario.OutboxMessageId,
                message => message.DeliveredAt is not null);
            Assert.Multiple(() =>
            {
                Assert.That(persisted.NativeOutboxEnqueued, Is.True);
                Assert.That(persisted.DeliveredAt, Is.EqualTo(workerObservedAt));
            });
        }
        finally
        {
            await worker.StopAsync(CancellationToken.None);
        }
    }

    [TestCase(1)]
    [TestCase(3)]
    public async Task NativeForwarderRetainsFailedPublicationAndRestartDeliversToRabbitMq(int messageCount)
    {
        await using var database = await CreateTestDatabaseAsync();
        var queueName = $"styrhous-native-restart-{Guid.CreateVersion7():N}";
        var capture = new InvitationHintCapture();
        using var receiver = BuildHintReceiver(queueName, capture);
        await receiver.StartAsync();
        try
        {
            var work = Enumerable.Range(0, messageCount).Select(_ => OutboxMessage.Enqueue(
                Guid.CreateVersion7(), Guid.CreateVersion7(),
                OutboxMessageTypes.OrganizationInvitationDelivery, "protected-payload",
                DateTimeOffset.UtcNow, DateTimeOffset.UtcNow.AddHours(1))).ToArray();
            await using (var context = database.CreateContext())
            {
                await using var transaction = await context.Database.BeginTransactionAsync();
                context.OutboxMessages.AddRange(work);
                foreach (var item in work)
                {
                    await new PostgresBackgroundWorkOutbox(queueName).EnqueueAsync(context,
                        BackgroundWorkReference.OrganizationInvitationDelivery(item.Id, item.OccurredAt),
                        CancellationToken.None);
                }
                await context.SaveChangesAsync();
                await transaction.CommitAsync();
            }
            var failedSend = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            using (var activator = new BuiltinHandlerActivator())
            using (var forwarder = Configure.With(activator)
                .Transport(transport => transport.UseRabbitMqAsOneWayClient(RabbitMqTestConnection.Value))
                .Outbox(outbox => outbox.StoreInPostgreSql(database.ConnectionString, PostgresBackgroundWorkOutbox.TableName))
                .Options(options => options.Decorate<ITransport>(resolution =>
                    new NativeOutboxSendGuardTransport(
                        new FailingPublicationTransport(resolution.Get<ITransport>(), failedSend, work[^1].Id))))
                .Start())
            {
                await failedSend.Task.WaitAsync(TimeSpan.FromSeconds(10));
            }
            await using (var verification = database.CreateContext())
            {
                Assert.That(await verification.Set<RebusOutboxMessage>().CountAsync(), Is.EqualTo(messageCount));
            }
            Assert.That(capture.WorkId.Task.IsCompleted, Is.False);

            var recovered = await Program.RunMaintenanceAsync(MaintenanceArguments(database.ConnectionString, queueName));
            Assert.That(recovered.OutboxDrained, Is.True);
            using var receiveTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
            while (capture.Received.Count < messageCount)
            {
                await Task.Delay(TimeSpan.FromMilliseconds(50), receiveTimeout.Token);
            }
            Assert.That(capture.Received, Is.EquivalentTo(work.Select(item => item.Id)));
            await using var completed = database.CreateContext();
            Assert.That(await completed.Set<RebusOutboxMessage>().AnyAsync(), Is.False);
        }
        finally
        {
            await receiver.StopAsync(CancellationToken.None);
        }
    }

    [Test]
    public async Task WorkerShutdownCancelsInFlightDeliveryAndReleasesEfLease()
    {
        await using var database = await CreateTestDatabaseAsync();
        var scenario = await CreateInvitationOutboxAsync(database, "shutdown");
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        var emailSender = new BlockingInvitationEmailSender();
        using var worker = BuildReceiver(database, queueName, emailSender);
        await worker.StartAsync();
        await worker.Services.GetRequiredService<IBus>().Send(new OrganizationInvitationDeliveryMessage(scenario.OutboxMessageId));
        Assert.That(
            await emailSender.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)),
            Is.EqualTo(scenario.OutboxMessageId));

        await worker.StopAsync(CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10));

        await emailSender.CancellationObserved.Task.WaitAsync(TimeSpan.FromSeconds(10));
        var persisted = await WaitForOutboxAsync(
            database,
            scenario.OutboxMessageId,
            message => message.ProcessingLeaseId is null);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.DeliveredAt, Is.Null);
            Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(1));
            Assert.That(persisted.ProcessingLeaseExpiresAt, Is.Null);
        });
    }

    private static Task<PostgresTestDatabase> CreateTestDatabaseAsync()
    {
        return PostgresTestDatabase.CreateAsync();
    }

    private static IHost BuildReceiver(
        PostgresTestDatabase database,
        string queueName,
        IOrganizationInvitationEmailSender emailSender,
        DateTimeOffset? observedAt = null)
    {
        return Program.BuildWorker([
            "--environment=Testing",
            "--Authentication:GitHub:ClientId=",
            "--Authentication:GitHub:ClientSecret=",
            "--Authentication:Google:ClientId=",
            "--Authentication:Google:ClientSecret=",
            "--Authentication:Microsoft:ClientId=",
            "--Authentication:Microsoft:ClientSecret=",
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
            "--Messaging:Transport=RabbitMq",
            $"--Messaging:QueueName={queueName}",
            $"--Messaging:ErrorQueueName={queueName}-error",
            $"--Messaging:RabbitMq:ConnectionString={RabbitMqTestConnection.Value}",
            $"--DataProtection:Certificate={TestDataProtectionCertificate.EncodedCertificate}",
            $"--DataProtection:CertificatePassword={TestDataProtectionCertificate.Password}",
            "--InvitationEmail:FromAddress=invitations@example.com",
            "--InvitationEmail:AcceptanceUrl=https://localhost/invitations/accept",
            "--InvitationEmail:AmazonSes:Region=eu-west-1",
            "--Stripe:SecretKey=sk_test_worker",
            "--Stripe:MonthlyPriceId=price_monthly",
            "--Stripe:AnnualPriceId=price_annual",
        ], applicationConfiguration: null, configureBuilder: builder =>
        {
            builder.ConfigureContainer(new DefaultServiceProviderFactory(new ServiceProviderOptions
            {
                ValidateScopes = true,
                ValidateOnBuild = true,
            }));
            builder.Services.RemoveAll<TimeProvider>();
            builder.Services.AddSingleton<TimeProvider>(new FixedTimeProvider(observedAt ?? ObservedAt));
            builder.Services.RemoveAll<IOrganizationInvitationEmailSender>();
            builder.Services.AddSingleton(emailSender);
        });
    }

    private static IHost BuildHintReceiver(
        string queueName,
        InvitationHintCapture capture)
    {
        var builder = Host.CreateApplicationBuilder();
        builder.Services.AddSingleton(capture);
        builder.Services.AddRebus(configure => configure
            .Transport(transport => transport.UseRabbitMq(
                RabbitMqTestConnection.Value,
                queueName))
            .Serialization(serializer => serializer.UseSystemTextJson()));
        builder.Services.AddRebusHandler<CapturingInvitationHintHandler>();
        return builder.Build();
    }

    private static IHost BuildBillingHintReceiver(
        string queueName,
        BillingHintCapture capture)
    {
        var builder = Host.CreateApplicationBuilder();
        builder.Services.AddSingleton(capture);
        builder.Services.AddRebus(configure => configure
            .Transport(transport => transport.UseRabbitMq(
                RabbitMqTestConnection.Value,
                queueName))
            .Serialization(serializer => serializer.UseSystemTextJson()));
        builder.Services.AddRebusHandler<CapturingBillingHintHandler>();
        return builder.Build();
    }

    internal static async Task<InvitationOutboxScenario> CreateInvitationOutboxAsync(
        PostgresTestDatabase database,
        string subject,
        string queueName = "test-queue")
    {
        var owner = await LicensingPersistenceScenario.SignUpAsync(
            database,
            $"rabbit-{subject}-owner",
            $"rabbit-{subject}-owner@example.com");
        var organization = await LicensingPersistenceScenario.CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Rabbit {subject}",
            LicensingPersistenceScenario.SignupTime.AddDays(1));
        await using var test = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, LicensingPersistenceScenario.SignupTime.AddDays(2),
            configureHostServices: (configuration, _) => configuration["Messaging:QueueName"] = queueName);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var result = await test.Service
            .CreateAsync(owner.UserId, organization.OrganizationId,
                $"rabbit-{subject}-invitee@example.com", OrganizationRole.Member);
        Assert.That(result, Is.TypeOf<OrganizationInvitationCreationResult.Success>());
        var invitation = (OrganizationInvitationCreationResult.Success)result;
        var outboxMessageId = await context.OutboxMessages
            .Where(message => message.SubjectId == invitation.InvitationId)
            .Select(message => message.Id)
            .SingleAsync();
        return new InvitationOutboxScenario(
            organization.OrganizationId,
            invitation.InvitationId,
            outboxMessageId,
            $"rabbit-{subject}-invitee@example.com",
            invitation.Secret);
    }

    private static async Task MakeRecoverableAtCurrentTimeAsync(
        PostgresTestDatabase database,
        InvitationOutboxScenario scenario,
        DateTimeOffset observedAt)
    {
        var occurredAt = observedAt.AddMinutes(-5);
        var expiresAt = occurredAt.Add(OrganizationInvitation.Lifetime);
        var delivery = OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Created,
            scenario.InvitationId,
            scenario.OrganizationId,
            scenario.Email,
            OrganizationRole.Member,
            scenario.Secret.Reveal(),
            expiresAt);
        await using var test = ServiceTestBase<DataProtectionOrganizationInvitationDeliveryProtector>.ForDatabase(database, observedAt);
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == scenario.InvitationId)
            .ExecuteUpdateAsync(setters => setters
                .SetProperty(invitation => invitation.CreatedAt, occurredAt)
                .SetProperty(invitation => invitation.LastSentAt, occurredAt)
                .SetProperty(invitation => invitation.ExpiresAt, expiresAt));
        await context.OutboxMessages
            .Where(message => message.Id == scenario.OutboxMessageId)
            .ExecuteUpdateAsync(setters => setters
                .SetProperty(message => message.OccurredAt, occurredAt)
                .SetProperty(message => message.NotAfter, expiresAt)
                .SetProperty(
                    message => message.ProtectedPayload,
                    test.Service.Protect(delivery)));
    }

    private static async Task<OutboxMessage> WaitForOutboxAsync(
        PostgresTestDatabase database,
        Guid outboxMessageId,
        Func<OutboxMessage, bool> predicate)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        while (true)
        {
            await using var context = database.CreateContext();
            var message = await context.OutboxMessages
                .AsNoTracking()
                .SingleAsync(
                    candidate => candidate.Id == outboxMessageId,
                    timeout.Token);
            if (predicate(message))
            {
                return message;
            }

            await Task.Delay(TimeSpan.FromMilliseconds(50), timeout.Token);
        }
    }

    private static string[] MaintenanceArguments(
        string databaseConnectionString,
        string queueName)
    {
        return [
        $"--ConnectionStrings:Licensing={databaseConnectionString}",
        "--Messaging:Transport=RabbitMq",
        $"--Messaging:QueueName={queueName}",
        $"--Messaging:RabbitMq:ConnectionString={RabbitMqTestConnection.Value}",
    ];
    }

    internal sealed record InvitationOutboxScenario(
        Guid OrganizationId,
        Guid InvitationId,
        Guid OutboxMessageId,
        string Email,
        OrganizationInvitationSecret Secret);

    private sealed class FailingPublicationTransport(
        ITransport inner, TaskCompletionSource failedSend, Guid failedWorkId) : ITransport
    {
        public string Address => inner.Address;

        public void CreateQueue(string address)
        {
            inner.CreateQueue(address);
        }

        public Task Send(string destinationAddress, TransportMessage message, ITransactionContext context)
        {
            var work = JsonSerializer.Deserialize<OrganizationInvitationDeliveryMessage>(message.Body)!;
            if (work.WorkId == failedWorkId)
            {
                failedSend.TrySetResult();
                throw new IOException("The test transport rejects publication before contacting the shared broker.");
            }
            return inner.Send(destinationAddress, message, context);
        }

        public Task<TransportMessage> Receive(ITransactionContext context, CancellationToken cancellationToken)
        {
            return inner.Receive(context, cancellationToken);
        }
    }

    private sealed class InvitationHintCapture
    {
        public ConcurrentQueue<Guid> Received { get; } = new();
        public TaskCompletionSource<Guid> WorkId { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    private sealed class BillingHintCapture
    {
        public TaskCompletionSource<Guid> WorkId { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    private sealed class CapturingInvitationHintHandler(InvitationHintCapture capture)
        : IHandleMessages<OrganizationInvitationDeliveryMessage>
    {

        public Task Handle(OrganizationInvitationDeliveryMessage message)
        {
            capture.Received.Enqueue(message.WorkId);
            capture.WorkId.TrySetResult(message.WorkId);
            return Task.CompletedTask;
        }
    }

    private sealed class CapturingBillingHintHandler(BillingHintCapture capture)
        : IHandleMessages<BillingWebhookProcessingMessage>
    {

        public Task Handle(BillingWebhookProcessingMessage message)
        {
            capture.WorkId.TrySetResult(message.WorkId);
            return Task.CompletedTask;
        }
    }

    private sealed class AttemptingInvitationEmailSender(int failuresBeforeSuccess)
        : IOrganizationInvitationEmailSender
    {
        private int _attemptCount;

        public int AttemptCount => Volatile.Read(ref _attemptCount);

        public TaskCompletionSource<Guid> Delivered { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task SendAsync(
            Guid outboxMessageId,
            OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var attempt = Interlocked.Increment(ref _attemptCount);
            if (attempt <= failuresBeforeSuccess)
            {
                throw new InvalidOperationException("Synthetic email submission failure.");
            }

            Delivered.TrySetResult(outboxMessageId);
            return Task.CompletedTask;
        }
    }

    private sealed class BlockingInvitationEmailSender
        : IOrganizationInvitationEmailSender
    {
        public TaskCompletionSource<Guid> Started { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource CancellationObserved { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task SendAsync(
            Guid outboxMessageId,
            OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            Started.TrySetResult(outboxMessageId);
            try
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                CancellationObserved.TrySetResult();
                throw;
            }
        }
    }
}
