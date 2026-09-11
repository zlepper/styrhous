using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CommercialSubscriptionProjectionPersistenceTests
{
    [TestCase(CommercialSubscriptionStatus.Active)]
    [TestCase(CommercialSubscriptionStatus.PastDue)]
    public async Task EligiblePaidProjectionTerminatesInternalTrialAtomically(
        CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"projection-trial-{status}",
            $"projection-trial-{status}@example.com");
        var projectedAt = SignupTime.AddDays(3);
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var result = await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(projectedAt, $"trial-{status}", status));
            Assert.That(
                result.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Created));
        }

        await using var verification = database.CreateContext();
        var trial = await verification.Trials.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.StartedAt, Is.EqualTo(SignupTime));
            Assert.That(trial.EndsAt, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(trial.TerminatedAt, Is.EqualTo(projectedAt));
            Assert.That(trial.EffectiveEndsAt, Is.EqualTo(projectedAt));
        });
    }

    [TestCase(CommercialSubscriptionStatus.Unpaid)]
    [TestCase(CommercialSubscriptionStatus.Paused)]
    [TestCase(CommercialSubscriptionStatus.Incomplete)]
    [TestCase(CommercialSubscriptionStatus.IncompleteExpired)]
    [TestCase(CommercialSubscriptionStatus.Trialing)]
    [TestCase(CommercialSubscriptionStatus.Canceled)]
    public async Task IneligibleProjectionDoesNotTerminateInternalTrial(
        CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"projection-trial-ineligible-{status}",
            $"projection-trial-ineligible-{status}@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(SignupTime.AddDays(3), $"trial-ineligible-{status}", status));
        }

        await using var verification = database.CreateContext();
        var trial = await verification.Trials.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.TerminatedAt, Is.Null);
            Assert.That(trial.EffectiveEndsAt, Is.EqualTo(SignupTime.AddDays(30)));
        });
    }

    [TestCase(-1)]
    [TestCase(30)]
    public async Task ProjectionOutsideTheHalfOpenPaidPeriodDoesNotTerminateTrial(
        int projectedMinuteOffset)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"projection-trial-window-{projectedMinuteOffset}",
            $"projection-trial-window-{projectedMinuteOffset}@example.com");
        var projectedAt = projectedMinuteOffset < 0
            ? SignupTime.AddMinutes(projectedMinuteOffset)
            : SignupTime.AddMonths(1);
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(projectedAt, $"trial-window-{projectedMinuteOffset}", CommercialSubscriptionStatus.Active));
        }

        await using var verification = database.CreateContext();
        Assert.That((await verification.Trials.SingleAsync()).TerminatedAt, Is.Null);
    }

    [Test]
    public async Task PaidProjectionAtTrialStartTerminatesTrialImmediately()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "projection-trial-exact-start",
            "projection-trial-exact-start@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime,
                    "trial-exact-start",
                    CommercialSubscriptionStatus.Active));
        }

        await using var verification = database.CreateContext();
        Assert.That(
            (await verification.Trials.SingleAsync()).TerminatedAt,
            Is.EqualTo(SignupTime));
    }

    [Test]
    public async Task LaterInactiveProjectionCannotReviveATerminatedTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "projection-trial-no-revival",
            "projection-trial-no-revival@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var service = projectionTest.Service;
            await service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddDays(3),
                    "trial-paid",
                    CommercialSubscriptionStatus.Active));
            await service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddDays(4),
                    "trial-canceled",
                    CommercialSubscriptionStatus.Canceled));
        }

        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(5));
        var entitlement = (await entitlementTest.Service
            .ListForUserAsync(signup.UserId)).Single();
        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionInactive));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddMonths(1)));
        });
    }

    [Test]
    public async Task StoreCreatesUpdatesAndIgnoresStaleProjectionWithoutChangingIdentity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-owner", "owner@example.com");
        CommercialSubscriptionProjectionResult created;
        CommercialSubscriptionProjectionResult updated;
        CommercialSubscriptionProjectionResult ignored;
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var service = projectionTest.Service;
            created = await service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(SignupTime, "created", CommercialSubscriptionStatus.Active));
            updated = await service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(2),
                    "updated",
                    CommercialSubscriptionStatus.PastDue));
            ignored = await service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(1),
                    "stale",
                    CommercialSubscriptionStatus.Canceled));
        }

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(created.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Created));
            Assert.That(updated.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(ignored.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Ignored));
            Assert.That(created.SubscriptionId, Is.EqualTo(updated.SubscriptionId));
            Assert.That(created.SubscriptionId, Is.EqualTo(ignored.SubscriptionId));
            Assert.That(created.SubscriptionId!.Value.Version, Is.EqualTo(7));
            Assert.That(persisted.Id, Is.EqualTo(created.SubscriptionId));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(2)));
        });
    }

    [Test]
    public async Task ReusedStoreReloadsProjectionChangedByAnotherContext()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-reload", "owner@example.com");
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime);
        var reusedService = projectionTest.Service;
        var created = await reusedService.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(SignupTime, "initial", CommercialSubscriptionStatus.Active));
        await using (var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var external = await projectionTest2.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(2),
                    "external",
                    CommercialSubscriptionStatus.PastDue));
            Assert.That(external.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
        }

        var ignored = await reusedService.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(
                SignupTime.AddMinutes(1),
                "stale",
                CommercialSubscriptionStatus.Canceled));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(ignored.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Ignored));
            Assert.That(ignored.SubscriptionId, Is.EqualTo(created.SubscriptionId));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(2)));
        });
    }

    [Test]
    public async Task ProviderReadRevisionBreaksTiedResponseTimesAcrossContexts()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "projection-provider-revision",
            "owner@example.com");
        long olderReadRevision;
        long newerReadRevision;
        var revisions = new PostgresBillingProviderReadRevisionSource(
            database.CreateContextFactory());
        olderReadRevision = await revisions.ReserveAsync(CancellationToken.None);
        newerReadRevision = await revisions.ReserveAsync(CancellationToken.None);

        CommercialSubscriptionProjectionResult newerResult;
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            newerResult = await projectionTest.Service.ApplyAsync(
                new AuthoritativeCommercialSubscription(
                    signup.PersonalBillingAccountId,
                    CreateProjection(
                        SignupTime,
                        "newer-read",
                        CommercialSubscriptionStatus.Canceled),
                    newerReadRevision));
        }

        CommercialSubscriptionProjectionResult staleResult;
        await using (var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            staleResult = await projectionTest2.Service.ApplyAsync(
                new AuthoritativeCommercialSubscription(
                    signup.PersonalBillingAccountId,
                    CreateProjection(
                        SignupTime,
                        "older-read-same-clock",
                        CommercialSubscriptionStatus.Active),
                    olderReadRevision));
        }

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(newerReadRevision, Is.GreaterThan(olderReadRevision));
            Assert.That(
                newerResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Created));
            Assert.That(
                staleResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Ignored));
            Assert.That(persisted.ProviderReadRevision, Is.EqualTo(newerReadRevision));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.Canceled));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime));
        });
    }

    [Test]
    public async Task MutationResponseWinsTiedProviderTimeAgainstLaterObservationRevision()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "projection-mutation-response-priority",
            "owner@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var result = await projectionTest.Service.ApplyAsync(
                new AuthoritativeCommercialSubscription(
                    signup.PersonalBillingAccountId,
                    CreateProjection(
                        SignupTime,
                        "mutation-response",
                        CommercialSubscriptionStatus.Active),
                    ProviderReadRevision: 1,
                    ProviderSnapshotKind:
                        CommercialSubscriptionSnapshotKind.MutationResponse));
            Assert.That(
                result.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Created));
        }

        await using (var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            var result = await projectionTest2.Service.ApplyAsync(
                new AuthoritativeCommercialSubscription(
                    signup.PersonalBillingAccountId,
                    CreateProjection(
                        SignupTime,
                        "delayed-observation",
                        CommercialSubscriptionStatus.Canceled),
                    ProviderReadRevision: 2,
                    ProviderSnapshotKind: CommercialSubscriptionSnapshotKind.Observation));
            Assert.That(
                result.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.CausalConflict));
        }

        await using var verificationContext = database.CreateContext();
        var persisted = verificationContext.CommercialSubscriptions.Single();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.Active));
            Assert.That(persisted.ProviderReadRevision, Is.EqualTo(1));
            Assert.That(
                persisted.ProviderSnapshotKind,
                Is.EqualTo(CommercialSubscriptionSnapshotKind.MutationResponse));
        });
    }

    [Test]
    public async Task ConcurrentProviderReadRevisionReservationsAreUniqueAndDurable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var revisions = await Task.WhenAll(
            Enumerable.Range(0, 16).Select(async _ =>
            {
                return await new PostgresBillingProviderReadRevisionSource(
                        database.CreateContextFactory())
                    .ReserveAsync(CancellationToken.None);
            }));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                revisions,
                Is.EquivalentTo(Enumerable.Range(1, 16).Select(value => (long)value)));
            Assert.That(
                verificationContext.BillingProviderReadCursors.Single().Revision,
                Is.EqualTo(16));
        });
    }

    [Test]
    public async Task StoreSerializesConcurrentUpdatesSoNewerProjectionWinsAfterOlderCommits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-race", "owner@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(SignupTime, "initial", CommercialSubscriptionStatus.Active));
        }
        var olderSaveGate = new DatabaseCommandGate();

        await using var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SavedChangesGateInterceptor(olderSaveGate)]);
        var olderTask = projectionTest2.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(
                SignupTime.AddMinutes(1),
                "older",
                CommercialSubscriptionStatus.Active));
        await olderSaveGate.WaitUntilReachedAsync();
        await using var projectionTest3 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime);
        var newerTask = projectionTest3.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(
                SignupTime.AddMinutes(2),
                "newer",
                CommercialSubscriptionStatus.PastDue));
        olderSaveGate.Release();
        var results = await Task.WhenAll(olderTask, newerTask);

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                results.Select(result => result.Status),
                Is.EqualTo(
                    new[]
                    {
                        CommercialSubscriptionProjectionStatus.Updated,
                        CommercialSubscriptionProjectionStatus.Updated,
                    }));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(2)));
        });
    }

    [Test]
    public async Task ConcurrentFirstProjectionsCreateOneStableCurrentProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-first-race", "owner@example.com");
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts")]);
        await using var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts")]);
        var results = await Task.WhenAll(
            projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(1),
                    "older-first",
                    CommercialSubscriptionStatus.Active)),
            projectionTest2.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(2),
                    "newer-first",
                    CommercialSubscriptionStatus.PastDue)));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result =>
                    result.Status == CommercialSubscriptionProjectionStatus.Created),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result => result.Status
                    is CommercialSubscriptionProjectionStatus.Updated
                    or CommercialSubscriptionProjectionStatus.Ignored),
                Is.EqualTo(1));
            Assert.That(results.Select(result => result.SubscriptionId), Is.All.EqualTo(persisted.Id));
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime.AddMinutes(2)));
        });
    }

    [Test]
    public async Task UnknownBillingAccountDoesNotCreateProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime);
        var result = await projectionTest.Service.ApplyAsync(
            Guid.CreateVersion7(),
            CreateProjection(SignupTime, "unknown", CommercialSubscriptionStatus.Active));

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.BillingAccountNotFound));
            Assert.That(result.SubscriptionId, Is.Null);
            Assert.That(context.CommercialSubscriptions, Is.Empty);
        });
    }

    [Test]
    public async Task FailedProjectionUpdateRollsBackPersistedState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-rollback", "owner@example.com");
        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, SignupTime))
        {
            await projectionTest.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(SignupTime, "initial", CommercialSubscriptionStatus.Active));
        }

        await using var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await projectionTest2.Service.ApplyAsync(
                signup.PersonalBillingAccountId,
                CreateProjection(
                    SignupTime.AddMinutes(1),
                    "failed",
                    CommercialSubscriptionStatus.Canceled)));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.CommercialSubscriptions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(persisted.Status, Is.EqualTo(CommercialSubscriptionStatus.Active));
            Assert.That(persisted.ProjectedAt, Is.EqualTo(SignupTime));
        });
    }

    [Test]
    public async Task CancellationWhileWaitingForBillingSerializationStopsProjectionWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "projection-cancel", "owner@example.com");
        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(gate, "UPDATE billing_accounts");
        using var cancellation = new CancellationTokenSource();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [interceptor]);
        var projectionTask = projectionTest.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(SignupTime, "cancelled", CommercialSubscriptionStatus.Active),
            cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await projectionTask);
            await interceptor.WaitUntilCancellationObservedAsync();
        }
        finally
        {
            gate.Release();
        }

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(verificationContext.CommercialSubscriptions, Is.Empty);
            Assert.That(verificationContext.Trials.Single().TerminatedAt, Is.Null);
        });
    }

    [Test]
    public async Task CancellationAfterInitialSaveRollsBackPersistedState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "projection-post-save-cancel",
            "owner@example.com");
        var gate = new DatabaseCommandGate();
        var interceptor = new SavedChangesGateInterceptor(gate);
        using var cancellation = new CancellationTokenSource();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [interceptor]);
        var projectionTask = projectionTest.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(SignupTime, "cancelled-save", CommercialSubscriptionStatus.Active),
            cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await projectionTask);
        }
        finally
        {
            gate.Release();
        }

        Assert.That(interceptor.CancellationObserved, Is.True);
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(verificationContext.CommercialSubscriptions, Is.Empty);
            Assert.That(verificationContext.Trials.Single().TerminatedAt, Is.Null);
        });
    }

    private static CommercialSubscriptionProjection CreateProjection(
        DateTimeOffset projectedAt,
        string suffix,
        CommercialSubscriptionStatus status)
    {
        return new(
            $"cus_{suffix}",
            "sub_stable",
            $"price_{suffix}",
            status,
            seatQuantity: 5,
            cancelAtPeriodEnd: false,
            SignupTime,
            SignupTime.AddMonths(1),
            projectedAt);
    }
}
