using System.Net;
using System.Globalization;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class StripeBillingCheckoutProviderTests
{
    private static readonly Uri SuccessUri =
        new("https://licenses.example.test/billing?checkout=success");

    private static readonly Uri CancelUri =
        new("https://licenses.example.test/billing?checkout=cancelled");

    private static readonly DateTimeOffset ExpiresAt =
        new(2026, 9, 1, 11, 0, 0, TimeSpan.Zero);

    [TestCase(BillingCadence.Monthly, "price_checkout_monthly", null)]
    [TestCase(BillingCadence.Annual, "price_checkout_annual", null)]
    [TestCase(BillingCadence.Monthly, "price_checkout_monthly", "cus_existing")]
    public async Task CreatesSubscriptionCheckoutFromTrustedConfiguration(
        BillingCadence cadence,
        string expectedPriceId,
        string? customerId)
    {
        var operationId = Guid.CreateVersion7();
        var billingAccountId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            HttpStatusCode.OK,
            """
            {
              "id": "cs_checkout_created",
              "object": "checkout.session",
              "created": 1788256800,
              "url": "https://checkout.stripe.test/c/pay/cs_checkout_created"
            }
            """);
        var provider = CreateProvider(httpClient);

        var result = await provider.CreateSessionAsync(
            new BillingCheckoutProviderRequest(
                operationId,
                billingAccountId,
                cadence,
                SeatQuantity: 37,
                ExpiresAt, customerId),
            CancellationToken.None);

        var request = httpClient.Requests.Single();
        Assert.Multiple(() =>
        {
            Assert.That(request.Method, Is.EqualTo(HttpMethod.Post));
            Assert.That(request.Path, Is.EqualTo("/v1/checkout/sessions"));
            Assert.That(request.Form["mode"], Is.EqualTo("subscription"));
            Assert.That(request.Form.GetValueOrDefault("customer"), Is.EqualTo(customerId));
            Assert.That(request.Form["success_url"], Is.EqualTo(SuccessUri.ToString()));
            Assert.That(request.Form["cancel_url"], Is.EqualTo(CancelUri.ToString()));
            Assert.That(request.Form["client_reference_id"], Is.EqualTo(operationId.ToString()));
            Assert.That(request.Form["line_items[0][price]"], Is.EqualTo(expectedPriceId));
            Assert.That(request.Form["line_items[0][quantity]"], Is.EqualTo("37"));
            Assert.That(request.Form["automatic_tax[enabled]"], Is.EqualTo("true"));
            Assert.That(
                request.Form["expires_at"],
                Is.EqualTo(ExpiresAt.ToUnixTimeSeconds().ToString(CultureInfo.InvariantCulture)));
            Assert.That(
                request.Form["metadata[styrhous_billing_operation_id]"],
                Is.EqualTo(operationId.ToString()));
            Assert.That(
                request.Form["subscription_data[metadata][styrhous_billing_account_id]"],
                Is.EqualTo(billingAccountId.ToString()));
            Assert.That(
                request.Form["subscription_data[metadata][styrhous_billing_operation_id]"],
                Is.EqualTo(operationId.ToString()));
            Assert.That(
                request.IdempotencyKey,
                Is.EqualTo($"styrhous-checkout-{operationId:N}"));
            Assert.That(result.ExternalSessionId, Is.EqualTo("cs_checkout_created"));
            Assert.That(
                result.RedirectUri,
                Is.EqualTo(
                    new Uri("https://checkout.stripe.test/c/pay/cs_checkout_created")));
        });
    }

    [Test]
    public void MissingOrUnsafeCheckoutConfigurationFailsBeforeProviderCall()
    {
        var configurations = new[]
        {
            new StripeBillingOptions(),
            ValidOptions() with { MonthlyPriceId = " " },
            ValidOptions() with { AnnualPriceId = " " },
            ValidOptions() with { MonthlyPriceId = " price_checkout_monthly" },
            ValidOptions() with { AnnualPriceId = "price_checkout_annual " },
            ValidOptions() with { AnnualPriceId = "price_checkout_monthly" },
            ValidOptions() with { CheckoutSuccessUrl = "http://licenses.example.test/success" },
            ValidOptions() with { CheckoutCancelUrl = "relative/cancel" },
        };

        foreach (var options in configurations)
        {
            var httpClient = new RecordingStripeHttpClient(HttpStatusCode.OK, "{}");
            var provider = new StripeBillingCheckoutProvider(
                new StripeClient("sk_test_checkout", httpClient: httpClient),
                Options.Create(options));

            Assert.That(
                async () => await provider.CreateSessionAsync(
                    new BillingCheckoutProviderRequest(
                        Guid.CreateVersion7(),
                        Guid.CreateVersion7(),
                        BillingCadence.Monthly,
                        SeatQuantity: 1,
                        ExpiresAt),
                    CancellationToken.None),
                Throws.InvalidOperationException);
            Assert.That(httpClient.Requests, Is.Empty);
        }
    }

    [Test]
    public void ProviderFailureIsMappedWithoutLeakingResponseBody()
    {
        const string sensitiveProviderBody = "provider response must not leak";
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.ServiceUnavailable,
                sensitiveProviderBody));

        var exception = Assert.ThrowsAsync<BillingCheckoutProviderUnavailableException>(
            async () => await provider.CreateSessionAsync(
                new BillingCheckoutProviderRequest(
                    Guid.CreateVersion7(),
                    Guid.CreateVersion7(),
                    BillingCadence.Monthly,
                    SeatQuantity: 1,
                    ExpiresAt),
                CancellationToken.None));

        Assert.That(exception!.Message, Does.Not.Contain(sensitiveProviderBody));
    }

    [Test]
    public void DefiniteExpiredRequestRejectionIsDistinguishedFromAnOutage()
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.BadRequest,
                """
                {
                  "error": {
                    "type": "invalid_request_error",
                    "message": "expires_at is too soon",
                    "param": "expires_at"
                  }
                }
                """));

        Assert.ThrowsAsync<BillingCheckoutProviderSessionCreationRejectedException>(
            async () => await provider.CreateSessionAsync(
                new BillingCheckoutProviderRequest(
                    Guid.CreateVersion7(),
                    Guid.CreateVersion7(),
                    BillingCadence.Monthly,
                    SeatQuantity: 1,
                    ExpiresAt),
                CancellationToken.None));
    }

    [TestCase("open", BillingCheckoutProviderSessionStatus.Open)]
    [TestCase("complete", BillingCheckoutProviderSessionStatus.Complete)]
    [TestCase("expired", BillingCheckoutProviderSessionStatus.Expired)]
    public async Task RetrievesAuthoritativeCheckoutSessionStatus(
        string stripeStatus,
        BillingCheckoutProviderSessionStatus expectedStatus)
    {
        var subscriptionJson = stripeStatus == "complete"
            ? "\"sub_status\""
            : "null";
        var httpClient = new RecordingStripeHttpClient(
            HttpStatusCode.OK,
            $$"""
            {
              "id": "cs_status",
              "object": "checkout.session",
              "status": "{{stripeStatus}}",
              "subscription": {{subscriptionJson}}
            }
            """);
        var provider = CreateProvider(httpClient);

        var state = await provider.GetSessionStateAsync(
            "cs_status",
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(state.Status, Is.EqualTo(expectedStatus));
            Assert.That(
                state.ExternalSubscriptionId,
                Is.EqualTo(stripeStatus == "complete" ? "sub_status" : null));
            Assert.That(httpClient.Requests.Single().Method, Is.EqualTo(HttpMethod.Get));
            Assert.That(
                httpClient.Requests.Single().Path,
                Is.EqualTo("/v1/checkout/sessions/cs_status"));
        });
    }

    [TestCase("cs_different", "open")]
    [TestCase("cs_status", "unexpected")]
    [TestCase("cs_status", "complete")]
    public void MalformedAuthoritativeCheckoutSessionStatusFailsClosed(
        string returnedSessionId,
        string returnedStatus)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.OK,
                $$"""
                {
                  "id": "{{returnedSessionId}}",
                  "object": "checkout.session",
                  "status": "{{returnedStatus}}"
                }
                """));

        Assert.ThrowsAsync<InvalidOperationException>(
            async () => await provider.GetSessionStateAsync(
                "cs_status",
                CancellationToken.None));
    }

    [Test]
    public void CheckoutSessionStatusProviderFailureAndCallerCancellationAreDistinguished()
    {
        var unavailable = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.ServiceUnavailable,
                "provider response must not leak"));
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var cancelled = CreateProvider(
            new RecordingStripeHttpClient(HttpStatusCode.OK, "{}"));

        Assert.Multiple(() =>
        {
            Assert.That(
                async () => await unavailable.GetSessionStateAsync(
                    "cs_status",
                    CancellationToken.None),
                Throws.TypeOf<BillingCheckoutProviderUnavailableException>());
            Assert.That(
                async () => await cancelled.GetSessionStateAsync(
                    "cs_status",
                    cancellation.Token),
                Throws.TypeOf<OperationCanceledException>());
        });
    }

    [TestCase("", "https://checkout.stripe.test/session")]
    [TestCase("cs_missing_url", "")]
    [TestCase("cs_insecure", "http://checkout.stripe.test/session")]
    public void MalformedProviderSessionFailsClosed(string sessionId, string redirectUrl)
    {
        var body = $$"""
        {
          "id": "{{sessionId}}",
          "object": "checkout.session",
          "url": "{{redirectUrl}}"
        }
        """;
        var provider = CreateProvider(
            new RecordingStripeHttpClient(HttpStatusCode.OK, body));

        Assert.That(
            async () => await provider.CreateSessionAsync(
                new BillingCheckoutProviderRequest(
                    Guid.CreateVersion7(),
                    Guid.CreateVersion7(),
                    BillingCadence.Monthly,
                    SeatQuantity: 1,
                    ExpiresAt),
                CancellationToken.None),
            Throws.InvalidOperationException);
    }

    [Test]
    public void CallerCancellationIsPropagated()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var provider = CreateProvider(
            new RecordingStripeHttpClient(HttpStatusCode.OK, "{}"));

        Assert.ThrowsAsync<OperationCanceledException>(
            async () => await provider.CreateSessionAsync(
                new BillingCheckoutProviderRequest(
                    Guid.CreateVersion7(),
                    Guid.CreateVersion7(),
                    BillingCadence.Monthly,
                    SeatQuantity: 1,
                    ExpiresAt),
                cancellation.Token));
    }

    private static StripeBillingCheckoutProvider CreateProvider(
        RecordingStripeHttpClient httpClient)
    {
        return new(
            new StripeClient("sk_test_checkout", httpClient: httpClient),
            Options.Create(ValidOptions()));
    }

    private static StripeBillingOptions ValidOptions()
    {
        return new()
        {
            MonthlyPriceId = "price_checkout_monthly",
            AnnualPriceId = "price_checkout_annual",
            CheckoutSuccessUrl = SuccessUri.ToString(),
            CheckoutCancelUrl = CancelUri.ToString(),
        };
    }

    private sealed class RecordingStripeHttpClient(
        HttpStatusCode statusCode,
        string responseBody) : IHttpClient
    {
        public List<RecordedStripeRequest> Requests { get; } = [];

        public async Task<StripeResponse> MakeRequestAsync(
            StripeRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var content = request.Content is null
                ? string.Empty
                : await request.Content.ReadAsStringAsync(cancellationToken);
            request.StripeHeaders.TryGetValue("Idempotency-Key", out var idempotencyKey);
            Requests.Add(
                new RecordedStripeRequest(
                    request.Method,
                    request.Uri.AbsolutePath,
                    ParseForm(content),
                    idempotencyKey));
            using var responseMessage = new HttpResponseMessage(statusCode);
            return new StripeResponse(statusCode, responseMessage.Headers, responseBody);
        }

        public Task<StripeStreamedResponse> MakeStreamingRequestAsync(
            StripeRequest request,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException("Streaming is not used by billing tests.");
        }

        private static Dictionary<string, string> ParseForm(string content)
        {
            return content.Split('&', StringSplitOptions.RemoveEmptyEntries)
                .Select(pair => pair.Split('=', 2))
                .ToDictionary(
                    pair => Decode(pair[0]),
                    pair => pair.Length == 2 ? Decode(pair[1]) : string.Empty,
                    StringComparer.Ordinal);
        }

        private static string Decode(string value)
        {
            return Uri.UnescapeDataString(value.Replace('+', ' '));
        }
    }

    private sealed record RecordedStripeRequest(
        HttpMethod Method,
        string Path,
        IReadOnlyDictionary<string, string> Form,
        string? IdempotencyKey);
}
