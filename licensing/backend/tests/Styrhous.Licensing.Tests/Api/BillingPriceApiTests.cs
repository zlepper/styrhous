using System.Net;
using System.Net.Http.Json;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.AspNetCore.TestHost;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingPriceApiTests
{
    [TestCase(false, false, HttpStatusCode.Unauthorized)]
    [TestCase(true, false, HttpStatusCode.OK)]
    [TestCase(true, true, HttpStatusCode.ServiceUnavailable)]
    public async Task PricesRequireSignInAndDoNotExposeProviderFailures(
        bool authenticated,
        bool unavailable,
        HttpStatusCode expected)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var configured = factory.WithWebHostBuilder(builder => builder.ConfigureTestServices(services =>
        {
            services.RemoveAll<IBillingPriceProvider>();
            services.AddSingleton<IBillingPriceProvider>(new PriceProvider(unavailable));
        }));
        using var client = configured.CreateClient(new WebApplicationFactoryClientOptions
        {
            BaseAddress = new Uri("https://localhost"),
            AllowAutoRedirect = false,
        });
        if (authenticated)
        {
            var user = await SignUpAsync(database, "price-user", "price-user@example.com");
            client.DefaultRequestHeaders.Add(LicensingWebApplicationFactory.UserIdHeader, user.UserId.ToString());
        }

        using var response = await client.GetAsync("/api/billing-prices");
        Assert.That(response.StatusCode, Is.EqualTo(expected));
        if (response.IsSuccessStatusCode)
        {
            var body = await response.Content.ReadFromJsonAsync<PriceResponse>();
            Assert.That(body?.Prices.Single(), Is.EqualTo(new BillingPrice("monthly", 12m, "USD", false)));
        }
        else if (unavailable)
        {
            var body = await response.Content.ReadAsStringAsync();
            Assert.Multiple(() =>
            {
                Assert.That(body, Does.Contain("billing_prices_unavailable"));
                Assert.That(body, Does.Not.Contain("provider-internal-detail"));
            });
        }
    }

    private sealed record PriceResponse(BillingPrice[] Prices);

    private sealed class PriceProvider(bool unavailable) : IBillingPriceProvider
    {
        public Task<IReadOnlyList<BillingPrice>> ListAsync(CancellationToken cancellationToken)
        {
            return unavailable
                ? throw new BillingPricesUnavailableException(new InvalidOperationException("provider-internal-detail"))
                : Task.FromResult<IReadOnlyList<BillingPrice>>([new("monthly", 12m, "USD", false)]);
        }
    }
}
