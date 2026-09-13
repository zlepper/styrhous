using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCustomerPortalPersistenceTests
{
    [Test]
    public async Task PersonalAndOrganizationOwnersResolveOnlyTheirProjectedCustomer()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "customer-portal-owner",
            "customer-portal-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Customer Portal owners",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(
            database,
            owner.PersonalBillingAccountId,
            "cus_personal_portal",
            "sub_personal_portal");
        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            "cus_organization_portal",
            "sub_organization_portal");
        await using var context = database.CreateContext();
        var store = new PostgresBillingCustomerPortalStore(context);

        var personal = await store.PrepareAsync(
            owner.UserId,
            owner.PersonalBillingAccountId,
            CancellationToken.None);
        var organizationResult = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                personal.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.Prepared));
            Assert.That(personal.ExternalCustomerId, Is.EqualTo("cus_personal_portal"));
            Assert.That(
                organizationResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.Prepared));
            Assert.That(
                organizationResult.ExternalCustomerId,
                Is.EqualTo("cus_organization_portal"));
        });
    }

    [Test]
    public async Task AuthorizationAndSubscriptionExistenceFailClosed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "customer-portal-authorization-owner",
            "customer-portal-authorization-owner@example.com");
        var administrator = await SignUpAsync(
            database,
            "customer-portal-authorization-admin",
            "customer-portal-authorization-admin@example.com");
        var outsider = await SignUpAsync(
            database,
            "customer-portal-authorization-outsider",
            "customer-portal-authorization-outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Customer Portal authorization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            administrator.UserId,
            OrganizationRole.Admin);
        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            "cus_authorization_portal",
            "sub_authorization_portal");
        await using var context = database.CreateContext();
        var store = new PostgresBillingCustomerPortalStore(context);

        var administratorResult = await store.PrepareAsync(
            administrator.UserId,
            organization.BillingAccountId,
            CancellationToken.None);
        var outsiderResult = await store.PrepareAsync(
            outsider.UserId,
            organization.BillingAccountId,
            CancellationToken.None);
        var otherPersonalResult = await store.PrepareAsync(
            owner.UserId,
            outsider.PersonalBillingAccountId,
            CancellationToken.None);
        var missingResult = await store.PrepareAsync(
            owner.UserId,
            Guid.CreateVersion7(),
            CancellationToken.None);
        var noSubscriptionResult = await store.PrepareAsync(
            owner.UserId,
            owner.PersonalBillingAccountId,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                administratorResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.InsufficientPermission));
            Assert.That(
                outsiderResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.BillingAccountNotFound));
            Assert.That(
                otherPersonalResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.BillingAccountNotFound));
            Assert.That(
                missingResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.BillingAccountNotFound));
            Assert.That(
                noSubscriptionResult.Status,
                Is.EqualTo(BillingCustomerPortalPreparationStatus.SubscriptionNotFound));
        });
    }

    [Test]
    public async Task UnknownActorRemainsAnAuthenticationFailure()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();
        var store = new PostgresBillingCustomerPortalStore(context);

        Assert.That(
            async () => await store.PrepareAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                CancellationToken.None),
            Throws.TypeOf<UserNotFoundException>());
    }

    private static async Task ProjectSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        string externalCustomerId,
        string externalSubscriptionId)
    {
        await using var serviceTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database,
            SignupTime,
            configureServices: null,
            configureHostServices: null);
        await serviceTest.Service
            .ApplyAsync(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    externalCustomerId,
                    externalSubscriptionId,
                    "price_test_monthly",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime.AddDays(2)),
                CancellationToken.None);
    }
}
