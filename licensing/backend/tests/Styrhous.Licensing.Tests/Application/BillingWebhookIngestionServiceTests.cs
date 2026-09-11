using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookIngestionServiceTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 12, 0, 0, TimeSpan.Zero);

    [TestCase(BillingWebhookVerificationStatus.InvalidSignature, BillingWebhookIngestionStatus.InvalidSignature)]
    [TestCase(BillingWebhookVerificationStatus.InvalidPayload, BillingWebhookIngestionStatus.InvalidPayload)]
    public async Task RejectedVerificationNeverWritesToInbox(
        BillingWebhookVerificationStatus verificationStatus,
        BillingWebhookIngestionStatus expectedStatus)
    {
        await using var test = await CreateAsync(
            verificationStatus == BillingWebhookVerificationStatus.InvalidSignature
                ? BillingWebhookVerificationResult.InvalidSignature()
                : BillingWebhookVerificationResult.InvalidPayload());

        var result = await test.Service.IngestAsync("payload", "signature");
        await using var verification = test.Database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(expectedStatus));
            Assert.That(result.InboxEventId, Is.Null);
            Assert.That(verification.BillingWebhookEvents.Count(), Is.Zero);
        });
    }

    [TestCase(BillingWebhookEventKind.Unsupported, "charge.refunded", BillingWebhookIngestionStatus.Ignored)]
    [TestCase(BillingWebhookEventKind.SubscriptionChanged, "customer.subscription.updated", BillingWebhookIngestionStatus.Received)]
    public async Task VerifiedEventIsDurableAndDuplicateDeliveryDoesNotCreateAnotherRecord(
        BillingWebhookEventKind kind, string eventType, BillingWebhookIngestionStatus expected)
    {
        await using var test = await CreateAsync(BillingWebhookVerificationResult.Verified(
            new VerifiedBillingWebhookEvent("evt_supported", eventType, kind, ObservedAt.AddMinutes(-1))));

        var received = await test.Service.IngestAsync("payload", "signature");
        var duplicate = await test.Service.IngestAsync("payload", "signature");
        await using var verification = test.Database.CreateContext();
        var stored = await verification.BillingWebhookEvents.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(received.Status, Is.EqualTo(expected));
            Assert.That(received.InboxEventId, Is.EqualTo(stored.Id));
            Assert.That(stored.Id.Version, Is.EqualTo(7));
            Assert.That(duplicate.Status, Is.EqualTo(kind == BillingWebhookEventKind.Unsupported
                ? BillingWebhookIngestionStatus.Ignored : BillingWebhookIngestionStatus.Duplicate));
            Assert.That(duplicate.InboxEventId, Is.EqualTo(stored.Id));
            Assert.That(stored.EventType, Is.EqualTo(eventType));
            Assert.That(stored.ReceivedAt, Is.EqualTo(ObservedAt));
            Assert.That(stored.ProcessedAt, Is.EqualTo(kind == BillingWebhookEventKind.Unsupported
                ? (DateTimeOffset?)ObservedAt : null));
        });
    }

    private static Task<ServiceTestBase<BillingWebhookIngestionService>> CreateAsync(
        BillingWebhookVerificationResult result)
    {
        return ServiceTestBase<BillingWebhookIngestionService>.CreateAsync(ObservedAt, services =>
        {
            services.RemoveAll<IBillingWebhookVerifier>();
            services.AddSingleton<IBillingWebhookVerifier>(new FixedVerifier(result));
        });
    }

    private sealed class FixedVerifier(BillingWebhookVerificationResult result) : IBillingWebhookVerifier
    {
        public BillingWebhookVerificationResult Verify(string payload, string signature)
        {
            return result;
        }
    }
}
