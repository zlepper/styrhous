using Microsoft.AspNetCore.DataProtection.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Domain.Trials;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class SignupPersistenceTests
{
    private static readonly DateTimeOffset SignupTime =
        new(2026, 8, 30, 10, 15, 0, TimeSpan.Zero);

    [Test]
    public async Task FirstSignupPersistsThePersonalAccountSeatAndImmediateTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var service = signupTest.Service;
        var identity = VerifiedExternalIdentity.Create("github", "new-user", "person@example.com");

        var result = await service.SignUpAsync(identity);

        Assert.Multiple(() =>
        {
            Assert.That(result.WasCreated, Is.True);
            Assert.That(result.TrialStartedAt, Is.EqualTo(SignupTime));
            Assert.That(result.TrialEndsAt, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(
                new[] { result.UserId, result.PersonalBillingAccountId, result.SeatId, result.TrialId },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });

        await using var verificationContext = database.CreateContext();
        var user = await verificationContext.UserAccounts.SingleAsync();
        var externalIdentity = await verificationContext.ExternalIdentities.SingleAsync();
        var verifiedEmailClaim = await verificationContext.VerifiedEmailClaims.SingleAsync();
        var billingAccount = await verificationContext.BillingAccounts.SingleAsync();
        var seat = await verificationContext.Seats.SingleAsync();
        var trial = await verificationContext.Trials.SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(user.Id, Is.EqualTo(result.UserId));
            Assert.That(user.VerifiedEmail, Is.EqualTo("person@example.com"));
            Assert.That(externalIdentity.UserId, Is.EqualTo(user.Id));
            Assert.That(externalIdentity.Id.Version, Is.EqualTo(7));
            Assert.That(externalIdentity.Provider, Is.EqualTo("github"));
            Assert.That(externalIdentity.Subject, Is.EqualTo("new-user"));
            Assert.That(verifiedEmailClaim.Id.Version, Is.EqualTo(7));
            Assert.That(verifiedEmailClaim.UserId, Is.EqualTo(user.Id));
            Assert.That(verifiedEmailClaim.NormalizedEmail, Is.EqualTo("PERSON@EXAMPLE.COM"));
            Assert.That(billingAccount.PersonalOwnerUserId, Is.EqualTo(user.Id));
            Assert.That(billingAccount.Kind, Is.EqualTo(BillingAccountKind.Personal));
            Assert.That(seat.AssignedUserId, Is.EqualTo(user.Id));
            Assert.That(seat.BillingAccountId, Is.EqualTo(billingAccount.Id));
            Assert.That(trial.OriginatingUserId, Is.EqualTo(user.Id));
            Assert.That(trial.BillingAccountId, Is.EqualTo(billingAccount.Id));
            Assert.That(trial.StartedAt, Is.EqualTo(SignupTime));
            Assert.That(trial.EndsAt, Is.EqualTo(SignupTime.AddDays(30)));
        });
    }

    [Test]
    public async Task EveryApplicationOwnedEntityUsesUuid7PrimaryKeyGeneration()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        foreach (var entityType in context.Model.GetEntityTypes())
        {
            if (entityType.ClrType == typeof(RebusOutboxMessage)
                || entityType.ClrType == typeof(DataProtectionKey))
            {
                continue;
            }

            var primaryKey = entityType.FindPrimaryKey();
            Assert.That(primaryKey, Is.Not.Null, $"{entityType.Name} has no primary key.");
            Assert.That(
                primaryKey!.Properties,
                Has.Count.EqualTo(1),
                $"{entityType.Name} must have one UUIDv7 primary key.");

            var property = primaryKey.Properties[0];
            Assert.That(property.ClrType, Is.EqualTo(typeof(Guid)), entityType.Name);
            var generatorFactory = property.GetValueGeneratorFactory();
            Assert.That(generatorFactory, Is.Not.Null, entityType.Name);
            Assert.That(
                generatorFactory!(property, entityType),
                Is.TypeOf<Uuid7ValueGenerator>(),
                entityType.Name);
        }
    }

    [Test]
    public async Task RepeatingTheSameExternalSignupReturnsTheExistingTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var service = signupTest.Service;
        var identity = VerifiedExternalIdentity.Create("google", "stable-subject", "person@example.com");

        var first = await service.SignUpAsync(identity);
        var second = await service.SignUpAsync(identity);

        Assert.Multiple(() =>
        {
            Assert.That(first.WasCreated, Is.True);
            Assert.That(second.WasCreated, Is.False);
            Assert.That(second.UserId, Is.EqualTo(first.UserId));
            Assert.That(second.PersonalBillingAccountId, Is.EqualTo(first.PersonalBillingAccountId));
            Assert.That(second.SeatId, Is.EqualTo(first.SeatId));
            Assert.That(second.TrialId, Is.EqualTo(first.TrialId));
            Assert.That(second.TrialStartedAt, Is.EqualTo(first.TrialStartedAt));
            Assert.That(second.TrialEndsAt, Is.EqualTo(first.TrialEndsAt));
        });

        await AssertSingleRegistrationAsync(database);
    }

    [Test]
    public async Task RepeatingAStableExternalIdentityWithAChangedEmailReturnsTheExistingTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        SignupResult first;
        await using (var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime))
        {
            first = await signupTest.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("github", "stable-subject", "old@example.com"));
        }

        await using var secondContext = database.CreateContext();
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var second = await signupTest2.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "stable-subject", "new@example.com"));

        Assert.Multiple(() =>
        {
            Assert.That(second.WasCreated, Is.False);
            Assert.That(second.UserId, Is.EqualTo(first.UserId));
            Assert.That(second.TrialId, Is.EqualTo(first.TrialId));
        });

        var user = await secondContext.UserAccounts.SingleAsync();
        var claimedEmails = await secondContext.VerifiedEmailClaims
            .OrderBy(claim => claim.NormalizedEmail)
            .Select(claim => claim.NormalizedEmail)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(user.VerifiedEmail, Is.EqualTo("new@example.com"));
            Assert.That(user.NormalizedEmail, Is.EqualTo("NEW@EXAMPLE.COM"));
            Assert.That(claimedEmails, Has.Length.EqualTo(2));
            Assert.That(claimedEmails[0], Is.EqualTo("NEW@EXAMPLE.COM"));
            Assert.That(claimedEmails[1], Is.EqualTo("OLD@EXAMPLE.COM"));
        });
        await AssertSingleRegistrationAsync(database, verifiedEmailClaimCount: 2);

        await using var signupTest3 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<AccountLinkRequiredException>(
            async () => await signupTest3.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("google", "old-email", "old@example.com")));
        await using var signupTest4 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<AccountLinkRequiredException>(
            async () => await signupTest4.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("google", "new-email", "new@example.com")));
        await AssertSingleRegistrationAsync(database, verifiedEmailClaimCount: 2);
    }

    [Test]
    public async Task ConcurrentCallbacksSynchronizeTheSameNewVerifiedEmailOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using (var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime))
        {
            await signupTest.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("github", "stable-subject", "old@example.com"));
        }

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var changedIdentity = VerifiedExternalIdentity.Create(
            "github",
            "stable-subject",
            "new@example.com");

        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(
                barrier,
                SignupBarrierQuery.EmailClaimOwner)]);
        await using var signupTest3 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(
                barrier,
                SignupBarrierQuery.EmailClaimOwner)]);
        var results = await Task.WhenAll(
            signupTest2.Service.SignUpAsync(changedIdentity),
            signupTest3.Service.SignUpAsync(changedIdentity));

        Assert.Multiple(() =>
        {
            Assert.That(results, Has.All.Property(nameof(SignupResult.WasCreated)).False);
            Assert.That(results.Select(result => result.UserId).Distinct().Count(), Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.VerifiedEmailClaims.CountAsync(),
            Is.EqualTo(2));
        Assert.That(
            (await verificationContext.UserAccounts.SingleAsync()).VerifiedEmail,
            Is.EqualTo("new@example.com"));
    }

    [Test]
    public async Task EmailSynchronizationRetriesAfterAConcurrentUserClaim()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        SignupResult signup;
        await using (var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime))
        {
            signup = await signupTest.Service.SignUpAsync(
                VerifiedExternalIdentity.Create(
                    "github",
                    "claim-race-subject",
                    "old@example.com"));
        }

        var gate = new DatabaseCommandGate();
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new DatabaseCommandGateInterceptor(gate, "FROM verified_email_claims")]);
        var synchronizationTask = signupTest2.Service.SignUpAsync(
            VerifiedExternalIdentity.Create(
                "github",
                "claim-race-subject",
                "new@example.com"));
        await gate.WaitUntilReachedAsync();
        OrganizationCreationResult organization;
        try
        {
            await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1));
            organization = await organizationTest.Service
                .CreateAsync(signup.UserId, "Concurrent Claim Organization");
        }
        finally
        {
            gate.Release();
        }

        var synchronized = await synchronizationTask;

        await using var verificationContext = database.CreateContext();
        var persistedUser = await verificationContext.UserAccounts.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organization.OrganizationId.Version, Is.EqualTo(7));
            Assert.That(synchronized.WasCreated, Is.False);
            Assert.That(persistedUser.VerifiedEmail, Is.EqualTo("new@example.com"));
            Assert.That(persistedUser.ConcurrencyVersion, Is.EqualTo(1));
            Assert.That(
                verificationContext.VerifiedEmailClaims.Count(),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ConcurrentSignupForTheSameExternalIdentityCreatesExactlyOneTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(barrier)]);
        var firstService = signupTest.Service;
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(barrier)]);
        var secondService = signupTest2.Service;

        var results = await Task.WhenAll(
            firstService.SignUpAsync(
                VerifiedExternalIdentity.Create(
                    "microsoft",
                    "concurrent-user",
                    "first@example.com")),
            secondService.SignUpAsync(
                VerifiedExternalIdentity.Create(
                    "microsoft",
                    "concurrent-user",
                    "second@example.com")));

        Assert.Multiple(() =>
        {
            Assert.That(results.Count(result => result.WasCreated), Is.EqualTo(1));
            Assert.That(results.Select(result => result.UserId).Distinct().Count(), Is.EqualTo(1));
            Assert.That(results.Select(result => result.TrialId).Distinct().Count(), Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });

        await AssertSingleRegistrationAsync(database, verifiedEmailClaimCount: 2);
    }

    [Test]
    public async Task ConcurrentSignupForTheSameVerifiedEmailExercisesEmailConflictRecovery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(barrier)]);
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SignupPreflightBarrierInterceptor(barrier)]);
        var results = await Task.WhenAll(
            SignUpOrRequireLinkAsync(
                signupTest.Service,
                VerifiedExternalIdentity.Create("github", "github-subject", "Person@Example.com")),
            SignUpOrRequireLinkAsync(
                signupTest2.Service,
                VerifiedExternalIdentity.Create("google", "google-subject", "person@example.com")));

        Assert.Multiple(() =>
        {
            Assert.That(results.Count(wasCreated => wasCreated), Is.EqualTo(1));
            Assert.That(results.Count(wasCreated => !wasCreated), Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
        await AssertSingleRegistrationAsync(database);
    }

    [Test]
    public async Task ExistingVerifiedEmailThroughAnotherProviderRequiresExplicitLinking()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var service = signupTest.Service;
        await service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "github-subject", "Person@Example.com"));

        Assert.ThrowsAsync<AccountLinkRequiredException>(
            async () => await service.SignUpAsync(
                VerifiedExternalIdentity.Create("google", "google-subject", "person@example.com")));

        await AssertSingleRegistrationAsync(database);
    }

    [Test]
    public async Task ChangedVerifiedEmailAlreadyClaimedByAnotherUserRequiresExplicitLinking()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var service = signupTest.Service;
        await service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "first-subject", "first@example.com"));
        await service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "second-subject", "second@example.com"));

        Assert.ThrowsAsync<AccountLinkRequiredException>(
            async () => await service.SignUpAsync(
                VerifiedExternalIdentity.Create(
                    "github",
                    "first-subject",
                    "second@example.com")));

        var firstUser = await context.UserAccounts.SingleAsync(
            user => user.NormalizedEmail == "FIRST@EXAMPLE.COM");
        Assert.That(firstUser.VerifiedEmail, Is.EqualTo("first@example.com"));
        Assert.That(await context.VerifiedEmailClaims.CountAsync(), Is.EqualTo(2));
        Assert.That(await context.Trials.CountAsync(), Is.EqualTo(2));
    }

    [Test]
    public async Task DatabaseEmailClaimConstraintRollsBackACompetingRegistration()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var firstContext = database.CreateContext();
        await using var secondContext = database.CreateContext();
        var firstRegistration = SignupRegistration.Start(
            VerifiedExternalIdentity.Create("github", "github-race", "Person@Example.com"),
            SignupTime);
        var secondRegistration = SignupRegistration.Start(
            VerifiedExternalIdentity.Create("google", "google-race", "person@example.com"),
            SignupTime);

        var results = await Task.WhenAll(
            AddOrRequireLinkAsync(new PostgresUserSignupStore(firstContext, firstContext.CreateContextFactory()), firstRegistration),
            AddOrRequireLinkAsync(new PostgresUserSignupStore(secondContext, secondContext.CreateContextFactory()), secondRegistration));

        Assert.Multiple(() =>
        {
            Assert.That(results.Count(wasAdded => wasAdded), Is.EqualTo(1));
            Assert.That(results.Count(wasAdded => !wasAdded), Is.EqualTo(1));
        });
        await AssertSingleRegistrationAsync(database);
    }

    [Test]
    public async Task OrganizationAccountsDoNotConsumeThePersonalAccountOwnershipSlot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var signup = await signupTest.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "organization-owner", "person@example.com"));

        context.BillingAccounts.AddRange(
            BillingAccount.CreateOrganization(SignupTime),
            BillingAccount.CreateOrganization(SignupTime));
        await context.SaveChangesAsync();
        var accountCount = await context.BillingAccounts.CountAsync();

        context.BillingAccounts.Add(BillingAccount.CreatePersonal(signup.UserId, SignupTime));
        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.Multiple(() =>
        {
            Assert.That(accountCount, Is.EqualTo(3));
            Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
            Assert.That(
                ((PostgresException)exception.InnerException!).ConstraintName,
                Is.EqualTo("ux_billing_accounts_personal_owner_user_id"));
        });
    }

    [Test]
    public async Task RepeatingSignupAfterTrialTransferReturnsThePersonalAccountAndTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        SignupResult first;
        Guid organizationId;
        await using (var context = database.CreateContext())
        {
            await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime);
            var service = signupTest.Service;
            first = await service.SignUpAsync(
                VerifiedExternalIdentity.Create("github", "transfer-user", "person@example.com"));

            var organization = BillingAccount.CreateOrganization(SignupTime);
            organizationId = organization.Id;
            context.BillingAccounts.Add(organization);
            await context.SaveChangesAsync();
            var updatedTrialCount = await TransferTrialForTestAsync(
                context,
                first.TrialId,
                organization.Id,
                SignupTime.AddDays(1));
            Assert.That(updatedTrialCount, Is.EqualTo(1));
        }

        await using var repeatContext = database.CreateContext();
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var repeated = await signupTest2.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "transfer-user", "changed@example.com"));
        var transferredTrial = await repeatContext.Trials.SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(repeated.WasCreated, Is.False);
            Assert.That(repeated.UserId, Is.EqualTo(first.UserId));
            Assert.That(
                repeated.PersonalBillingAccountId,
                Is.EqualTo(first.PersonalBillingAccountId));
            Assert.That(repeated.TrialId, Is.EqualTo(first.TrialId));
            Assert.That(repeated.TrialStartedAt, Is.EqualTo(first.TrialStartedAt));
            Assert.That(repeated.TrialEndsAt, Is.EqualTo(first.TrialEndsAt));
            Assert.That(transferredTrial.BillingAccountId, Is.EqualTo(organizationId));
            Assert.That(transferredTrial.TransferredAt, Is.EqualTo(SignupTime.AddDays(1)));
        });
    }

    [Test]
    public async Task DatabaseRejectsASecondOriginatedTrialForTheSameUser()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();
        var identity = VerifiedExternalIdentity.Create("github", "one-trial-user", "person@example.com");
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var signup = await signupTest.Service.SignUpAsync(identity);

        var organization = BillingAccount.CreateOrganization(SignupTime.AddDays(1));
        context.BillingAccounts.Add(organization);
        context.Trials.Add(
            Trial.Start(
                signup.UserId,
                organization.Id,
                SignupTime.AddDays(1)));

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_trials_originating_user_id"));
    }

    [Test]
    public async Task DatabaseRejectsTwoTrialsOnTheSameBillingAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var first = await signupTest.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "first-user", "first@example.com"));
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime);
        var second = await signupTest2.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "second-user", "second@example.com"));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await TransferTrialForTestAsync(
                context,
                second.TrialId,
                first.PersonalBillingAccountId,
                SignupTime.AddDays(1)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ux_trials_billing_account_id"));
    }

    [Test]
    public async Task EmailSynchronizationDisposesTheFailedAttemptBeforeRetrying()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using (var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime))
        {
            await signupTest.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("github", "fresh-context", "before@example.com"));
        }

        var failures = new ConcurrencyFailureInterceptor(attempt => attempt == 1);
        await using var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [failures]);
        await signupTest2.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", "fresh-context", "after@example.com"));

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(failures.AttemptCount, Is.EqualTo(2));
            Assert.That(failures.ContextCount, Is.EqualTo(2));
            Assert.That(verification.UserAccounts.Single().VerifiedEmail, Is.EqualTo("after@example.com"));
            Assert.That(verification.VerifiedEmailClaims.Count(), Is.EqualTo(2));
        });
    }

    [Test]
    public async Task EmailSynchronizationLeavesRetryAndRollbackToTheOuterTransaction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using (var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(
                database, SignupTime))
        {
            await signupTest.Service.SignUpAsync(
                VerifiedExternalIdentity.Create("github", "outer-transaction", "before@example.com"));
        }

        var failures = new ConcurrencyFailureInterceptor(attempt => attempt == 1);
        await using (var signupTest2 = ServiceTestBase<UserSignupService>.ForDatabase(
            database, SignupTime, interceptors: [failures]))
        {
            var context = signupTest2.Services.GetRequiredService<LicensingDbContext>();
            await context.Database.CreateExecutionStrategy().ExecuteAsync(async () =>
            {
                await using var transaction = await context.Database.BeginTransactionAsync();
                Assert.ThrowsAsync<DbUpdateConcurrencyException>(async () =>
                    await signupTest2.Service.SignUpAsync(
                        VerifiedExternalIdentity.Create(
                            "github",
                            "outer-transaction",
                            "after@example.com")));
                await transaction.RollbackAsync();
            });
        }

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(failures.AttemptCount, Is.EqualTo(1));
            Assert.That(failures.ContextCount, Is.EqualTo(1));
            Assert.That(verification.UserAccounts.Single().VerifiedEmail, Is.EqualTo("before@example.com"));
            Assert.That(verification.VerifiedEmailClaims.Count(), Is.EqualTo(1));
        });
    }

    private static async Task AssertSingleRegistrationAsync(
        PostgresTestDatabase database,
        int verifiedEmailClaimCount = 1)
    {
        await using var context = database.CreateContext();
        var userCount = await context.UserAccounts.CountAsync();
        var externalIdentityCount = await context.ExternalIdentities.CountAsync();
        var actualVerifiedEmailClaimCount = await context.VerifiedEmailClaims.CountAsync();
        var billingAccountCount = await context.BillingAccounts.CountAsync();
        var seatCount = await context.Seats.CountAsync();
        var trialCount = await context.Trials.CountAsync();

        Assert.Multiple(() =>
        {
            Assert.That(userCount, Is.EqualTo(1));
            Assert.That(externalIdentityCount, Is.EqualTo(1));
            Assert.That(actualVerifiedEmailClaimCount, Is.EqualTo(verifiedEmailClaimCount));
            Assert.That(billingAccountCount, Is.EqualTo(1));
            Assert.That(seatCount, Is.EqualTo(1));
            Assert.That(trialCount, Is.EqualTo(1));
        });
    }

    private static async Task<bool> AddOrRequireLinkAsync(
        PostgresUserSignupStore store,
        SignupRegistration registration)
    {
        try
        {
            await store.AddAsync(registration, CancellationToken.None);
            return true;
        }
        catch (AccountLinkRequiredException)
        {
            return false;
        }
    }

    private static async Task<bool> SignUpOrRequireLinkAsync(
        UserSignupService service,
        VerifiedExternalIdentity identity)
    {
        try
        {
            return (await service.SignUpAsync(identity)).WasCreated;
        }
        catch (AccountLinkRequiredException)
        {
            return false;
        }
    }

    private static Task<int> TransferTrialForTestAsync(
        LicensingDbContext context,
        Guid trialId,
        Guid billingAccountId,
        DateTimeOffset transferredAt)
    {
        return context.Trials
            .Where(trial => trial.Id == trialId)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(trial => trial.BillingAccountId, billingAccountId)
                    .SetProperty(trial => trial.TransferredAt, transferredAt));
    }
}
