using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class StripeBillingWebhookVerifierTests
{
    private const string WebhookSecret = "whsec_verifier_test";

    private const string RecordedFixtureSecret = "whsec_recorded_fixture";

    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 12, 0, 0, TimeSpan.Zero);

    [Test]
    public void ValidSignaturePreservesExactEventMetadata()
    {
        const string payload = "  {\n  \"id\": \"evt_exact\", \"object\": \"event\", "
            + "\"created\": 1788177540, \"type\": \"invoice.paid\", "
            + "\"data\": { \"object\": {} }\n}  ";
        var verifier = CreateVerifier();

        var result = verifier.Verify(payload, Sign(payload));

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingWebhookVerificationStatus.Verified));
            Assert.That(result.Event?.ExternalEventId, Is.EqualTo("evt_exact"));
            Assert.That(result.Event?.EventType, Is.EqualTo("invoice.paid"));
            Assert.That(result.Event?.Kind, Is.EqualTo(BillingWebhookEventKind.InvoicePaid));
            Assert.That(
                result.Event?.OccurredAt,
                Is.EqualTo(DateTimeOffset.FromUnixTimeSeconds(1788177540)));
        });
    }

    [Test]
    public void RecordedSignedSnapshotFixtureIsAcceptedWithoutRegeneratingItsSignature()
    {
        var fixtureDirectory = Path.Combine(
            TestContext.CurrentContext.TestDirectory,
            "Fixtures",
            "Stripe");
        var payload = System.IO.File.ReadAllText(
            Path.Combine(fixtureDirectory, "invoice-paid.json"));
        var signature = System.IO.File.ReadAllText(
            Path.Combine(fixtureDirectory, "invoice-paid.signature.txt"));

        var result = CreateVerifier(RecordedFixtureSecret).Verify(
            payload,
            signature.TrimEnd());

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingWebhookVerificationStatus.Verified));
            Assert.That(
                result.Event,
                Is.EqualTo(
                    new VerifiedBillingWebhookEvent(
                        "evt_fixture_invoice_paid",
                        StripeBillingWebhookEventTypes.InvoicePaid,
                        BillingWebhookEventKind.InvoicePaid,
                        DateTimeOffset.FromUnixTimeSeconds(1788177540))));
        });
    }

    [Test]
    public void ModifiedPayloadFailsTheOriginalSignature()
    {
        var original = Payload("evt_original", StripeBillingWebhookEventTypes.InvoicePaid);
        var modified = Payload("evt_modified", StripeBillingWebhookEventTypes.InvoicePaid);

        var result = CreateVerifier().Verify(modified, Sign(original));

        Assert.That(
            result.Status,
            Is.EqualTo(BillingWebhookVerificationStatus.InvalidSignature));
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase("not-a-stripe-signature")]
    public void MissingOrMalformedSignatureIsRejected(string? signature)
    {
        var result = CreateVerifier().Verify(
            Payload("evt_signature", StripeBillingWebhookEventTypes.InvoicePaid),
            signature!);

        Assert.That(
            result.Status,
            Is.EqualTo(BillingWebhookVerificationStatus.InvalidSignature));
    }

    [Test]
    public void SignatureOutsideToleranceIsRejected()
    {
        var payload = Payload("evt_stale", StripeBillingWebhookEventTypes.InvoicePaid);
        var staleTimestamp = ObservedAt.AddMinutes(-6).ToUnixTimeSeconds();

        var result = CreateVerifier().Verify(
            payload,
            EventUtility.GenerateSignatureHeader(
                payload,
                WebhookSecret,
                staleTimestamp));

        Assert.That(
            result.Status,
            Is.EqualTo(BillingWebhookVerificationStatus.InvalidSignature));
    }

    [TestCase("{}")]
    [TestCase("{\"id\":\"evt_missing\",\"created\":1788177540}")]
    [TestCase("{\"id\":\"evt_bad_created\",\"type\":\"invoice.paid\",\"created\":\"now\"}")]
    [TestCase("{not-json")]
    public void SignedMalformedEventMetadataIsRejectedAsPayload(string payload)
    {
        var result = CreateVerifier().Verify(payload, Sign(payload));

        Assert.That(
            result.Status,
            Is.EqualTo(BillingWebhookVerificationStatus.InvalidPayload));
    }

    [Test]
    public void SignedMetadataOutsideEveryBoundIsRejectedAsPayload()
    {
        var payloads = new[]
        {
            "{\"id\":\"\",\"type\":\"invoice.paid\",\"created\":1788177540}",
            "{\"id\":\" evt_trimmed\",\"type\":\"invoice.paid\",\"created\":1788177540}",
            $"{{\"id\":\"{new string('e', 256)}\",\"type\":\"invoice.paid\",\"created\":1788177540}}",
            "{\"id\":\"evt_blank_type\",\"type\":\" \",\"created\":1788177540}",
            $"{{\"id\":\"evt_long_type\",\"type\":\"{new string('t', 129)}\",\"created\":1788177540}}",
            "{\"id\":\"evt_fractional\",\"type\":\"invoice.paid\",\"created\":1.5}",
            "{\"id\":\"evt_range\",\"type\":\"invoice.paid\",\"created\":9223372036854775807}",
        };

        Assert.That(
            payloads.Select(payload => CreateVerifier().Verify(payload, Sign(payload)).Status),
            Is.All.EqualTo(BillingWebhookVerificationStatus.InvalidPayload));
    }

    [Test]
    public void MissingWebhookSecretFailsClosedWithoutDisclosingIt()
    {
        var verifier = new StripeBillingWebhookVerifier(
            Options.Create(new StripeBillingOptions()),
            new FixedTimeProvider(ObservedAt));

        Assert.That(
            () => verifier.Verify("{}", "signature"),
            Throws.InvalidOperationException.With.Message
                .EqualTo("Stripe:WebhookSecret is required."));
    }

    private static StripeBillingWebhookVerifier CreateVerifier(
        string webhookSecret = WebhookSecret)
    {
        return new(
            Options.Create(
                new StripeBillingOptions
                {
                    WebhookSecret = webhookSecret,
                }),
            new FixedTimeProvider(ObservedAt));
    }

    private static string Sign(string payload)
    {
        return EventUtility.GenerateSignatureHeader(
            payload,
            WebhookSecret,
            ObservedAt.ToUnixTimeSeconds());
    }

    private static string Payload(string id, string eventType)
    {
        return $"{{\"id\":\"{id}\",\"object\":\"event\",\"created\":1788177540,"
        + $"\"type\":\"{eventType}\",\"data\":{{\"object\":{{}}}}}}";
    }
}
