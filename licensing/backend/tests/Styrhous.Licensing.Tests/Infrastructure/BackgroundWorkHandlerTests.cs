using Rebus.Handlers;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Npgsql;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BackgroundWorkHandlerTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 9, 1, 0, 0, 0, TimeSpan.Zero);

    [TestCase(WorkKind.Invitation)]
    [TestCase(WorkKind.Billing)]
    [TestCase(WorkKind.Smoke)]
    public async Task MissingWorkIsAcknowledgedWithoutCallingExternalProviders(WorkKind kind)
    {
        await using var test = await CreateAsync();
        await HandleAsync(test.Services, kind, Guid.NewGuid());
    }

    [TestCase(WorkKind.Invitation)]
    [TestCase(WorkKind.Billing)]
    [TestCase(WorkKind.Smoke)]
    public async Task DatabaseFailurePropagatesToTransportForRetry(WorkKind kind)
    {
        await using var test = await CreateAsync();
        await test.Database.DisposeAsync();

        Assert.That(async () => await HandleAsync(test.Services, kind, Guid.CreateVersion7()),
            Throws.InstanceOf<PostgresException>());
    }

    private static Task<ServiceTestBase<OrganizationInvitationDeliveryService>> CreateAsync()
    {
        return WorkerServiceTestBase.CreateAsync<OrganizationInvitationDeliveryService>(ObservedAt, services =>
        {
            services.RemoveAll<IOrganizationInvitationEmailSender>();
            services.AddSingleton<IOrganizationInvitationEmailSender, RejectingInvitationSender>();
            services.RemoveAll<ICommercialSubscriptionProvider>();
            services.AddSingleton<ICommercialSubscriptionProvider, RejectingSubscriptionProvider>();
            services.RemoveAll<IEmailSubmissionClient>();
            services.AddSingleton<IEmailSubmissionClient, RejectingEmailClient>();
        });
    }

    private static Task HandleAsync(IServiceProvider services, WorkKind kind, Guid id)
    {
        return kind switch
        {
            WorkKind.Invitation => services.GetRequiredService<IHandleMessages<OrganizationInvitationDeliveryMessage>>()
                .Handle(new OrganizationInvitationDeliveryMessage(id)),
            WorkKind.Billing => services.GetRequiredService<IHandleMessages<BillingWebhookProcessingMessage>>()
                .Handle(new BillingWebhookProcessingMessage(id)),
            WorkKind.Smoke => services.GetRequiredService<IHandleMessages<InfrastructureSmokeProbeMessage>>()
                .Handle(new InfrastructureSmokeProbeMessage(id)),
            _ => throw new ArgumentOutOfRangeException(nameof(kind)),
        };
    }

    public enum WorkKind
    {
        Invitation,
        Billing,
        Smoke,
    }

    private sealed class RejectingInvitationSender : IOrganizationInvitationEmailSender
    {
        public Task SendAsync(Guid outboxMessageId, OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            throw new AssertionException("Unclaimed work must not send email.");
        }
    }

    private sealed class RejectingSubscriptionProvider : ICommercialSubscriptionProvider
    {
        public Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(string externalEventId,
            BillingWebhookEventKind kind, CancellationToken cancellationToken)
        {
            throw new AssertionException("Unclaimed work must not call Stripe.");
        }

        public Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
            string externalSubscriptionId, Guid expectedBillingOperationId, CancellationToken cancellationToken)
        {
            throw new AssertionException("Unclaimed work must not call Stripe.");
        }
    }

    private sealed class RejectingEmailClient : IEmailSubmissionClient
    {
        public Task SendAsync(InvitationEmailSubmission submission, CancellationToken cancellationToken)
        {
            throw new AssertionException("Unclaimed work must not send email.");
        }
    }
}
