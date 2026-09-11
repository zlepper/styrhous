using System.Net;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[FixtureLifeCycle(LifeCycle.InstancePerTestCase)]
[Parallelizable(ParallelScope.All)]
public sealed class StripeBillingCustomerPortalProviderTests : IDisposable
{
    private readonly TestLogging _logging = new();

    public void Dispose()
    {
        _logging.Dispose();
    }

    private const string RestrictedConfigurationResponse = """
        {
          "id": "bpc_styrhous",
          "object": "billing_portal.configuration",
          "active": true,
          "features": {
            "subscription_update": {
              "enabled": false,
              "default_allowed_updates": [],
              "products": []
            }
          }
        }
        """;

    private static readonly string[] RequestFormKeys =
        ["configuration", "customer", "return_url"];

    private static readonly Uri ReturnUri =
        new("https://portal.styrhous.test/billing");

    [Test]
    public async Task CreatesRestrictedConfiguredPortalSessionForExactCustomer()
    {
        var httpClient = new RecordingStripeHttpClient(
            HttpStatusCode.OK,
            $$"""
            {
              "id": "bps_portal",
              "object": "billing_portal.session",
              "configuration": "bpc_styrhous",
              "customer": "cus_portal",
              "return_url": "{{ReturnUri}}",
              "url": "https://billing.stripe.test/p/session"
            }
            """);
        var provider = CreateProvider(httpClient);

        var session = await provider.CreateSessionAsync(
            "cus_portal",
            CancellationToken.None);

        var request = httpClient.Requests.Single(candidate =>
            candidate.Method == HttpMethod.Post);
        Assert.Multiple(() =>
        {
            Assert.That(httpClient.Requests, Has.Count.EqualTo(2));
            Assert.That(
                httpClient.Requests[0].Path,
                Is.EqualTo("/v1/billing_portal/configurations/bpc_styrhous"));
            Assert.That(request.Method, Is.EqualTo(HttpMethod.Post));
            Assert.That(request.Path, Is.EqualTo("/v1/billing_portal/sessions"));
            Assert.That(request.Form.Keys, Is.EquivalentTo(RequestFormKeys));
            Assert.That(request.Form["configuration"], Is.EqualTo("bpc_styrhous"));
            Assert.That(request.Form["customer"], Is.EqualTo("cus_portal"));
            Assert.That(request.Form["return_url"], Is.EqualTo(ReturnUri.ToString()));
            Assert.That(
                session.RedirectUri,
                Is.EqualTo(new Uri("https://billing.stripe.test/p/session")));
        });
    }

    [TestCase("", "https://portal.styrhous.test/billing")]
    [TestCase(" bpc_styrhous", "https://portal.styrhous.test/billing")]
    [TestCase("bpc_styrhous", "")]
    [TestCase("bpc_styrhous", "http://portal.styrhous.test/billing")]
    public void InvalidConfigurationFailsBeforeStripeCall(
        string configurationId,
        string returnUrl)
    {
        var httpClient = new RecordingStripeHttpClient(HttpStatusCode.OK, "{}");
        var provider = CreateProvider(
            httpClient,
            ValidOptions() with
            {
                CustomerPortalConfigurationId = configurationId,
                CustomerPortalReturnUrl = returnUrl,
            });

        Assert.That(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Requests, Is.Empty);
    }

