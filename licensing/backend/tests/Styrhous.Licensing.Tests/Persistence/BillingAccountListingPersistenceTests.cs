using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingAccountListingPersistenceTests
{
    private static readonly string[] OrderedAccountNames =
        ["Personal plan", "First by UUID", "Second by UUID"];

    [TestCase(0, true)]
    [TestCase(30, false)]
    public async Task TrialActivityUsesTheHalfOpenPersistedWindow(
        int daysAfterSignup,
        bool expectedActive)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"billing-trial-{daysAfterSignup}",
            $"trial-{daysAfterSignup}@example.com");

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(daysAfterSignup));
        var account = (await listingTest.Service
            .ListForUserAsync(signup.UserId))
            .Single();

        Assert.That(account.Trial!.IsActive, Is.EqualTo(expectedActive));
    }

    [TestCase(CommercialSubscriptionStatus.Trialing, 1, 31)]
    [TestCase(CommercialSubscriptionStatus.Active, 4, 31)]
    [TestCase(CommercialSubscriptionStatus.Active, -31, -1)]
    public async Task IneligibleSubscriptionFallsBackToTheActiveTrial(
        CommercialSubscriptionStatus status,
        int periodStartDay,
        int periodEndDay)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"billing-fallback-{status}-{periodStartDay}",
            $"fallback-{status}-{periodStartDay}@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service
                .ApplyAsync(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        $"cus_fallback_{status}_{periodStartDay}",
                        $"sub_fallback_{status}_{periodStartDay}",
                        $"price_fallback_{status}_{periodStartDay}",
                        status,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(periodStartDay),
                        SignupTime.AddDays(periodEndDay),
                        SignupTime.AddDays(2)));
        }

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var account = (await listingTest.Service
            .ListForUserAsync(signup.UserId)).Single();

        Assert.Multiple(() =>
        {
            Assert.That(account.Entitlement.State, Is.EqualTo(EntitlementState.Trial));
            Assert.That(account.Entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(account.Entitlement.IsEligible, Is.True);
            Assert.That(account.Trial!.IsActive, Is.True);
            Assert.That(account.Subscription!.Status, Is.EqualTo(status));
        });
    }

    [Test]
    public async Task OwnerListsOnlyAccessibleAccountsWithCurrentBillingState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "billing-owner", "owner@example.com");
        var member = await SignUpAsync(database, "billing-member", "member@example.com");
        var outsider = await SignUpAsync(database, "billing-outsider", "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Northstar Platform",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            "Hidden Organization",
            SignupTime.AddDays(1));
        var projection = new CommercialSubscriptionProjection(
            "cus_private",
            "sub_private",
            "price_private",
            CommercialSubscriptionStatus.PastDue,
            seatQuantity: 4,
            cancelAtPeriodEnd: true,
            SignupTime.AddDays(1),
            SignupTime.AddDays(31),
            SignupTime.AddDays(3));
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service
                .ApplyAsync(organization.BillingAccountId, projection);
        }

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var accounts = await listingTest.Service.ListForUserAsync(owner.UserId);

        Assert.That(accounts, Has.Count.EqualTo(2));
        var personal = accounts[0];
        var organizationAccount = accounts[1];
        Assert.Multiple(() =>
        {
            Assert.That(personal.BillingAccountId, Is.EqualTo(owner.PersonalBillingAccountId));
            Assert.That(personal.AccountKind, Is.EqualTo(BillingAccountKind.Personal));
            Assert.That(personal.OrganizationId, Is.Null);
            Assert.That(personal.OrganizationName, Is.Null);
            Assert.That(personal.OrganizationRole, Is.Null);
            Assert.That(personal.CanManageBilling, Is.True);
            Assert.That(personal.AssignedSeatCount, Is.EqualTo(1));
            Assert.That(personal.Entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(personal.Entitlement.IsEligible, Is.False);
            Assert.That(personal.Trial, Is.Null);
            Assert.That(personal.Subscription, Is.Null);

            Assert.That(
                organizationAccount.BillingAccountId,
                Is.EqualTo(organization.BillingAccountId));
            Assert.That(
                organizationAccount.AccountKind,
                Is.EqualTo(BillingAccountKind.Organization));
            Assert.That(
                organizationAccount.OrganizationId,
                Is.EqualTo(organization.OrganizationId));
            Assert.That(organizationAccount.OrganizationName, Is.EqualTo("Northstar Platform"));
            Assert.That(organizationAccount.OrganizationRole, Is.EqualTo(OrganizationRole.Owner));
            Assert.That(organizationAccount.CanManageBilling, Is.True);
            Assert.That(organizationAccount.AssignedSeatCount, Is.EqualTo(2));
            Assert.That(organizationAccount.Entitlement.State, Is.EqualTo(EntitlementState.Grace));
            Assert.That(organizationAccount.Entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionPastDue));
            Assert.That(organizationAccount.Entitlement.IsEligible, Is.True);
            Assert.That(organizationAccount.Trial, Is.Not.Null);
            Assert.That(organizationAccount.Trial!.TrialId.Version, Is.EqualTo(7));
            Assert.That(organizationAccount.Trial.StartedAt, Is.EqualTo(SignupTime));
            Assert.That(organizationAccount.Trial.EndsAt, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(
                organizationAccount.Trial.TerminatedAt,
                Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(organizationAccount.Trial.TransferredAt, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(organizationAccount.Trial.IsActive, Is.False);
            Assert.That(organizationAccount.Subscription, Is.Not.Null);
            Assert.That(organizationAccount.Subscription!.SubscriptionId.Version, Is.EqualTo(7));
            Assert.That(
                organizationAccount.Subscription.Status,
                Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(organizationAccount.Subscription.SeatQuantity, Is.EqualTo(4));
            Assert.That(organizationAccount.Subscription.CancelAtPeriodEnd, Is.True);
            Assert.That(
                organizationAccount.Subscription.CurrentPeriodStartedAt,
                Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(
                organizationAccount.Subscription.CurrentPeriodEndsAt,
                Is.EqualTo(SignupTime.AddDays(31)));
            Assert.That(
                organizationAccount.Subscription.ProjectedAt,
                Is.EqualTo(SignupTime.AddDays(3)));
        });
    }

    [Test]
    public async Task PersonalAccountComesFirstAndEqualMembershipTimesUseUuidOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var actor = await SignUpAsync(database, "billing-order-actor", "actor@example.com");
        var firstOwner = await SignUpAsync(
            database,
            "billing-order-first-owner",
            "first-owner@example.com");
        var secondOwner = await SignUpAsync(
            database,
            "billing-order-second-owner",
            "second-owner@example.com");
        var firstOrganization = await CreateOrganizationAsync(
            database,
            firstOwner.UserId,
            "First by UUID",
            SignupTime.AddDays(1));
        var secondOrganization = await CreateOrganizationAsync(
            database,
            secondOwner.UserId,
            "Second by UUID",
            SignupTime.AddDays(1));
        var earlierMembershipId = Guid.Parse("0191a8f0-1111-7000-8000-000000000011");
        var laterMembershipId = Guid.Parse("0191a8f0-1111-7000-8000-000000000022");
        await AddOrganizationMemberAsync(
            database,
            secondOrganization,
            actor.UserId,
            OrganizationRole.Member,
            membershipId: laterMembershipId,
            joinedAt: SignupTime.AddDays(2));
        await AddOrganizationMemberAsync(
            database,
            firstOrganization,
            actor.UserId,
            OrganizationRole.Member,
            membershipId: earlierMembershipId,
            joinedAt: SignupTime.AddDays(2));

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var accounts = await listingTest.Service.ListForUserAsync(actor.UserId);

        Assert.That(
            accounts.Select(account => account.OrganizationName ?? "Personal plan"),
            Is.EqualTo(OrderedAccountNames));
    }

    [Test]
    public async Task AccountsAndEntitlementsComeFromOneRepeatableReadSnapshot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var actor = await SignUpAsync(database, "billing-snapshot-actor", "actor@example.com");
        var owner = await SignUpAsync(database, "billing-snapshot-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Joined During Read",
            SignupTime.AddDays(1));
        var gate = new DatabaseCommandGate();
        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [
            new DatabaseCommandGateInterceptor(
                gate,
                "FROM seats AS s",
                DatabaseCommandInterceptionPhase.AfterReaderExecution)]);
        var service = listingTest.Service;

        var listingTask = service.ListForUserAsync(actor.UserId);
        await gate.WaitUntilReachedAsync();
        try
        {
            await AddOrganizationMemberAsync(
                database,
                organization,
                actor.UserId,
                OrganizationRole.Member,
                joinedAt: SignupTime.AddDays(2));
        }
        finally
        {
            gate.Release();
        }

        var duringWrite = await listingTask;
        await using var listingTest2 = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var afterWrite = await listingTest2.Service.ListForUserAsync(actor.UserId);

        Assert.Multiple(() =>
        {
            Assert.That(duringWrite, Has.Count.EqualTo(1));
            Assert.That(duringWrite.Single().AccountKind,
                Is.EqualTo(BillingAccountKind.Personal));
            Assert.That(afterWrite, Has.Count.EqualTo(2));
            Assert.That(afterWrite[1].OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(afterWrite[1].Entitlement.SeatId.Version, Is.EqualTo(7));
        });
    }

    [Test]
    public async Task OrganizationMemberCanViewStatusButCannotManageBilling()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "billing-access-owner", "owner@example.com");
        var member = await SignUpAsync(database, "billing-access-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Shared Billing",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var accounts = await listingTest.Service.ListForUserAsync(member.UserId);

        Assert.That(accounts, Has.Count.EqualTo(2));
        Assert.Multiple(() =>
        {
            Assert.That(accounts[0].AccountKind, Is.EqualTo(BillingAccountKind.Personal));
            Assert.That(accounts[0].CanManageBilling, Is.True);
            Assert.That(accounts[1].BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(accounts[1].OrganizationRole, Is.EqualTo(OrganizationRole.Member));
            Assert.That(accounts[1].CanManageBilling, Is.False);
        });
    }

    [Test]
    public async Task StaleUserIsRejectedWithoutReturningBillingState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await SignUpAsync(database, "billing-existing", "existing@example.com");

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await listingTest.Service.ListForUserAsync(Guid.CreateVersion7()));
    }


}
