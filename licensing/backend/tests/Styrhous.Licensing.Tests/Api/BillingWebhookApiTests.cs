using Styrhous.Licensing.Persistence;
using System.Net;
using System.Net.Http.Json;
using System.Text;
using Microsoft.EntityFrameworkCore;
using Stripe;
using Styrhous.Licensing.Api.Billing;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookApiTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 12, 0, 0, TimeSpan.Zero);

    [Test]
    public async Task SignedSupportedEventIsPersistedFromUntouchedBodyWithoutAuthentication()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            ObservedAt);
        using var client = factory.CreateApiClient();
        const string payload = "  {\n  \"id\": \"evt_api_exact\", \"object\": \"event\", "
            + "\"created\": 1788177540, \"type\": \"invoice.paid\", "
            + "\"data\": { \"object\": {} }\n}  ";

        using var response = await SendAsync(client, payload);
        var body = await response.Content.ReadFromJsonAsync<BillingWebhookResponse>();

        await using var context = database.CreateContext();
        var inboxEvent = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body?.ReasonCode, Is.EqualTo(BillingWebhookReasonCodes.Received));
            Assert.That(body?.InboxEventId, Is.EqualTo(inboxEvent.Id));
            Assert.That(body?.InboxEventId?.Version, Is.EqualTo(7));
            Assert.That(inboxEvent.ExternalEventId, Is.EqualTo("evt_api_exact"));
            Assert.That(
                inboxEvent.EventType,
                Is.EqualTo(StripeBillingWebhookEventTypes.InvoicePaid));
            Assert.That(inboxEvent.Kind, Is.EqualTo(BillingWebhookEventKind.InvoicePaid));
            Assert.That(
                inboxEvent.OccurredAt,
                Is.EqualTo(DateTimeOffset.FromUnixTimeSeconds(1788177540)));
            Assert.That(inboxEvent.ReceivedAt, Is.EqualTo(ObservedAt));
            Assert.That(inboxEvent.NativeOutboxEnqueued, Is.True);
            Assert.That(context.Set<RebusOutboxMessage>().Count(message =>
                message.DestinationAddress == database.DatabaseName), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task BillingWebhookRemainsDurablyQueuedWhenForwardingIsDeferred()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            ObservedAt);
        using var client = factory.CreateApiClient();

        using var response = await SendAsync(
            client,
            Payload("evt_hint_failure", StripeBillingWebhookEventTypes.InvoicePaid));

        await using var context = database.CreateContext();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(stored.ProcessedAt, Is.Null);
            Assert.That(context.Set<RebusOutboxMessage>().Count(message =>
                message.DestinationAddress == database.DatabaseName), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task EveryRequiredBillingEventTypeIsAccepted()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, ObservedAt);
        using var client = factory.CreateApiClient();
        (string EventType, BillingWebhookEventKind Kind)[] requiredEvents =
        [
            (
                StripeBillingWebhookEventTypes.CheckoutSessionCompleted,
                BillingWebhookEventKind.CheckoutCompleted),
            (
                StripeBillingWebhookEventTypes.CustomerSubscriptionCreated,
                BillingWebhookEventKind.SubscriptionChanged),
            (
                StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                BillingWebhookEventKind.SubscriptionChanged),
            (
                StripeBillingWebhookEventTypes.CustomerSubscriptionDeleted,
                BillingWebhookEventKind.SubscriptionChanged),
            (
                StripeBillingWebhookEventTypes.InvoicePaid,
                BillingWebhookEventKind.InvoicePaid),
            (
                StripeBillingWebhookEventTypes.InvoicePaymentFailed,
                BillingWebhookEventKind.PaymentFailed),
        ];
        var responses = new List<BillingWebhookResponse>();

        for (var index = 0; index < requiredEvents.Length; index++)
        {
            using var response = await SendAsync(
                client,
                Payload($"evt_required_{index}", requiredEvents[index].EventType));
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            responses.Add(
                (await response.Content.ReadFromJsonAsync<BillingWebhookResponse>())!);
        }

        await using var context = database.CreateContext();
        var storedEvents = await context.BillingWebhookEvents
            .AsNoTracking()
            .OrderBy(webhookEvent => webhookEvent.ExternalEventId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                responses.Select(response => response.ReasonCode),
                Is.All.EqualTo(BillingWebhookReasonCodes.Received));
            Assert.That(
                responses.Select(response => response.InboxEventId),
                Is.All.Not.Null);
            Assert.That(
                storedEvents.Select(webhookEvent =>
                    (webhookEvent.EventType, webhookEvent.Kind)),
                Is.EqualTo(requiredEvents));
            Assert.That(
                storedEvents.Select(webhookEvent => webhookEvent.ProcessedAt),
                Is.All.Null);
        });
    }

    [Test]
    public async Task DuplicateDeliveryReturnsOriginalInboxIdentityWithoutChangingMetadata()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var firstFactory = new LicensingWebApplicationFactory(
            database,
            ObservedAt);
        using var firstClient = firstFactory.CreateApiClient();
        var payload = Payload(
            "evt_api_duplicate",
            StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated);
        using var firstResponse = await SendAsync(firstClient, payload);
        var firstBody = await firstResponse.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();

        using var duplicateFactory = new LicensingWebApplicationFactory(
            database,
            ObservedAt.AddHours(1));
        using var duplicateClient = duplicateFactory.CreateApiClient();
        var conflictingPayload = Payload(
            "evt_api_duplicate",
            StripeBillingWebhookEventTypes.InvoicePaymentFailed,
            createdUnixSeconds: 1788181140);
        using var duplicateResponse = await SendAsync(
            duplicateClient,
            conflictingPayload,
            signatureTimestamp: ObservedAt.AddHours(1));
        var duplicateBody = await duplicateResponse.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();

        await using var context = database.CreateContext();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(firstBody?.ReasonCode, Is.EqualTo(BillingWebhookReasonCodes.Received));
            Assert.That(
                duplicateBody?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.Duplicate));
            Assert.That(duplicateBody?.InboxEventId, Is.EqualTo(firstBody?.InboxEventId));
            Assert.That(stored.Id, Is.EqualTo(firstBody?.InboxEventId));
            Assert.That(
                stored.EventType,
                Is.EqualTo(StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated));
            Assert.That(
                stored.Kind,
                Is.EqualTo(BillingWebhookEventKind.SubscriptionChanged));
            Assert.That(
                stored.OccurredAt,
                Is.EqualTo(DateTimeOffset.FromUnixTimeSeconds(1788177540)));
            Assert.That(stored.ReceivedAt, Is.EqualTo(ObservedAt));
            Assert.That(context.Set<RebusOutboxMessage>().Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task UnsupportedSignedEventIsPersistedAsAlreadyProcessed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, ObservedAt);
        using var client = factory.CreateApiClient();
        var payload = Payload("evt_ignored", "charge.refunded");

        using var response = await SendAsync(client, payload);
        var body = await response.Content.ReadFromJsonAsync<BillingWebhookResponse>();

        await using var context = database.CreateContext();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(body?.ReasonCode, Is.EqualTo(BillingWebhookReasonCodes.Ignored));
            Assert.That(body?.InboxEventId, Is.EqualTo(stored.Id));
            Assert.That(stored.Kind, Is.EqualTo(BillingWebhookEventKind.Unsupported));
            Assert.That(stored.ProcessedAt, Is.EqualTo(ObservedAt));
        });
    }

    [Test]
    public async Task InvalidOrMissingSignatureIsRejectedWithoutPersistence()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var logs = new TestLogCollector();
        using var factory = new LicensingWebApplicationFactory(
            database,
            ObservedAt,
            logCollector: logs);
        using var client = factory.CreateApiClient();
        var payload = Payload(
            "evt_invalid_signature",
            StripeBillingWebhookEventTypes.InvoicePaid);

        using var invalidResponse = await SendAsync(
            client,
            payload,
            signature: "t=1788177600,v1=invalid");
        using var missingResponse = await SendAsync(client, payload, signature: null);
        var invalidBody = await invalidResponse.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();
        var missingBody = await missingResponse.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();

        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(invalidResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(missingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                invalidBody?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.InvalidSignature));
            Assert.That(
                missingBody?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.InvalidSignature));
            Assert.That(context.BillingWebhookEvents, Is.Empty);
            Assert.That(
                logs.Messages.Count(message => message.Contains(
                    $"Stripe webhook request rejected: {BillingWebhookReasonCodes.InvalidSignature}.",
                    StringComparison.Ordinal)),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task AuthenticallySignedMalformedJsonIsRejectedAsPayload()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var logs = new TestLogCollector();
        using var factory = new LicensingWebApplicationFactory(
            database,
            ObservedAt,
            logCollector: logs);
        using var client = factory.CreateApiClient();
        const string payload = "{not-json";

        using var response = await SendAsync(client, payload);
        var body = await response.Content.ReadFromJsonAsync<BillingWebhookResponse>();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(body?.ReasonCode, Is.EqualTo(BillingWebhookReasonCodes.InvalidPayload));
            Assert.That(
                logs.Messages,
                Has.Some.EqualTo(
                    $"Stripe webhook request failed: {BillingWebhookReasonCodes.InvalidPayload}."));
        });
    }

    [Test]
    public async Task MultipleSignatureHeaderValuesAreRejectedWithoutCrashing()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, ObservedAt);
        using var client = factory.CreateApiClient();
        using var request = new HttpRequestMessage(HttpMethod.Post, "/api/webhooks/stripe")
        {
            Content = new StringContent(
                Payload(
                    "evt_multiple_signatures",
                    StripeBillingWebhookEventTypes.InvoicePaid),
                Encoding.UTF8,
                "application/json"),
        };
        request.Headers.TryAddWithoutValidation(
            BillingWebhookEndpoints.SignatureHeaderName,
            ["invalid-one", "invalid-two"]);

        using var response = await client.SendAsync(request);
        var body = await response.Content.ReadFromJsonAsync<BillingWebhookResponse>();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                body?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.InvalidSignature));
        });
    }

    [Test]
    public async Task OversizedOrInvalidUtf8PayloadIsRejectedBeforeVerification()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, ObservedAt);
        using var client = factory.CreateApiClient();
        using var oversizedRequest = new HttpRequestMessage(
            HttpMethod.Post,
            "/api/webhooks/stripe")
        {
            Content = new ByteArrayContent(
                new byte[BillingWebhookEndpoints.MaximumPayloadSizeBytes + 1]),
        };
        using var invalidUtf8Request = new HttpRequestMessage(
            HttpMethod.Post,
            "/api/webhooks/stripe")
        {
            Content = new ByteArrayContent([0xff]),
        };

        using var oversizedResponse = await client.SendAsync(oversizedRequest);
        using var invalidUtf8Response = await client.SendAsync(invalidUtf8Request);
        var oversizedBody = await oversizedResponse.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();
        var invalidUtf8Body = await invalidUtf8Response.Content
            .ReadFromJsonAsync<BillingWebhookResponse>();

        Assert.Multiple(() =>
        {
            Assert.That(
                oversizedResponse.StatusCode,
                Is.EqualTo(HttpStatusCode.RequestEntityTooLarge));
            Assert.That(
                oversizedBody?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.PayloadTooLarge));
            Assert.That(invalidUtf8Response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                invalidUtf8Body?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.InvalidPayload));
        });
    }

    [Test]
    public async Task UnknownLengthPayloadIsStillBoundedWhileStreaming()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, ObservedAt);
        using var client = factory.CreateApiClient();
        using var request = new HttpRequestMessage(HttpMethod.Post, "/api/webhooks/stripe")
        {
            Content = new UnknownLengthContent(
                new byte[BillingWebhookEndpoints.MaximumPayloadSizeBytes + 1]),
        };

        using var response = await client.SendAsync(request);
        var body = await response.Content.ReadFromJsonAsync<BillingWebhookResponse>();

        Assert.Multiple(() =>
        {
            Assert.That(
                response.StatusCode,
                Is.EqualTo(HttpStatusCode.RequestEntityTooLarge));
            Assert.That(
                body?.ReasonCode,
                Is.EqualTo(BillingWebhookReasonCodes.PayloadTooLarge));
        });
    }

    private static async Task<HttpResponseMessage> SendAsync(
        HttpClient client,
        string payload,
        string? signature = "generated",
        DateTimeOffset? signatureTimestamp = null)
    {
        using var request = new HttpRequestMessage(HttpMethod.Post, "/api/webhooks/stripe")
        {
            Content = new StringContent(payload, Encoding.UTF8, "application/json"),
        };
        if (signature is not null)
        {
            request.Headers.Add(
                BillingWebhookEndpoints.SignatureHeaderName,
                signature == "generated"
                    ? EventUtility.GenerateSignatureHeader(
                        payload,
                        LicensingWebApplicationFactory.StripeWebhookSecret,
                        (signatureTimestamp ?? ObservedAt).ToUnixTimeSeconds())
                    : signature);
        }

        return await client.SendAsync(request);
    }

    private static string Payload(
        string id,
        string eventType,
        long createdUnixSeconds = 1788177540)
    {
        return $"{{\"id\":\"{id}\",\"object\":\"event\",\"created\":{createdUnixSeconds},"
        + $"\"type\":\"{eventType}\",\"data\":{{\"object\":{{}}}}}}";
    }

    private sealed class UnknownLengthContent(byte[] content) : HttpContent
    {

        protected override Task SerializeToStreamAsync(
            Stream stream,
            TransportContext? context)
        {
            return stream.WriteAsync(content.AsMemory()).AsTask();
        }

        protected override bool TryComputeLength(out long length)
        {
            length = 0;
            return false;
        }
    }
}
