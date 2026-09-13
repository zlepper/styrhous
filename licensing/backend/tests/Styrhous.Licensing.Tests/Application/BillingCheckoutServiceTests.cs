using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCheckoutServiceTests
{
    private static readonly DateTimeOffset CheckoutTime = SignupTime.AddDays(3);

    [Test]
    public async Task SuccessfulProviderSessionCompletesDurableOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service",
            "checkout-service@example.com");
        var provider = new RecordingCheckoutProvider();
        await using var context = database.CreateContext();
        await using var checkoutTest1 = CreateTest(database, provider, CheckoutTime);
        var service = checkoutTest1.Service;

        var result = await service.CreateSessionAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 1,
            retryOperationId: null,
            CancellationToken.None);

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(result.BillingOperationId, Is.EqualTo(operation.Id));
            Assert.That(
                result.RedirectUri,
                Is.EqualTo(new Uri("https://checkout.stripe.test/session")));
            Assert.That(result.RequiredSeatQuantity, Is.EqualTo(1));
            Assert.That(
                operation.Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_test_checkout"));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
            Assert.That(provider.Requests[0].BillingOperationId, Is.EqualTo(operation.Id));
            Assert.That(
                provider.Requests[0].BillingAccountId,
                Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(provider.Requests[0].Cadence, Is.EqualTo(BillingCadence.Monthly));
            Assert.That(provider.Requests[0].SeatQuantity, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ProviderFailureReturnsReusableOperationWithoutLosingIt()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-retry",
            "checkout-service-retry@example.com");
        var provider = new RecordingCheckoutProvider { FailNextRequest = true };
        await using var context = database.CreateContext();
        await using var checkoutTest2 = CreateTest(database, provider, CheckoutTime);
        var service = checkoutTest2.Service;

        var unavailable = await service.CreateSessionAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Annual,
            seatQuantity: 1,
            retryOperationId: null,
            CancellationToken.None);
        var retryTime = CheckoutTime.AddMinutes(5);
        await using var checkoutTest3 = CreateTest(database, provider, retryTime);
        var retried = await checkoutTest3.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Annual,
                seatQuantity: 1,
                unavailable.BillingOperationId,
                CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                unavailable.Status,
                Is.EqualTo(BillingCheckoutStatus.ProviderUnavailable));
            Assert.That(unavailable.BillingOperationId, Is.Not.Null);
            Assert.That(unavailable.RedirectUri, Is.Null);
            Assert.That(retried.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(
                retried.BillingOperationId,
                Is.EqualTo(unavailable.BillingOperationId));
            Assert.That(
                provider.Requests.Select(request => request.BillingOperationId),
                Is.All.EqualTo(unavailable.BillingOperationId));
            Assert.That(
                provider.Requests[1].ExpiresAt - retryTime,
                Is.EqualTo(TimeSpan.FromMinutes(55)));
            Assert.That(context.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task RejectedPreparationNeverCallsProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-rejected",
            "checkout-service-rejected@example.com");
        var provider = new RecordingCheckoutProvider();
        await using var context = database.CreateContext();
        await using var checkoutTest4 = CreateTest(database, provider, CheckoutTime);
        var service = checkoutTest4.Service;

        var result = await service.CreateSessionAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 2,
            retryOperationId: null,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(BillingCheckoutStatus.PersonalSeatQuantityInvalid));
            Assert.That(result.RequiredSeatQuantity, Is.EqualTo(1));
            Assert.That(result.BillingOperationId, Is.Null);
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    [Test]
    public async Task RequestCancellationIsNotConvertedToProviderFailure()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-cancel",
            "checkout-service-cancel@example.com");
        using var cancellation = new CancellationTokenSource();
        var provider = new RecordingCheckoutProvider { Cancellation = cancellation };
        await using var context = database.CreateContext();
        await using var checkoutTest5 = CreateTest(database, provider, CheckoutTime);
        var service = checkoutTest5.Service;

        Assert.ThrowsAsync<TaskCanceledException>(
            async () => await service.CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                seatQuantity: 1,
                retryOperationId: null,
                cancellation.Token));

        Guid operationId;
        await using (var verification = database.CreateContext())
        {
            var operation = await verification.BillingOperations.SingleAsync();
            operationId = operation.Id;
            Assert.Multiple(() =>
            {
                Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
                Assert.That(
                    verification.AuditRecords.Count(record => record.TargetId == operationId),
                    Is.EqualTo(1));
            });
        }

        var recoveryProvider = new RecordingCheckoutProvider();
        await using var recoveryContext = database.CreateContext();
        await using var checkoutTest6 = CreateTest(database, recoveryProvider, CheckoutTime.AddMinutes(1));
        var recovered = await checkoutTest6.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(recovered.BillingOperationId, Is.EqualTo(operationId));
            Assert.That(recoveryProvider.Requests.Single().BillingOperationId,
                Is.EqualTo(operationId));
        });
    }

    [Test]
    public async Task NonHttpsProviderRedirectFailsClosed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-redirect",
            "checkout-service-redirect@example.com");
        var provider = new RecordingCheckoutProvider
        {
            Session = new BillingCheckoutProviderSession(
                "cs_insecure",
                new Uri("http://checkout.invalid/session")),
        };
        await using var context = database.CreateContext();
        await using var checkoutTest7 = CreateTest(database, provider, CheckoutTime);
        var service = checkoutTest7.Service;

        Assert.That(
            async () => await service.CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                seatQuantity: 1,
                retryOperationId: null,
                CancellationToken.None),
            Throws.InvalidOperationException);
    }

    [Test]
    public async Task PositiveQuantityIsValidatedBeforePersistence()
    {
        await using var test = await ServiceTestBase<BillingCheckoutService>.CreateAsync(CheckoutTime);
        Assert.That(async () => await test.Service.CreateSessionAsync(
            Guid.NewGuid(), Guid.NewGuid(), BillingCadence.Monthly, 0, null, CancellationToken.None),
            Throws.TypeOf<ArgumentOutOfRangeException>());
    }

    [Test]
    public async Task CompletedProviderSessionRecoversAMissedSubscriptionProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-projection-delay",
            "checkout-service-projection-delay@example.com");
        var provider = new RecordingCheckoutProvider();
        Guid billingOperationId;
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest8 = CreateTest(database, provider, CheckoutTime);
            var created = await checkoutTest8.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None);
            Assert.That(created.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            billingOperationId = created.BillingOperationId!.Value;
        }

        provider.SessionStatus = BillingCheckoutProviderSessionStatus.Complete;
        await using var delayedContext = database.CreateContext();
        var projectionTime = CheckoutTime.Add(BillingOperation.CheckoutLifetime);
        var subscriptionProvider = new StaticCommercialSubscriptionProvider(
            new AuthoritativeCommercialSubscription(
                signup.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_checkout_recovery",
                    "sub_checkout_recovery",
                    "price_checkout_recovery",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    CheckoutTime,
                    CheckoutTime.AddMonths(1),
                    projectionTime)));
        await using var checkoutTest9 = CreateTest(database, provider, projectionTime, subscriptionProvider);
        var delayed = await checkoutTest9.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Annual,
                1,
                retryOperationId: null,
                CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                delayed.Status,
                Is.EqualTo(BillingCheckoutStatus.SubscriptionAlreadyExists));
            Assert.That(provider.StatusRequests, Has.Count.EqualTo(1));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
            Assert.That(
                subscriptionProvider.RequestedBillingOperationId,
                Is.EqualTo(billingOperationId));
            Assert.That(delayedContext.BillingOperations.Count(), Is.EqualTo(1));
            Assert.That(
                delayedContext.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(delayedContext.CommercialSubscriptions.Count(), Is.EqualTo(1));
            Assert.That(
                delayedContext.Trials.Single().TerminatedAt,
                Is.EqualTo(projectionTime));
        });
    }

    [Test]
    public async Task CompletedSubscriptionForAnotherBillingAccountFailsClosed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var expectedBillingAccountId = Guid.CreateVersion7();
        var expectedBillingOperationId = Guid.CreateVersion7();
        var provider = new StaticCommercialSubscriptionProvider(
            new AuthoritativeCommercialSubscription(
                Guid.CreateVersion7(),
                new CommercialSubscriptionProjection(
                    "cus_wrong_account",
                    "sub_wrong_account",
                    "price_wrong_account",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    CheckoutTime,
                    CheckoutTime.AddMonths(1),
                    CheckoutTime)));
        await using var test = ServiceTestBase<BillingCheckoutCompletionReconciler>.ForDatabase(database, CheckoutTime, services =>
        {
            services.RemoveAll<ICommercialSubscriptionProvider>();
            services.AddSingleton<ICommercialSubscriptionProvider>(provider);
        });
        var reconciler = test.Service;

        Assert.That(
            async () => await reconciler.ReconcileAsync(
                expectedBillingAccountId,
                expectedBillingOperationId,
                "sub_wrong_account",
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(
            provider.RequestedBillingOperationId,
            Is.EqualTo(expectedBillingOperationId));
    }

    [Test]
    public async Task ProviderConfirmedExpiryAllowsExactlyOneReplacement()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-expired",
            "checkout-service-expired@example.com");
        var provider = new RecordingCheckoutProvider
        {
            SessionFactory = request => new BillingCheckoutProviderSession(
                $"cs_{request.BillingOperationId:N}",
                new Uri("https://checkout.stripe.test/session")),
        };
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest10 = CreateTest(database, provider, CheckoutTime);
            await checkoutTest10.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        provider.SessionStatus = BillingCheckoutProviderSessionStatus.Expired;
        await using var replacementContext = database.CreateContext();
        await using var checkoutTest11 = CreateTest(database, provider, CheckoutTime.Add(BillingOperation.CheckoutLifetime));
        var replacement = await checkoutTest11.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Annual,
                1,
                retryOperationId: null,
                CancellationToken.None);

        var operations = await replacementContext.BillingOperations
            .OrderBy(operation => operation.CreatedAt)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(replacement.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(operations, Has.Length.EqualTo(2));
            Assert.That(operations[0].Status, Is.EqualTo(BillingOperationStatus.Expired));
            Assert.That(
                operations[1].Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(provider.StatusRequests, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task LostLocalSessionRecordRecoversIdempotentProviderSuccessAtExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-lost-record",
            "checkout-service-lost-record@example.com");
        using var cancellation = new CancellationTokenSource();
        var provider = new RecordingCheckoutProvider
        {
            CancellationAfterSessionResponse = cancellation,
            SessionFactory = request => new BillingCheckoutProviderSession(
                $"cs_{request.BillingOperationId:N}",
                new Uri("https://checkout.stripe.test/session")),
        };
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest12 = CreateTest(database, provider, CheckoutTime);
            Assert.ThrowsAsync<OperationCanceledException>(
                async () => await checkoutTest12.Service
                    .CreateSessionAsync(
                        signup.UserId,
                        signup.PersonalBillingAccountId,
                        BillingCadence.Monthly,
                        1,
                        retryOperationId: null,
                        cancellation.Token));
            await using var verification = database.CreateContext();
            var pending = await verification.BillingOperations.SingleAsync();
            operationId = pending.Id;
            Assert.That(pending.Status, Is.EqualTo(BillingOperationStatus.Pending));
        }

        provider.CancellationAfterSessionResponse = null;
        provider.SessionStatus = BillingCheckoutProviderSessionStatus.Expired;
        var retryTime = CheckoutTime.Add(BillingOperation.CheckoutLifetime);
        await using var retryContext = database.CreateContext();
        await using var checkoutTest13 = CreateTest(database, provider, retryTime);
        var recovered = await checkoutTest13.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                operationId,
                CancellationToken.None);

        var operations = await retryContext.BillingOperations
            .OrderBy(operation => operation.CreatedAt)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(operations, Has.Length.EqualTo(2));
            Assert.That(operations[0].Status, Is.EqualTo(BillingOperationStatus.Expired));
            Assert.That(
                operations[1].Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(
                provider.Requests.Select(request => request.BillingOperationId),
                Is.EqualTo(new[] { operationId, operationId, operations[1].Id }));
        });
    }

    [Test]
    public async Task ProviderOpenSessionStillBlocksReplacementAfterLocalDeadline()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-provider-open",
            "checkout-service-provider-open@example.com");
        var provider = new RecordingCheckoutProvider();
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest14 = CreateTest(database, provider, CheckoutTime);
            await checkoutTest14.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        await using var retryContext = database.CreateContext();
        await using var checkoutTest15 = CreateTest(database, provider, CheckoutTime.Add(BillingOperation.CheckoutLifetime));
        var result = await checkoutTest15.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(BillingCheckoutStatus.CheckoutOperationInProgress));
            Assert.That(retryContext.BillingOperations.Count(), Is.EqualTo(1));
            Assert.That(provider.StatusRequests, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task SessionReconciliationOutageReturnsTheReusableOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-status-outage",
            "checkout-service-status-outage@example.com");
        var provider = new RecordingCheckoutProvider();
        Guid? operationId;
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest16 = CreateTest(database, provider, CheckoutTime);
            operationId = (await checkoutTest16.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None)).BillingOperationId;
        }

        provider.FailStatusRequest = true;
        await using var retryContext = database.CreateContext();
        await using var checkoutTest17 = CreateTest(database, provider, CheckoutTime.Add(BillingOperation.CheckoutLifetime));
        var result = await checkoutTest17.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                operationId,
                CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingCheckoutStatus.ProviderUnavailable));
            Assert.That(result.BillingOperationId, Is.EqualTo(operationId));
            Assert.That(retryContext.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task CompletedSubscriptionReconciliationOutageReturnsTheReusableOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-completion-outage",
            "checkout-service-completion-outage@example.com");
        var provider = new RecordingCheckoutProvider();
        Guid? operationId;
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest18 = CreateTest(database, provider, CheckoutTime);
            operationId = (await checkoutTest18.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None)).BillingOperationId;
        }

        provider.SessionStatus = BillingCheckoutProviderSessionStatus.Complete;
        provider.FailCompletionReconciliation = true;
        await using var retryContext = database.CreateContext();
        await using var checkoutTest19 = CreateTest(database, provider, CheckoutTime.Add(BillingOperation.CheckoutLifetime));
        var result = await checkoutTest19.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                operationId,
                CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingCheckoutStatus.ProviderUnavailable));
            Assert.That(result.BillingOperationId, Is.EqualTo(operationId));
            Assert.That(retryContext.BillingOperations.Count(), Is.EqualTo(1));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
            Assert.That(provider.StatusRequests, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task LateRetryReplacesOperationOnlyAfterDefiniteCreationRejection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-service-late-retry",
            "checkout-service-late-retry@example.com");
        var provider = new RecordingCheckoutProvider { FailNextRequest = true };
        Guid? operationId;
        await using (var context = database.CreateContext())
        {
            await using var checkoutTest20 = CreateTest(database, provider, CheckoutTime);
            operationId = (await checkoutTest20.Service
                .CreateSessionAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Monthly,
                    1,
                    retryOperationId: null,
                    CancellationToken.None)).BillingOperationId;
        }

        provider.RejectNextRequest = true;
        var retryTime = CheckoutTime.AddMinutes(31);
        await using var retryContext = database.CreateContext();
        await using var checkoutTest21 = CreateTest(database, provider, retryTime);
        var recovered = await checkoutTest21.Service
            .CreateSessionAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                operationId,
                CancellationToken.None);

        var operations = await retryContext.BillingOperations
            .OrderBy(operation => operation.CreatedAt)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingCheckoutStatus.Created));
            Assert.That(operations, Has.Length.EqualTo(2));
            Assert.That(operations[0].Status, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(
                operations[1].ExpiresAt,
                Is.EqualTo(retryTime.Add(BillingOperation.CheckoutLifetime)));
            Assert.That(recovered.BillingOperationId, Is.Not.EqualTo(operationId));
        });
    }

    private static ServiceTestBase<BillingCheckoutService> CreateTest(PostgresTestDatabase database,
        RecordingCheckoutProvider provider, DateTimeOffset observedAt,
        ICommercialSubscriptionProvider? subscriptionProvider = null)
    {
        return ServiceTestBase<BillingCheckoutService>.ForDatabase(database, observedAt, services =>
        {
            services.RemoveAll<IBillingCheckoutProvider>();
            services.AddSingleton<IBillingCheckoutProvider>(provider);
            services.RemoveAll<ICommercialSubscriptionProvider>();
            services.AddSingleton<ICommercialSubscriptionProvider>(subscriptionProvider ?? provider);
        });
    }

    private sealed class RecordingCheckoutProvider :
        IBillingCheckoutProvider,
        ICommercialSubscriptionProvider
    {
        public List<BillingCheckoutProviderRequest> Requests { get; } = [];

        public bool FailNextRequest { get; set; }

        public bool RejectNextRequest { get; set; }

        public BillingCheckoutProviderSessionStatus SessionStatus { get; set; } =
            BillingCheckoutProviderSessionStatus.Open;

        public string? ExternalSubscriptionId { get; set; } =
            "sub_checkout_recovery";

        public bool FailStatusRequest { get; set; }

        public bool FailCompletionReconciliation { get; set; }

        public List<string> StatusRequests { get; } = [];

        public CancellationTokenSource? Cancellation { get; init; }

        public CancellationTokenSource? CancellationAfterSessionResponse { get; set; }

        public BillingCheckoutProviderSession? Session { get; init; }

        public Func<BillingCheckoutProviderRequest, BillingCheckoutProviderSession>?
            SessionFactory
        { get; init; }

        public Task<BillingCheckoutProviderSession> CreateSessionAsync(
            BillingCheckoutProviderRequest request,
            CancellationToken cancellationToken)
        {
            Requests.Add(request);
            if (Cancellation is not null)
            {
                Cancellation.Cancel();
                return Task.FromCanceled<BillingCheckoutProviderSession>(cancellationToken);
            }

            if (FailNextRequest)
            {
                FailNextRequest = false;
                throw new BillingCheckoutProviderUnavailableException(
                    "The provider is unavailable.");
            }

            if (RejectNextRequest)
            {
                RejectNextRequest = false;
                throw new BillingCheckoutProviderSessionCreationRejectedException(
                    "The provider rejected the expired request.");
            }

            var session =
                SessionFactory?.Invoke(request)
                ?? Session
                ?? new BillingCheckoutProviderSession(
                    "cs_test_checkout",
                    new Uri("https://checkout.stripe.test/session"));
            CancellationAfterSessionResponse?.Cancel();
            return Task.FromResult(session);
        }

        public Task<BillingCheckoutProviderSessionState> GetSessionStateAsync(
            string externalSessionId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            StatusRequests.Add(externalSessionId);
            if (FailStatusRequest)
            {
                throw new BillingCheckoutProviderUnavailableException(
                    "The provider is unavailable.");
            }

            return Task.FromResult(SessionStatus switch
            {
                BillingCheckoutProviderSessionStatus.Open =>
                    BillingCheckoutProviderSessionState.Open,
                BillingCheckoutProviderSessionStatus.Complete =>
                    BillingCheckoutProviderSessionState.Complete(
                        ExternalSubscriptionId
                            ?? throw new InvalidOperationException(
                                "A completed test session requires a subscription.")),
                BillingCheckoutProviderSessionStatus.Expired =>
                    BillingCheckoutProviderSessionState.Expired,
                _ => throw new InvalidOperationException("Unsupported test session state."),
            });
        }

        public Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
            string externalEventId, BillingWebhookEventKind kind, CancellationToken cancellationToken)
        {
            throw new AssertionException("Checkout must resolve the subscription directly.");
        }

        public Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
            string externalSubscriptionId,
            Guid expectedBillingOperationId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (FailCompletionReconciliation)
            {
                throw new BillingCheckoutProviderUnavailableException(
                    "The subscription lookup is unavailable.");
            }

            throw new InvalidOperationException(
                "No completed Checkout reconciliation was configured for this test.");
        }
    }

    private sealed class StaticCommercialSubscriptionProvider(
        AuthoritativeCommercialSubscription subscription)
        : ICommercialSubscriptionProvider
    {
        public Guid? RequestedBillingOperationId { get; private set; }

        public Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
            string externalEventId,
            BillingWebhookEventKind kind,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException();
        }

        public Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
            string externalSubscriptionId,
            Guid expectedBillingOperationId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            RequestedBillingOperationId = expectedBillingOperationId;
            return Task.FromResult(subscription);
        }
    }

}
