using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCustomerPortalServiceTests
{
    private static readonly string[] PortalCustomer = ["cus_portal"];
    [TestCase(false)]
    [TestCase(true)]
    public async Task PreparedAccountUsesPersistedCustomerAndMapsProviderOutcome(bool fail)
    {
        var provider = new RecordingCustomerPortalProvider { Fail = fail };
        await using var test = await CreateAsync(provider);
        var signup = await SignUpAsync(test.Database);
        await ProjectAsync(test, signup.PersonalBillingAccountId);

        var result = await test.Service.CreateSessionAsync(
            signup.UserId, signup.PersonalBillingAccountId, CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(fail
                ? BillingCustomerPortalStatus.ProviderUnavailable
                : BillingCustomerPortalStatus.Created));
            Assert.That(result.RedirectUri, Is.EqualTo(fail
                ? null : new Uri("https://billing.stripe.test/session")));
            Assert.That(provider.ExternalCustomerIds, Is.EqualTo(PortalCustomer));
        });
    }

    [TestCase(BillingCustomerPortalStatus.BillingAccountNotFound)]
    [TestCase(BillingCustomerPortalStatus.InsufficientPermission)]
    [TestCase(BillingCustomerPortalStatus.SubscriptionNotFound)]
    public async Task PreparationRejectionDoesNotContactProvider(BillingCustomerPortalStatus expected)
    {
        var provider = new RecordingCustomerPortalProvider();
        await using var test = await CreateAsync(provider);
        var owner = await SignUpAsync(test.Database);
        var actor = owner.UserId;
        var account = owner.PersonalBillingAccountId;
        if (expected == BillingCustomerPortalStatus.BillingAccountNotFound)
        {
            account = Guid.NewGuid();
        }
        else if (expected == BillingCustomerPortalStatus.InsufficientPermission)
        {
            var admin = await SignUpAsync(test.Database, "admin", "admin@example.com");
            var organization = await CreateOrganizationAsync(test.Database, owner.UserId,
                "Portal permissions", SignupTime.AddDays(1));
            await AddOrganizationMemberAsync(test.Database, organization, admin.UserId,
                OrganizationRole.Admin);
            actor = admin.UserId;
            account = organization.BillingAccountId;
            await ProjectAsync(test, account);
        }

        var result = await test.Service.CreateSessionAsync(actor, account, CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(expected));
            Assert.That(result.RedirectUri, Is.Null);
            Assert.That(provider.ExternalCustomerIds, Is.Empty);
        });
    }

    [Test]
    public async Task ProviderCancellationRemainsCancellation()
    {
        await using var test = await CreateAsync(new RecordingCustomerPortalProvider { Cancel = true });
        var signup = await SignUpAsync(test.Database);
        await ProjectAsync(test, signup.PersonalBillingAccountId);
        Assert.That(async () => await test.Service.CreateSessionAsync(
            signup.UserId, signup.PersonalBillingAccountId, CancellationToken.None),
            Throws.TypeOf<OperationCanceledException>());
    }

    [Test]
    public async Task InsecureProviderRedirectFailsClosed()
    {
        await using var test = await CreateAsync(new RecordingCustomerPortalProvider
        {
            RedirectUri = new Uri("http://billing.stripe.test/session"),
        });
        var signup = await SignUpAsync(test.Database);
        await ProjectAsync(test, signup.PersonalBillingAccountId);
        Assert.That(async () => await test.Service.CreateSessionAsync(
            signup.UserId, signup.PersonalBillingAccountId, CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    private static Task<ServiceTestBase<BillingCustomerPortalService>> CreateAsync(
        IBillingCustomerPortalProvider provider)
    {
        return ServiceTestBase<BillingCustomerPortalService>.CreateAsync(SignupTime.AddDays(3), services =>
        {
            services.RemoveAll<IBillingCustomerPortalProvider>();
            services.AddSingleton(provider);
        });
    }

    private static async Task ProjectAsync(ServiceTestBase<BillingCustomerPortalService> test,
        Guid accountId)
    {
        await using var scope = test.CreateScope();
        await scope.ServiceProvider.GetRequiredService<CommercialSubscriptionProjectionService>()
            .ApplyAsync(accountId, new CommercialSubscriptionProjection(
                "cus_portal", "sub_portal", "price_test_monthly", CommercialSubscriptionStatus.Active,
                1, false, SignupTime, SignupTime.AddMonths(1), SignupTime.AddDays(2)),
                CancellationToken.None);
    }

    private sealed class RecordingCustomerPortalProvider : IBillingCustomerPortalProvider
    {
        public List<string> ExternalCustomerIds { get; } = [];

        public bool Fail { get; init; }

        public bool Cancel { get; init; }

        public Uri RedirectUri { get; init; } =
            new("https://billing.stripe.test/session");

        public Task<BillingCustomerPortalProviderSession> CreateSessionAsync(
            string externalCustomerId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ExternalCustomerIds.Add(externalCustomerId);
            if (Cancel)
            {
                throw new OperationCanceledException("The provider operation was cancelled.");
            }

            if (Fail)
            {
                throw new BillingCustomerPortalProviderUnavailableException(
                    "The provider is unavailable.");
            }

            return Task.FromResult(
                new BillingCustomerPortalProviderSession(RedirectUri));
        }
    }
}
