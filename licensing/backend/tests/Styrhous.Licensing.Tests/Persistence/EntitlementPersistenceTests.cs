using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class EntitlementPersistenceTests
{
    private static readonly DateTimeOffset SignupTime =
        new(2026, 8, 30, 10, 15, 0, TimeSpan.Zero);

    [TestCase(0, EntitlementState.Trial, EntitlementReasonCodes.ActiveTrial)]
    [TestCase(30, EntitlementState.Evaluation, EntitlementReasonCodes.TrialExpired)]
    public async Task PersonalSeatReflectsPersistedTrialAtResolutionTime(
        int daysAfterSignup,
        EntitlementState expectedState,
        string expectedReasonCode)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(daysAfterSignup));
        var entitlements = await entitlementTest.Service
            .ListForUserAsync(signup.UserId);

        var entitlement = entitlements.Single();
        Assert.Multiple(() =>
        {
            Assert.That(entitlement.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(entitlement.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(entitlement.State, Is.EqualTo(expectedState));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(expectedReasonCode));
            Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(30)));
        });
    }

    [Test]
    public async Task PaidSubscriptionOverridesPersistedTrialForEveryAssignedSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        var subscription = CommercialSubscription.Create(
            signup.PersonalBillingAccountId,
            new CommercialSubscriptionProjection(
                "cus_paid",
                "sub_paid",
                "price_monthly",
                CommercialSubscriptionStatus.PastDue,
                seatQuantity: 1,
                cancelAtPeriodEnd: false,
                SignupTime.AddDays(1),
                SignupTime.AddDays(32),
                SignupTime.AddDays(1)));
        context.CommercialSubscriptions.Add(subscription);
        await context.SaveChangesAsync();

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var entitlement = (await entitlementTest.Service
                .ListForUserAsync(signup.UserId))
            .Single();

        Assert.Multiple(() =>
        {
            Assert.That(subscription.Id.Version, Is.EqualTo(7));
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Grace));
            Assert.That(
                entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionPastDue));
            Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(32)));
        });
    }

    [Test]
    public async Task TransferredTrialFollowsBillingAccountAcrossAllAssignedSeats()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        OrganizationCreationResult trialOrganization;
        OrganizationCreationResult unlicensedOrganization;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1)))
        {
            var organizationService = organizationTest.Service;
            trialOrganization = await organizationService.CreateAsync(
                signup.UserId,
                "Trial Organization");
            unlicensedOrganization = await organizationService.CreateAsync(
                signup.UserId,
                "Unlicensed Organization");
        }

        await using var context = database.CreateContext();
        var persistedTrial = await context.Trials.AsNoTracking().SingleAsync();
        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var entitlements = await entitlementTest.Service
            .ListForUserAsync(signup.UserId);

        Assert.That(entitlements, Has.Count.EqualTo(3));
        var personal = entitlements.Single(
            entitlement => entitlement.BillingAccountId == signup.PersonalBillingAccountId);
        var transferred = entitlements.Single(
            entitlement => entitlement.BillingAccountId == persistedTrial.BillingAccountId);
        var unlicensed = entitlements.Single(
            entitlement => entitlement.BillingAccountId != signup.PersonalBillingAccountId
                && entitlement.BillingAccountId != persistedTrial.BillingAccountId);
        var expectedSeatOrder = new[]
        {
            trialOrganization.SeatId,
            unlicensedOrganization.SeatId,
        }.Order().Prepend(signup.SeatId);
        Assert.Multiple(() =>
        {
            Assert.That(entitlements.Select(entitlement => entitlement.SeatId), Is.EqualTo(expectedSeatOrder));
            Assert.That(personal.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(personal.ReasonCode, Is.EqualTo(EntitlementReasonCodes.NoValidEntitlement));
            Assert.That(transferred.State, Is.EqualTo(EntitlementState.Trial));
            Assert.That(transferred.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(transferred.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(transferred.ValidUntil, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(unlicensed.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(unlicensed.ReasonCode, Is.EqualTo(EntitlementReasonCodes.NoValidEntitlement));
        });
    }

    [Test]
    public async Task OrganizationTrialLicensesMembersWithoutLeakingSeatsBetweenUsers()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "trial-owner", "owner@example.com");
        var member = await SignUpAsync(
            database,
            "trial-member",
            "member@example.com",
            SignupTime.AddDays(-31));
        OrganizationCreationResult organization;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1)))
        {
            organization = await organizationTest.Service
                .CreateAsync(owner.UserId, "Shared Trial Organization");
        }

        var memberSeat = await LicensingPersistenceScenario.AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(1));
        var memberSeatId = memberSeat.SeatId;

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = entitlementTest.Service;
        var memberEntitlements = await service.ListForUserAsync(member.UserId);
        var ownerEntitlements = await service.ListForUserAsync(owner.UserId);
        var memberPersonalEntitlement = memberEntitlements.Single(
            entitlement => entitlement.SeatId == member.SeatId);
        var memberOrganizationEntitlement = memberEntitlements.Single(
            entitlement => entitlement.SeatId == memberSeatId);

        Assert.Multiple(() =>
        {
            Assert.That(
                memberEntitlements.Select(entitlement => entitlement.SeatId),
                Is.EquivalentTo(new[] { member.SeatId, memberSeatId }));
            Assert.That(memberPersonalEntitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                memberPersonalEntitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.TrialExpired));
            Assert.That(memberOrganizationEntitlement.State, Is.EqualTo(EntitlementState.Trial));
            Assert.That(memberOrganizationEntitlement.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(
                memberOrganizationEntitlement.ValidUntil,
                Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(
                memberEntitlements.Select(entitlement => entitlement.SeatId),
                Does.Not.Contain(organization.SeatId));
            Assert.That(
                ownerEntitlements.Select(entitlement => entitlement.SeatId),
                Is.EquivalentTo(new[] { owner.SeatId, organization.SeatId }));
            Assert.That(
                ownerEntitlements.Select(entitlement => entitlement.SeatId),
                Does.Not.Contain(memberSeatId));
        });
    }

    [Test]
    public async Task OrganizationSubscriptionLicensesOnlyItsAssignedUsersInSeatOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-owner", "owner@example.com");
        var member = await SignUpAsync(
            database,
            "paid-member",
            "member@example.com",
            SignupTime.AddDays(-31));
        var unrelated = await SignUpAsync(
            database,
            "paid-unrelated",
            "unrelated@example.com",
            SignupTime.AddDays(-31));
        OrganizationCreationResult organization;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1)))
        {
            organization = await organizationTest.Service
                .CreateAsync(owner.UserId, "Shared Paid Organization");
        }

        var memberSeat = await LicensingPersistenceScenario.AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        var memberSeatId = memberSeat.SeatId;
        await using (var setupContext = database.CreateContext())
        {
            setupContext.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    organization.BillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_paid_organization",
                        "sub_paid_organization",
                        "price_paid_organization",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 2,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(30),
                        SignupTime.AddDays(60),
                        SignupTime.AddDays(30))));
            await setupContext.SaveChangesAsync();
        }

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var service = entitlementTest.Service;
        var ownerEntitlements = await service.ListForUserAsync(owner.UserId);
        var memberEntitlements = await service.ListForUserAsync(member.UserId);
        var unrelatedEntitlements = await service.ListForUserAsync(unrelated.UserId);

        Assert.Multiple(() =>
        {
            Assert.That(
                ownerEntitlements.Select(entitlement => entitlement.SeatId),
                Is.EqualTo(new[] { owner.SeatId, organization.SeatId }));
            Assert.That(
                memberEntitlements.Select(entitlement => entitlement.SeatId),
                Is.EqualTo(new[] { member.SeatId, memberSeatId }));
            Assert.That(
                unrelatedEntitlements.Select(entitlement => entitlement.SeatId),
                Is.EqualTo(new[] { unrelated.SeatId }));
            Assert.That(
                ownerEntitlements.Single(entitlement => entitlement.SeatId == organization.SeatId)
                    .State,
                Is.EqualTo(EntitlementState.Commercial));
            Assert.That(
                memberEntitlements.Single(entitlement => entitlement.SeatId == memberSeatId).State,
                Is.EqualTo(EntitlementState.Commercial));
            Assert.That(
                ownerEntitlements.Select(entitlement => entitlement.SeatId),
                Does.Not.Contain(memberSeatId));
            Assert.That(
                memberEntitlements.Select(entitlement => entitlement.SeatId),
                Does.Not.Contain(organization.SeatId));
            Assert.That(
                unrelatedEntitlements.Select(entitlement => entitlement.BillingAccountId),
                Does.Not.Contain(organization.BillingAccountId));
        });
    }

    [TestCase(0)]
    [TestCase(-1)]
    public async Task DatabaseRejectsInvalidTrialWindows(int daysFromStart)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await SignUpAsync(database);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await context.Trials.ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    trial => trial.EndsAt,
                    SignupTime.AddDays(daysFromStart))));

        Assert.That(exception!.SqlState, Is.EqualTo(Npgsql.PostgresErrorCodes.CheckViolation));
        Assert.That(exception.ConstraintName, Is.EqualTo("ck_trials_valid_window"));
    }

    [TestCase(InvalidCommercialSubscriptionMutation.NonPositiveSeatQuantity)]
    [TestCase(InvalidCommercialSubscriptionMutation.InvalidPeriod)]
    [TestCase(InvalidCommercialSubscriptionMutation.UnknownStatus)]
    public async Task DatabaseRejectsInvalidCommercialSubscriptionProjection(
        InvalidCommercialSubscriptionMutation mutation)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(
            CommercialSubscription.Create(
                signup.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_constraint",
                    "sub_constraint",
                    "price_constraint",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime)));
        await context.SaveChangesAsync();

        Func<Task> invalidUpdate = mutation switch
        {
            InvalidCommercialSubscriptionMutation.NonPositiveSeatQuantity => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.SeatQuantity,
                        0)),
            InvalidCommercialSubscriptionMutation.InvalidPeriod => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.CurrentPeriodEndsAt,
                        subscription => subscription.CurrentPeriodStartedAt)),
            InvalidCommercialSubscriptionMutation.UnknownStatus => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.Status,
                        (CommercialSubscriptionStatus)int.MaxValue)),
            _ => throw new ArgumentOutOfRangeException(nameof(mutation)),
        };

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(invalidUpdate);

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo(
                mutation switch
                {
                    InvalidCommercialSubscriptionMutation.NonPositiveSeatQuantity =>
                        DatabaseConstraintNames.CommercialSubscriptionSeatQuantity,
                    InvalidCommercialSubscriptionMutation.InvalidPeriod =>
                        DatabaseConstraintNames.CommercialSubscriptionValidPeriod,
                    InvalidCommercialSubscriptionMutation.UnknownStatus =>
                        DatabaseConstraintNames.CommercialSubscriptionStatus,
                    _ => throw new ArgumentOutOfRangeException(nameof(mutation)),
                }));
    }

    [TestCase(CommercialSubscriptionExternalField.Customer)]
    [TestCase(CommercialSubscriptionExternalField.Subscription)]
    [TestCase(CommercialSubscriptionExternalField.Price)]
    public async Task DatabaseRequiresCommercialExternalIdentifiers(
        CommercialSubscriptionExternalField field)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(CreateSubscription(signup.PersonalBillingAccountId));
        await context.SaveChangesAsync();

        Func<Task> clearIdentifier = field switch
        {
            CommercialSubscriptionExternalField.Customer => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalCustomerId,
                        (string)null!)),
            CommercialSubscriptionExternalField.Subscription => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalSubscriptionId,
                        (string)null!)),
            CommercialSubscriptionExternalField.Price => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalPriceId,
                        (string)null!)),
            _ => throw new ArgumentOutOfRangeException(nameof(field)),
        };

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(clearIdentifier);

        Assert.That(exception!.SqlState, Is.EqualTo(Npgsql.PostgresErrorCodes.NotNullViolation));
    }

    [TestCase(CommercialSubscriptionExternalField.Customer)]
    [TestCase(CommercialSubscriptionExternalField.Subscription)]
    [TestCase(CommercialSubscriptionExternalField.Price)]
    public async Task DatabaseEnforcesCommercialExternalIdentifierLength(
        CommercialSubscriptionExternalField field)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(CreateSubscription(signup.PersonalBillingAccountId));
        await context.SaveChangesAsync();
        var oversized = new string(
            'x',
            CommercialSubscription.MaximumExternalIdentifierLength + 1);
        Func<Task> update = field switch
        {
            CommercialSubscriptionExternalField.Customer => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalCustomerId,
                        oversized)),
            CommercialSubscriptionExternalField.Subscription => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalSubscriptionId,
                        oversized)),
            CommercialSubscriptionExternalField.Price => async () =>
                await context.CommercialSubscriptions.ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        subscription => subscription.ExternalPriceId,
                        oversized)),
            _ => throw new ArgumentOutOfRangeException(nameof(field)),
        };

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await update());

        Assert.That(
            exception!.SqlState,
            Is.EqualTo(Npgsql.PostgresErrorCodes.StringDataRightTruncation));
    }

    [Test]
    public async Task DatabaseRequiresCommercialSubscriptionBillingAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(CreateSubscription(signup.PersonalBillingAccountId));
        await context.SaveChangesAsync();
        var unknownBillingAccountId = Guid.CreateVersion7();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await context.CommercialSubscriptions.ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    subscription => subscription.BillingAccountId,
                    unknownBillingAccountId)));

        Assert.Multiple(() =>
        {
            Assert.That(exception!.SqlState, Is.EqualTo(Npgsql.PostgresErrorCodes.ForeignKeyViolation));
            Assert.That(
                exception.ConstraintName,
                Is.EqualTo("fk_commercial_subscriptions_billing_account_id"));
        });
    }

    [Test]
    public async Task DatabaseAllowsOnlyOneCurrentSubscriptionPerBillingAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.AddRange(
            CommercialSubscription.Create(
                signup.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_first",
                    "sub_first",
                    "price_first",
                    CommercialSubscriptionStatus.Active,
                    1,
                    false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime)),
            CommercialSubscription.Create(
                signup.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_second",
                    "sub_second",
                    "price_second",
                    CommercialSubscriptionStatus.Active,
                    1,
                    false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime)));

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.That(
            ((Npgsql.PostgresException)exception!.InnerException!).ConstraintName,
            Is.EqualTo("ux_commercial_subscriptions_billing_account_id"));
    }

    [TestCase(
        CommercialSubscriptionIdentityCollision.ExternalCustomer,
        "ux_commercial_subscriptions_external_customer_id")]
    [TestCase(
        CommercialSubscriptionIdentityCollision.ExternalSubscription,
        "ux_commercial_subscriptions_external_subscription_id")]
    public async Task DatabaseRejectsExternalSubscriptionIdentityReuse(
        CommercialSubscriptionIdentityCollision collision,
        string expectedConstraint)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var first = await SignUpAsync(database, "identity-first", "first@example.com");
        var second = await SignUpAsync(database, "identity-second", "second@example.com");
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.AddRange(
            CommercialSubscription.Create(
                first.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_shared",
                    "sub_shared",
                    "price_first",
                    CommercialSubscriptionStatus.Active,
                    1,
                    false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime)),
            CommercialSubscription.Create(
                second.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    collision == CommercialSubscriptionIdentityCollision.ExternalCustomer
                        ? "cus_shared"
                        : "cus_second",
                    collision == CommercialSubscriptionIdentityCollision.ExternalSubscription
                        ? "sub_shared"
                        : "sub_second",
                    "price_second",
                    CommercialSubscriptionStatus.Active,
                    1,
                    false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime)));

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.That(
            ((Npgsql.PostgresException)exception!.InnerException!).ConstraintName,
            Is.EqualTo(expectedConstraint));
    }

    [Test]
    public async Task NewerSubscriptionProjectionPersistsWithoutChangingOwnedIdentity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();
        var subscription = CreateSubscription(signup.PersonalBillingAccountId);
        context.CommercialSubscriptions.Add(subscription);
        await context.SaveChangesAsync();
        var id = subscription.Id;
        var initialVersion = subscription.Version;

        Assert.That(
            subscription.TryApplyProjection(
                new CommercialSubscriptionProjection(
                    "cus_updated",
                    subscription.ExternalSubscriptionId,
                    "price_annual",
                    CommercialSubscriptionStatus.PastDue,
                    3,
                    true,
                    SignupTime.AddMonths(1),
                    SignupTime.AddMonths(2),
                    SignupTime.AddMinutes(1))),
            Is.True);
        await context.SaveChangesAsync();

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.Id, Is.EqualTo(id));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo(subscription.ExternalSubscriptionId));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(persisted.SeatQuantity, Is.EqualTo(3));
            Assert.That(persisted.CancelAtPeriodEnd, Is.True);
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(1)));
            Assert.That(persisted.Version, Is.GreaterThan(initialVersion));
        });
    }

    [Test]
    public async Task ConcurrentSubscriptionProjectionUpdatesCannotLoseNewerSnapshot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using (var setupContext = database.CreateContext())
        {
            setupContext.CommercialSubscriptions.Add(
                CreateSubscription(signup.PersonalBillingAccountId));
            await setupContext.SaveChangesAsync();
        }
        await using var olderContext = database.CreateContext();
        await using var newerContext = database.CreateContext();
        var older = await olderContext.CommercialSubscriptions.SingleAsync();
        var newer = await newerContext.CommercialSubscriptions.SingleAsync();
        Assert.That(
            older.TryApplyProjection(
                new CommercialSubscriptionProjection(
                    "cus_older",
                    older.ExternalSubscriptionId,
                    "price_older",
                    CommercialSubscriptionStatus.Active,
                    2,
                    false,
                    SignupTime.AddMonths(1),
                    SignupTime.AddMonths(2),
                    SignupTime.AddMinutes(1))),
            Is.True);
        Assert.That(
            newer.TryApplyProjection(
                new CommercialSubscriptionProjection(
                    "cus_newer",
                    newer.ExternalSubscriptionId,
                    "price_newer",
                    CommercialSubscriptionStatus.PastDue,
                    4,
                    true,
                    SignupTime.AddMonths(1),
                    SignupTime.AddMonths(2),
                    SignupTime.AddMinutes(2))),
            Is.True);

        await newerContext.SaveChangesAsync();
        Assert.ThrowsAsync<DbUpdateConcurrencyException>(
            async () => await olderContext.SaveChangesAsync());

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo(newer.ExternalSubscriptionId));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(persisted.SeatQuantity, Is.EqualTo(4));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(2)));
        });
    }

    [Test]
    public async Task UnknownUserCannotResolveEntitlements()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await entitlementTest.Service
                .ListForUserAsync(Guid.CreateVersion7()));
    }

    [Test]
    public async Task MissingUserCannotReadEntitlements()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await entitlementTest.Service
                .ListForUserAsync(Guid.Empty));
    }

    private static async Task<SignupResult> SignUpAsync(
        PostgresTestDatabase database,
        string subject = "entitlement-user",
        string email = "person@example.com",
        DateTimeOffset? observedAt = null)
    {
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, observedAt ?? SignupTime);
        var service = signupTest.Service;
        return await service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", subject, email));
    }

    private static CommercialSubscription CreateSubscription(Guid billingAccountId)
    {
        return CommercialSubscription.Create(
            billingAccountId,
            new CommercialSubscriptionProjection(
                "cus_projection",
                "sub_projection",
                "price_projection",
                CommercialSubscriptionStatus.Active,
                1,
                false,
                SignupTime,
                SignupTime.AddMonths(1),
                SignupTime));
    }

    public enum InvalidCommercialSubscriptionMutation
    {
        NonPositiveSeatQuantity,
        InvalidPeriod,
        UnknownStatus,
    }

    public enum CommercialSubscriptionIdentityCollision
    {
        ExternalCustomer,
        ExternalSubscription,
    }

    public enum CommercialSubscriptionExternalField
    {
        Customer,
        Subscription,
        Price,
    }
}
