using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookEventTests
{
    [Test]
    public void ReceiveCreatesUuid7AndNormalizesTimestamps()
    {
        var occurredAt = new DateTimeOffset(2026, 8, 31, 13, 15, 0, TimeSpan.FromHours(2));
        var receivedAt = occurredAt.AddMinutes(1);

        var webhookEvent = BillingWebhookEvent.Receive(
            " evt_123 ",
            " invoice.paid ",
            BillingWebhookEventKind.InvoicePaid,
            occurredAt,
            receivedAt);

        Assert.Multiple(() =>
        {
            Assert.That(webhookEvent.Id.Version, Is.EqualTo(7));
            Assert.That(webhookEvent.ExternalEventId, Is.EqualTo("evt_123"));
            Assert.That(webhookEvent.EventType, Is.EqualTo("invoice.paid"));
            Assert.That(webhookEvent.Kind, Is.EqualTo(BillingWebhookEventKind.InvoicePaid));
            Assert.That(webhookEvent.OccurredAt, Is.EqualTo(occurredAt.ToUniversalTime()));
            Assert.That(webhookEvent.ReceivedAt, Is.EqualTo(receivedAt.ToUniversalTime()));
            Assert.That(webhookEvent.ProcessedAt, Is.Null);
        });
    }

    [TestCase(null, "invoice.paid", "externalEventId")]
    [TestCase("", "invoice.paid", "externalEventId")]
    [TestCase("evt_123", null, "eventType")]
    [TestCase("evt_123", "", "eventType")]
    public void ReceiveRejectsMissingProviderMetadata(
        string? externalEventId,
        string? eventType,
        string parameterName)
    {
        Assert.That(
            () => BillingWebhookEvent.Receive(
                externalEventId!,
                eventType!,
                BillingWebhookEventKind.InvoicePaid,
                DateTimeOffset.UtcNow,
                DateTimeOffset.UtcNow),
            Throws.InstanceOf<ArgumentException>()
                .With.Property("ParamName").EqualTo(parameterName));
    }

    [Test]
    public void ReceiveRejectsOversizedProviderMetadata()
    {
        Assert.Multiple(() =>
        {
            Assert.That(
                () => BillingWebhookEvent.Receive(
                    new string('e', BillingWebhookEvent.MaximumExternalIdentifierLength + 1),
                    "invoice.paid",
                    BillingWebhookEventKind.InvoicePaid,
                    DateTimeOffset.UtcNow,
                    DateTimeOffset.UtcNow),
                Throws.ArgumentException.With.Property("ParamName")
                    .EqualTo("externalEventId"));
            Assert.That(
                () => BillingWebhookEvent.Receive(
                    "evt_123",
                    new string('t', BillingWebhookEvent.MaximumEventTypeLength + 1),
                    BillingWebhookEventKind.InvoicePaid,
                    DateTimeOffset.UtcNow,
                    DateTimeOffset.UtcNow),
                Throws.ArgumentException.With.Property("ParamName").EqualTo("eventType"));
        });
    }

    [Test]
    public void ReceiveRejectsUnknownProviderNeutralKind()
    {
        Assert.That(
            () => BillingWebhookEvent.Receive(
                "evt_unknown_kind",
                "invoice.paid",
                (BillingWebhookEventKind)int.MaxValue,
                DateTimeOffset.UtcNow,
                DateTimeOffset.UtcNow),
            Throws.TypeOf<ArgumentOutOfRangeException>()
                .With.Property("ParamName").EqualTo("kind"));
    }

    [Test]
    public void ProcessingIsOneWayAndCannotPredateReceipt()
    {
        var receivedAt = DateTimeOffset.UtcNow;
        var webhookEvent = BillingWebhookEvent.Receive(
            "evt_processed",
            "charge.refunded",
            BillingWebhookEventKind.Unsupported,
            receivedAt.AddMinutes(-1),
            receivedAt);

        Assert.Multiple(() =>
        {
            Assert.That(
                () => webhookEvent.TryMarkProcessed(receivedAt.AddTicks(-1)),
                Throws.TypeOf<ArgumentOutOfRangeException>()
                    .With.Property("ParamName").EqualTo("processedAt"));
            Assert.That(webhookEvent.ProcessedAt, Is.Null);
            Assert.That(webhookEvent.TryMarkProcessed(receivedAt), Is.True);
            Assert.That(webhookEvent.ProcessedAt, Is.EqualTo(receivedAt));
            Assert.That(webhookEvent.TryMarkProcessed(receivedAt.AddMinutes(1)), Is.False);
            Assert.That(webhookEvent.ProcessedAt, Is.EqualTo(receivedAt));
        });
    }

    [Test]
    public void ProcessingLeaseCanBeReleasedOrReclaimedAfterExpiry()
    {
        var receivedAt = DateTimeOffset.UtcNow;
        var firstLeaseId = Guid.CreateVersion7();
        var secondLeaseId = Guid.CreateVersion7();
        var webhookEvent = BillingWebhookEvent.Receive(
            "evt_leased",
            "invoice.paid",
            BillingWebhookEventKind.InvoicePaid,
            receivedAt.AddMinutes(-1),
            receivedAt);

        Assert.Multiple(() =>
        {
            Assert.That(
                webhookEvent.TryAcquireProcessingLease(
                    firstLeaseId,
                    receivedAt,
                    receivedAt.AddMinutes(5)),
                Is.True);
            Assert.That(webhookEvent.ProcessingLeaseId, Is.EqualTo(firstLeaseId));
            Assert.That(webhookEvent.ProcessingAttemptCount, Is.EqualTo(1));
            Assert.That(
                webhookEvent.TryAcquireProcessingLease(
                    secondLeaseId,
                    receivedAt.AddMinutes(1),
                    receivedAt.AddMinutes(6)),
                Is.False);
            Assert.That(webhookEvent.TryReleaseProcessingLease(secondLeaseId), Is.False);
            Assert.That(webhookEvent.TryReleaseProcessingLease(firstLeaseId), Is.True);
            Assert.That(webhookEvent.ProcessingLeaseId, Is.Null);
            Assert.That(
                webhookEvent.TryAcquireProcessingLease(
                    secondLeaseId,
                    receivedAt.AddMinutes(6),
                    receivedAt.AddMinutes(11)),
                Is.True);
            Assert.That(webhookEvent.ProcessingLeaseId, Is.EqualTo(secondLeaseId));
            Assert.That(webhookEvent.ProcessingAttemptCount, Is.EqualTo(2));
        });
    }

    [Test]
    public void LeasedProcessingRequiresTheCurrentLease()
    {
        var receivedAt = DateTimeOffset.UtcNow;
        var leaseId = Guid.CreateVersion7();
        var webhookEvent = BillingWebhookEvent.Receive(
            "evt_complete",
            "invoice.paid",
            BillingWebhookEventKind.InvoicePaid,
            receivedAt.AddMinutes(-1),
            receivedAt);
        webhookEvent.TryAcquireProcessingLease(
            leaseId,
            receivedAt,
            receivedAt.AddMinutes(5));

        Assert.Multiple(() =>
        {
            Assert.That(
                webhookEvent.TryMarkProcessed(
                    Guid.CreateVersion7(),
                    receivedAt.AddMinutes(1)),
                Is.False);
            Assert.That(webhookEvent.ProcessedAt, Is.Null);
            Assert.That(
                webhookEvent.TryMarkProcessed(leaseId, receivedAt.AddMinutes(1)),
                Is.True);
            Assert.That(webhookEvent.ProcessedAt, Is.EqualTo(receivedAt.AddMinutes(1)));
            Assert.That(webhookEvent.ProcessingLeaseId, Is.Null);
            Assert.That(
                webhookEvent.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    receivedAt.AddMinutes(2),
                    receivedAt.AddMinutes(7)),
                Is.False);
        });
    }

    [Test]
    public void ProcessingLeasePreservesAnExistingIdentifier()
    {
        var receivedAt = DateTimeOffset.UtcNow;
        var webhookEvent = BillingWebhookEvent.Receive("evt_existing_lease", "invoice.paid",
            BillingWebhookEventKind.InvoicePaid, receivedAt.AddMinutes(-1), receivedAt);
        var leaseId = Guid.NewGuid();
        Assert.That(webhookEvent.TryAcquireProcessingLease(leaseId,
            receivedAt, receivedAt.AddMinutes(5)), Is.True);
        Assert.That(webhookEvent.ProcessingLeaseId, Is.EqualTo(leaseId));
    }

    [Test]
    public void ProcessingLeaseRejectsInvalidWindow()
    {
        var receivedAt = DateTimeOffset.UtcNow;
        var webhookEvent = BillingWebhookEvent.Receive(
            "evt_invalid_lease",
            "invoice.paid",
            BillingWebhookEventKind.InvoicePaid,
            receivedAt.AddMinutes(-1),
            receivedAt);

        Assert.Multiple(() =>
        {
            Assert.That(
                () => webhookEvent.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    receivedAt,
                    receivedAt),
                Throws.TypeOf<ArgumentOutOfRangeException>()
                    .With.Property("ParamName").EqualTo("expiresAt"));
            Assert.That(
                () => webhookEvent.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    receivedAt.AddTicks(-1),
                    receivedAt.AddMinutes(5)),
                Throws.TypeOf<ArgumentOutOfRangeException>()
                    .With.Property("ParamName").EqualTo("acquiredAt"));
        });
    }

}
