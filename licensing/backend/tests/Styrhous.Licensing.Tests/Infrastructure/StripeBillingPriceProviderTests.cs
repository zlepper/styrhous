using System.Net;
using System.Text.Json;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Infrastructure.Billing;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class StripeBillingPriceProviderTests
{
    private static readonly string[] Cadences = ["monthly", "annual"];
    private static readonly string[] PricePaths = ["/v1/prices/price_monthly", "/v1/prices/price_annual"];

    [TestCase("inclusive", true)]
    [TestCase("exclusive", false)]
    [TestCase("unspecified", null)]
    public async Task PreservesWhetherTaxTreatmentIsKnown(string taxBehavior, bool? expected)
    {
        var prices = await CreateProvider(new PriceHttpClient("usd", 1200, taxBehavior: taxBehavior))
            .ListAsync(CancellationToken.None);
        Assert.That(prices.Select(price => price.TaxIncluded), Is.All.EqualTo(expected));
    }

    [TestCase("usd", 1200, 12)]
    [TestCase("jpy", 1200, 1200)]
    [TestCase("isk", 1200, 12)]
    [TestCase("ugx", 1200, 12)]
    public async Task ReadsConfiguredRecurringPricesInDisplayUnits(string currency, int amount, int expected)
    {
        var http = new PriceHttpClient(currency, amount);
        var prices = await CreateProvider(http).ListAsync(CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(prices.Select(price => price.Cadence), Is.EqualTo(Cadences));
            Assert.That(prices.Select(price => price.UnitAmount), Is.All.EqualTo(expected));
            Assert.That(prices.Select(price => price.Currency), Is.All.EqualTo(currency.ToUpperInvariant()));
            Assert.That(prices.Select(price => price.TaxIncluded), Is.All.True);
            Assert.That(http.Paths, Is.EquivalentTo(PricePaths));
        });
    }

    [TestCase("tiered", "month", true)]
    [TestCase("per_unit", "week", true)]
    [TestCase("per_unit", "month", false)]
    public void RejectsPricesThatCannotBeRepresentedAsARecurringSeatAmount(string scheme, string interval, bool active)
    {
        var provider = CreateProvider(new PriceHttpClient("usd", 1200, scheme, interval, active));
        Assert.ThrowsAsync<BillingPricesUnavailableException>(() => provider.ListAsync(CancellationToken.None));
    }

    [Test]
    public void ReportsProviderFailureWithoutReturningPartialPrices()
    {
        var provider = CreateProvider(new PriceHttpClient("usd", 1200, status: HttpStatusCode.ServiceUnavailable));
        Assert.ThrowsAsync<BillingPricesUnavailableException>(() => provider.ListAsync(CancellationToken.None));
    }

    [Test]
    public void PreservesCallerCancellation()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        Assert.ThrowsAsync<OperationCanceledException>(() =>
            CreateProvider(new PriceHttpClient("usd", 1200)).ListAsync(cancellation.Token));
    }

    private static StripeBillingPriceProvider CreateProvider(IHttpClient http)
    {
        return new(
        new StripeClient("sk_test_prices", httpClient: http),
        Options.Create(new StripeBillingOptions { MonthlyPriceId = "price_monthly", AnnualPriceId = "price_annual" }));
    }

    private sealed class PriceHttpClient(
        string currency,
        int amount,
        string scheme = "per_unit",
        string monthlyInterval = "month",
        bool active = true,
        HttpStatusCode status = HttpStatusCode.OK,
        string taxBehavior = "inclusive") : IHttpClient
    {
        public List<string> Paths { get; } = [];

        public Task<StripeResponse> MakeRequestAsync(StripeRequest request, CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Paths.Add(request.Uri.AbsolutePath);
            var annual = request.Uri.AbsolutePath.EndsWith("price_annual", StringComparison.Ordinal);
            using var response = new HttpResponseMessage(status);
            return Task.FromResult(new StripeResponse(status, response.Headers, JsonSerializer.Serialize(new
            {
                id = annual ? "price_annual" : "price_monthly",
                @object = "price",
                active,
                billing_scheme = scheme,
                currency,
                unit_amount_decimal = amount.ToString(System.Globalization.CultureInfo.InvariantCulture),
                tax_behavior = taxBehavior,
                recurring = new { interval = annual ? "year" : monthlyInterval, interval_count = 1, usage_type = "licensed" },
            })));
        }

        public Task<StripeStreamedResponse> MakeStreamingRequestAsync(StripeRequest request, CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }
    }
}