    [TestCase("cus_different", "bpc_styrhous", "https://portal.styrhous.test/billing", "https://billing.stripe.test/p/session")]
    [TestCase("cus_portal", "bpc_different", "https://portal.styrhous.test/billing", "https://billing.stripe.test/p/session")]
    [TestCase("cus_portal", "bpc_styrhous", "https://portal.styrhous.test/elsewhere", "https://billing.stripe.test/p/session")]
    [TestCase("cus_portal", "bpc_styrhous", "https://portal.styrhous.test/billing", "http://billing.stripe.test/p/session")]
    public void MismatchedOrUnsafeStripeResponseFailsClosed(
        string customerId,
        string configurationId,
        string returnUrl,
        string redirectUrl)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.OK,
                $$"""
                {
                  "id": "bps_portal",
                  "object": "billing_portal.session",
                  "configuration": "{{configurationId}}",
                  "customer": "{{customerId}}",
                  "return_url": "{{returnUrl}}",
                  "url": "{{redirectUrl}}"
                }
                """));

        Assert.That(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    private static IEnumerable<string> InvalidSessionIdentifiers()
    {
        yield return string.Empty;
        yield return " ";
        yield return new string(
            's',
            CommercialSubscription.MaximumExternalIdentifierLength + 1);
    }

    [TestCaseSource(nameof(InvalidSessionIdentifiers))]
    public void InvalidStripeSessionIdentifierFailsClosed(string sessionId)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.OK,
                $$"""
                {
                  "id": "{{sessionId}}",
                  "object": "billing_portal.session",
                  "configuration": "bpc_styrhous",
                  "customer": "cus_portal",
                  "return_url": "https://portal.styrhous.test/billing",
                  "url": "https://billing.stripe.test/p/session"
                }
                """));

        Assert.That(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [TestCase(false, false)]
    [TestCase(true, true)]
    public void InactiveOrSubscriptionUpdatingConfigurationFailsClosed(
        bool active,
        bool subscriptionUpdatesEnabled)
    {
        var configurationBody = $$"""
            {
              "id": "bpc_styrhous",
              "object": "billing_portal.configuration",
              "active": {{active.ToString().ToLowerInvariant()}},
              "features": {
                "subscription_update": {
                  "enabled": {{subscriptionUpdatesEnabled.ToString().ToLowerInvariant()}},
                  "default_allowed_updates": [],
                  "products": []
                }
              }
            }
            """;
        var httpClient = new RecordingStripeHttpClient(
            HttpStatusCode.OK,
            "{}",
            configurationBody);
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Requests, Has.Count.EqualTo(1));
    }

    private static IEnumerable<string> MalformedConfigurationResponses()
    {
        yield return """
            {
              "id": "bpc_different",
              "object": "billing_portal.configuration",
              "active": true,
              "features": {
                "subscription_update": { "enabled": false }
              }
            }
            """;
        yield return """
            {
              "id": "bpc_styrhous",
              "object": "billing_portal.configuration",
              "active": true,
              "features": {}
            }
            """;
    }

    [TestCaseSource(nameof(MalformedConfigurationResponses))]
    public void MismatchedOrIncompleteConfigurationFailsBeforeSessionCreation(
        string configurationBody)
    {
        var httpClient = new RecordingStripeHttpClient(
            HttpStatusCode.OK,
            "{}",
            configurationBody);
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Requests, Has.Count.EqualTo(1));
    }

    [Test]
    public void InvalidProjectedCustomerFailsBeforeStripeCall()
    {
        var httpClient = new RecordingStripeHttpClient(HttpStatusCode.OK, "{}");
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.CreateSessionAsync(
                " cus_portal",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Requests, Is.Empty);
    }

    [Test]
    public void StripeOutageAndCallerCancellationAreDistinguished()
    {
        var logger = new RecordingLogger();
        var unavailable = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.ServiceUnavailable,
                "provider response must not leak"),
            logger: logger);
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var cancelled = CreateProvider(
            new RecordingStripeHttpClient(HttpStatusCode.OK, "{}"));

        Assert.Multiple(() =>
        {
            Assert.That(
                async () => await unavailable.CreateSessionAsync(
                    "cus_portal",
                    CancellationToken.None),
                Throws.TypeOf<BillingCustomerPortalProviderUnavailableException>());
            Assert.That(
                async () => await cancelled.CreateSessionAsync(
                    "cus_portal",
                    cancellation.Token),
                Throws.TypeOf<OperationCanceledException>());
            Assert.That(logger.Events, Has.Count.EqualTo(1));
            Assert.That(logger.Events[0].Level, Is.EqualTo(LogLevel.Warning));
            Assert.That(logger.Events[0].EventId.Id, Is.EqualTo(2403));
            Assert.That(
                logger.Events[0].Message,
                Does.Not.Contain("provider response must not leak"));
        });
    }

    [Test]
    public void PermanentStripeRejectionIsNotReportedAsATemporaryOutage()
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.BadRequest,
                "provider response must not leak"));

        var exception = Assert.ThrowsAsync<InvalidOperationException>(
            async () => await provider.CreateSessionAsync(
                "cus_portal",
                CancellationToken.None));

        Assert.That(
            exception!.Message,
            Does.Not.Contain("provider response must not leak"));
    }

    private StripeBillingCustomerPortalProvider CreateProvider(
        RecordingStripeHttpClient httpClient,
        StripeBillingOptions? options = null)
    {
        return CreateProvider(httpClient, _logging.GetLogger<StripeBillingCustomerPortalProvider>(), options);
    }

    private static StripeBillingCustomerPortalProvider CreateProvider(
        RecordingStripeHttpClient httpClient,
        ILogger<StripeBillingCustomerPortalProvider> logger,
        StripeBillingOptions? options = null)
    {
        return new(
            new StripeClient("sk_test_customer_portal", httpClient: httpClient),
            Options.Create(options ?? ValidOptions()), logger);
    }

    private static StripeBillingOptions ValidOptions()
    {
        return new()
        {
            CustomerPortalConfigurationId = "bpc_styrhous",
            CustomerPortalReturnUrl = ReturnUri.ToString(),
        };
    }

    private sealed class RecordingStripeHttpClient : IHttpClient
    {
        private readonly Queue<(HttpStatusCode StatusCode, string Body)> _responses;

        public RecordingStripeHttpClient(
            HttpStatusCode statusCode,
            string responseBody,
            string configurationBody = RestrictedConfigurationResponse)
        {
            _responses = new Queue<(HttpStatusCode, string)>(
            [
                (HttpStatusCode.OK, configurationBody),
                (statusCode, responseBody),
            ]);
        }

        public List<RecordedStripeRequest> Requests { get; } = [];

        public async Task<StripeResponse> MakeRequestAsync(
            StripeRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var content = request.Content is null
                ? string.Empty
                : await request.Content.ReadAsStringAsync(cancellationToken);
            Requests.Add(
                new RecordedStripeRequest(
                    request.Method,
                    request.Uri.AbsolutePath,
                    ParseForm(content)));
            var response = _responses.Dequeue();
            using var responseMessage = new HttpResponseMessage(response.StatusCode);
            return new StripeResponse(
                response.StatusCode,
                responseMessage.Headers,
                response.Body);
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
        IReadOnlyDictionary<string, string> Form);

    private sealed class RecordingLogger
        : ILogger<StripeBillingCustomerPortalProvider>
    {
        public List<RecordedLog> Events { get; } = [];

        public IDisposable? BeginScope<TState>(TState state)
            where TState : notnull
        {
            return null;
        }

        public bool IsEnabled(LogLevel logLevel)
        {
            return true;
        }

        public void Log<TState>(
            LogLevel logLevel,
            EventId eventId,
            TState state,
            Exception? exception,
            Func<TState, Exception?, string> formatter)
        {
            Events.Add(new RecordedLog(logLevel, eventId, formatter(state, exception)));
        }
    }

    private sealed record RecordedLog(
        LogLevel Level,
        EventId EventId,
        string Message);
}
